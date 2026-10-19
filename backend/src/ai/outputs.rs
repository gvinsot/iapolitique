//! Typed outputs of Mistral (AGENTS.md §6.2, §13.4) and the checks that a JSON schema cannot
//! express. The answers are first validated against their schema of `prompts/`, then
//! deserialized here, then checked: a failed check is an invalid output, retried like a schema
//! violation (§6.1).
//!
//! Sources (INJ-3): the analysis model never writes a URL. The pages that the web research
//! really consulted (its `tool_reference` chunks) and the sources of the analysis being replaced
//! are numbered `R1`, `R2`… ([`ConsultedPages`]) in the structuring prompt; the answer
//! ([`AnalysisDraft`]) cites them by that identifier (`ref`), and the backend fills the title,
//! the publisher and the URL of each published source from that list ([`AnalysisDraft::resolve`]).
//! A URL written in an idea, a contribution, a memo or a page's text can therefore never become
//! a source.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::LazyLock;

use regex::Regex;
use serde::{Deserialize, Serialize};

use super::mistral::Rejection;
use super::prompts::{Criterion, Taxonomy};
use crate::domain::money::EurRange;
use crate::domain::types::{
    AdminLevel, Confidence, Decision, IdeaKind, ImpactDirection, LegalVehicle, ReviewOutcome,
};

/// Longest accepted delay before effects (`analyses.time_to_effect_months` CHECK).
pub const MAX_TIME_TO_EFFECT_MONTHS: u32 = 600;
/// `analyses.territory_code` CHECK.
static TERRITORY_CODE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[0-9A-Z]{1,12}$").expect("valid regex"));
static SOURCE_ID_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^S[1-9][0-9]*$").expect("valid regex"));
static SOURCE_REF_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\[(S[0-9]+)\]").expect("valid regex"));
static PAGE_ID_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^R([1-9][0-9]{0,3})$").expect("valid regex"));

/// Longest accepted title of an analysis (also the source of its slug).
pub const MAX_TITLE_CHARS: usize = 200;
/// Longest accepted summary of an analysis.
pub const MAX_SUMMARY_CHARS: usize = 1_500;
/// Longest accepted long text of an analysis (proposal, calculation, explanation…).
pub const MAX_LONG_TEXT_CHARS: usize = 6_000;
/// Longest accepted list item or short explanation.
pub const MAX_ITEM_CHARS: usize = 1_500;
/// Longest accepted source field (title, publisher, use).
pub const MAX_SOURCE_FIELD_CHARS: usize = 500;

/// Checks a generated text before it is stored and published (INJ-6): no control character
/// but `\n` and `\t` (PostgreSQL refuses NUL, and a failed storage would rerun the paid
/// analysis), no bidi override or invisible format character (spoofed titles), and at most
/// `max_chars` characters.
fn check_ai_text(field: &str, value: &str, max_chars: usize, problems: &mut Vec<String>) {
    if let Some(c) = value.chars().find(|&c| {
        (c.is_control() && c != '\n' && c != '\t')
            || crate::domain::validation::is_invisible_format_char(c)
    }) {
        problems.push(format!(
            "{field}: forbidden character U+{:04X}",
            u32::from(c)
        ));
    }
    let len = value.chars().count();
    if len > max_chars {
        problems.push(format!("{field}: {len} characters, at most {max_chars}"));
    }
}

/// Longest accepted source URL.
pub const MAX_SOURCE_URL_LEN: usize = 2_048;

/// Comparison key of a source URL (INJ-3): host and path (without trailing `/`), and the query
/// without `utm_*` parameters; scheme, default port and fragment do not count. `Err` when the
/// URL cannot be published: not http(s), credentials, explicit non-default port, IP address
/// instead of a host name, or longer than [`MAX_SOURCE_URL_LEN`].
pub fn source_url_key(raw: &str) -> Result<String, &'static str> {
    if raw.len() > MAX_SOURCE_URL_LEN {
        return Err("URL too long");
    }
    let url = url::Url::parse(raw.trim()).map_err(|_| "invalid URL")?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err("not an http(s) URL");
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err("URL with credentials");
    }
    if url.port().is_some() {
        return Err("URL with an explicit port");
    }
    let host = match url.host() {
        Some(url::Host::Domain(host)) => host.trim_end_matches('.').to_ascii_lowercase(),
        Some(_) => return Err("IP address instead of a host name"),
        None => return Err("URL without host"),
    };
    let query: Vec<(String, String)> = url
        .query_pairs()
        .filter(|(key, _)| !key.to_ascii_lowercase().starts_with("utm_"))
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();
    let mut key = format!("{host}{}", url.path().trim_end_matches('/'));
    if !query.is_empty() {
        key.push('?');
        key.push_str(
            &url::form_urlencoded::Serializer::new(String::new())
                .extend_pairs(query)
                .finish(),
        );
    }
    Ok(key)
}

/// A page that an analysis may cite (INJ-3), as published: its title as the web search gave
/// it (cleaned), its site as publisher, its URL.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConsultedPage {
    pub title: String,
    pub publisher: String,
    pub url: String,
}

/// Site of a source URL, shown as its publisher: the host name without `www.`.
fn site_of(url: &str) -> String {
    let host = url::Url::parse(url.trim())
        .ok()
        .and_then(|u| {
            u.host_str()
                .map(|h| h.trim_end_matches('.').to_ascii_lowercase())
        })
        .unwrap_or_default();
    host.strip_prefix("www.").unwrap_or(&host).to_owned()
}

/// A title from the web (untrusted, INJ-6): no control, bidi or invisible character, spaces
/// collapsed, at most [`MAX_SOURCE_FIELD_CHARS`] characters.
fn clean_title(title: &str) -> String {
    let visible: String = title
        .chars()
        .filter(|&c| !crate::domain::validation::is_invisible_format_char(c))
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    let words = visible.split_whitespace().collect::<Vec<_>>().join(" ");
    if words.chars().count() <= MAX_SOURCE_FIELD_CHARS {
        return words;
    }
    let mut short: String = words.chars().take(MAX_SOURCE_FIELD_CHARS - 1).collect();
    short.push('…');
    short
}

/// The pages an analysis may cite (INJ-3), numbered `R1`, `R2`… in this order: the pages
/// consulted by its web research (`tool_reference` chunks), then the sources of the analysis it
/// replaces. One entry per page ([`source_url_key`]); a URL that cannot be published (not
/// http(s), credentials, explicit port, IP address, too long, control character) is left out.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ConsultedPages(Vec<ConsultedPage>);

impl ConsultedPages {
    /// `pages`: (title, URL) pairs, in order.
    pub fn new<'a>(pages: impl IntoIterator<Item = (&'a str, &'a str)>) -> Self {
        let mut keys = BTreeSet::new();
        let mut list = Vec::new();
        for (title, url) in pages {
            let url = url.trim();
            let Ok(key) = source_url_key(url) else {
                continue;
            };
            let mut problems = Vec::new();
            check_ai_text("url", url, MAX_SOURCE_URL_LEN, &mut problems);
            if !problems.is_empty() || !keys.insert(key) {
                continue;
            }
            let publisher = clean_title(&site_of(url));
            let title = match clean_title(title) {
                title if title.is_empty() => publisher.clone(),
                title => title,
            };
            list.push(ConsultedPage {
                title,
                publisher,
                url: url.to_owned(),
            });
        }
        Self(list)
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// `R1`, `R2`…, in order.
    pub fn ids(&self) -> Vec<String> {
        (1..=self.0.len()).map(|n| format!("R{n}")).collect()
    }

    /// The page of an identifier `Rn`, if any.
    pub fn get(&self, id: &str) -> Option<&ConsultedPage> {
        let n: usize = PAGE_ID_RE.captures(id)?.get(1)?.as_str().parse().ok()?;
        self.0.get(n.checked_sub(1)?)
    }

    /// `{{pages}}` of `analysis.md`: « Rn — titre — site — adresse », one page per line.
    pub fn for_prompt(&self) -> String {
        self.0
            .iter()
            .zip(self.ids())
            .map(|(page, id)| format!("{id} — {} — {} — {}", page.title, page.publisher, page.url))
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// `territory`: two empty strings when no specific local authority is targeted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Territory {
    pub name: String,
    pub insee_code: String,
}

/// `annual_gain_eur`, `implementation_cost_eur`: whole euros.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Estimate {
    pub low: i64,
    pub median: i64,
    pub high: i64,
    pub calculation: String,
}

impl Estimate {
    pub fn range(&self) -> EurRange {
        EurRange {
            low: self.low,
            median: self.median,
            high: self.high,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Feasibility {
    pub score: u8,
    pub obstacles: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Impact {
    pub effect: ImpactDirection,
    pub explanation: String,
}

/// A published source (§6.2): `title`, `publisher` and `url` come from [`ConsultedPages`], never
/// from the model. Also the shape of the analyses stored before the citation by identifier.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Source {
    pub id: String,
    pub title: String,
    pub publisher: String,
    pub url: String,
    pub used_for: String,
}

/// A source as the model gives it (`prompts/analysis.schema.json`): the identifier cited in
/// the texts (`S1`…), the consulted page it is (`R1`…), and what it supports.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceCitation {
    pub id: String,
    #[serde(rename = "ref")]
    pub page: String,
    pub used_for: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContributionConsidered {
    pub contribution_id: String,
    pub summary: String,
    pub taken_into_account: bool,
    pub response: String,
}

/// An analysis (§6.2), as published (`S` = [`Source`], stored in `analyses.content`) or as the
/// model answers it ([`AnalysisDraft`]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnalysisOutput<S = Source> {
    pub title: String,
    pub summary: String,
    pub improved_proposal: String,
    pub existing_situation: String,
    pub admin_level: AdminLevel,
    pub territory: Territory,
    pub category: String,
    pub kind: IdeaKind,
    pub annual_gain_eur: Estimate,
    pub implementation_cost_eur: Estimate,
    pub time_to_effect_months: u32,
    pub legal_vehicle: LegalVehicle,
    pub feasibility: Feasibility,
    pub confidence: Confidence,
    pub confidence_explanation: String,
    pub assumptions: Vec<String>,
    pub pros: Vec<String>,
    pub cons: Vec<String>,
    pub risks: Vec<String>,
    /// Public of `taxonomy.yaml` → impact.
    pub impacts: BTreeMap<String, Impact>,
    pub sources: Vec<S>,
    pub contributions_considered: Vec<ContributionConsidered>,
    pub changes_since_previous: String,
}

/// Output of `prompts/analysis.md`: sources cite the consulted pages by identifier.
pub type AnalysisDraft = AnalysisOutput<SourceCitation>;

impl<S> AnalysisOutput<S> {
    /// The same analysis with other sources.
    fn with_sources<T>(self, sources: Vec<T>) -> AnalysisOutput<T> {
        AnalysisOutput {
            title: self.title,
            summary: self.summary,
            improved_proposal: self.improved_proposal,
            existing_situation: self.existing_situation,
            admin_level: self.admin_level,
            territory: self.territory,
            category: self.category,
            kind: self.kind,
            annual_gain_eur: self.annual_gain_eur,
            implementation_cost_eur: self.implementation_cost_eur,
            time_to_effect_months: self.time_to_effect_months,
            legal_vehicle: self.legal_vehicle,
            feasibility: self.feasibility,
            confidence: self.confidence,
            confidence_explanation: self.confidence_explanation,
            assumptions: self.assumptions,
            pros: self.pros,
            cons: self.cons,
            risks: self.risks,
            impacts: self.impacts,
            sources,
            contributions_considered: self.contributions_considered,
            changes_since_previous: self.changes_since_previous,
        }
    }

    /// Characters and lengths of every generated string but the sources (INJ-6).
    fn check_texts(&self, problems: &mut Vec<String>) {
        let p = problems;
        check_ai_text("title", &self.title, MAX_TITLE_CHARS, p);
        check_ai_text("summary", &self.summary, MAX_SUMMARY_CHARS, p);
        for (field, value) in [
            ("improved_proposal", &self.improved_proposal),
            ("existing_situation", &self.existing_situation),
            (
                "annual_gain_eur.calculation",
                &self.annual_gain_eur.calculation,
            ),
            (
                "implementation_cost_eur.calculation",
                &self.implementation_cost_eur.calculation,
            ),
            ("confidence_explanation", &self.confidence_explanation),
            ("changes_since_previous", &self.changes_since_previous),
        ] {
            check_ai_text(field, value, MAX_LONG_TEXT_CHARS, p);
        }
        check_ai_text("territory.name", &self.territory.name, MAX_TITLE_CHARS, p);
        check_ai_text("category", &self.category, MAX_TITLE_CHARS, p);
        for (field, list) in [
            ("feasibility.obstacles", &self.feasibility.obstacles),
            ("assumptions", &self.assumptions),
            ("pros", &self.pros),
            ("cons", &self.cons),
            ("risks", &self.risks),
        ] {
            for item in list {
                check_ai_text(field, item, MAX_ITEM_CHARS, p);
            }
        }
        for (public, impact) in &self.impacts {
            check_ai_text("impacts", public, MAX_SOURCE_FIELD_CHARS, p);
            check_ai_text(
                "impacts.explanation",
                &impact.explanation,
                MAX_ITEM_CHARS,
                p,
            );
        }
        for c in &self.contributions_considered {
            check_ai_text(
                "contributions_considered.summary",
                &c.summary,
                MAX_ITEM_CHARS,
                p,
            );
            check_ai_text(
                "contributions_considered.response",
                &c.response,
                MAX_ITEM_CHARS,
                p,
            );
        }
    }

    /// Every generated text that may cite sources.
    fn texts(&self) -> Vec<&str> {
        let mut texts = vec![
            self.title.as_str(),
            &self.summary,
            &self.improved_proposal,
            &self.existing_situation,
            &self.annual_gain_eur.calculation,
            &self.implementation_cost_eur.calculation,
            &self.confidence_explanation,
            &self.changes_since_previous,
        ];
        for list in [
            &self.feasibility.obstacles,
            &self.assumptions,
            &self.pros,
            &self.cons,
            &self.risks,
        ] {
            texts.extend(list.iter().map(String::as_str));
        }
        texts.extend(self.impacts.values().map(|i| i.explanation.as_str()));
        for c in &self.contributions_considered {
            texts.push(&c.summary);
            texts.push(&c.response);
        }
        texts
    }

    /// Checks of §13.4 that do not concern the sources.
    fn check_content(
        &self,
        taxonomy: &Taxonomy,
        contribution_ids: &[String],
        problems: &mut Vec<String>,
    ) {
        if self.title.trim().is_empty() || self.summary.trim().is_empty() {
            problems.push("empty title or summary".to_owned());
        }
        self.check_texts(problems);
        if taxonomy.category(&self.category).is_none() {
            problems.push(format!("unknown category `{}`", self.category));
        } else if !taxonomy.category_allowed(self.admin_level.as_str(), &self.category) {
            problems.push(format!(
                "category `{}` not allowed for admin level `{}`",
                self.category, self.admin_level
            ));
        }
        for (name, estimate) in [
            ("annual_gain_eur", &self.annual_gain_eur),
            ("implementation_cost_eur", &self.implementation_cost_eur),
        ] {
            if let Err(err) = estimate.range().validate() {
                problems.push(format!("{name}: {err}"));
            }
        }
        if self.time_to_effect_months > MAX_TIME_TO_EFFECT_MONTHS {
            problems.push(format!(
                "time_to_effect_months above {MAX_TIME_TO_EFFECT_MONTHS}"
            ));
        }
        if !(1..=5).contains(&self.feasibility.score) {
            problems.push("feasibility.score must be between 1 and 5".to_owned());
        }
        let territory = &self.territory;
        if territory.name.trim().is_empty() && !territory.insee_code.trim().is_empty() {
            problems.push("territory code without a name".to_owned());
        }
        if !territory.insee_code.is_empty() && !TERRITORY_CODE_RE.is_match(&territory.insee_code) {
            problems.push("territory.insee_code must be 1 to 12 digits or capitals".to_owned());
        }
        let publics: BTreeSet<&str> = taxonomy
            .impact_publics
            .iter()
            .map(|p| p.key.as_str())
            .collect();
        let answered: BTreeSet<&str> = self.impacts.keys().map(String::as_str).collect();
        if publics != answered {
            problems.push("impacts must cover exactly the publics of the taxonomy".to_owned());
        }
        let unknown_contributions: Vec<&str> = self
            .contributions_considered
            .iter()
            .map(|c| c.contribution_id.as_str())
            .filter(|id| !contribution_ids.iter().any(|k| k == id))
            .collect();
        if !unknown_contributions.is_empty() {
            problems.push(format!(
                "unknown contributions considered: {unknown_contributions:?}"
            ));
        }
    }

    /// `[Sx]` cited in the texts.
    fn cited(&self) -> BTreeSet<&str> {
        self.texts()
            .into_iter()
            .flat_map(|t| SOURCE_REF_RE.captures_iter(t))
            .filter_map(|c| c.get(1).map(|m| m.as_str()))
            .collect()
    }
}

impl AnalysisDraft {
    /// Checks of §13.4 beyond the schema. `contribution_ids` are the contributions given to the
    /// prompt: the analysis may only report on those. `pages` are the pages it may cite, by
    /// identifier. A source problem (unknown page, `[Sx]` cited without its source) comes with
    /// a feedback for the next attempt that lists the valid identifiers: the model never wrote
    /// a URL, so a wrong identifier is a slip to correct, not a planted page.
    pub fn check(
        &self,
        taxonomy: &Taxonomy,
        contribution_ids: &[String],
        pages: &ConsultedPages,
    ) -> Result<(), Rejection> {
        let mut problems = Vec::new();
        self.check_content(taxonomy, contribution_ids, &mut problems);
        // Feedback lines: built from validated identifiers only, never from model text.
        let mut feedback = Vec::new();
        if self.sources.is_empty() {
            problems.push("at least one source is required".to_owned());
            feedback.push("`sources` est vide : au moins une source est requise".to_owned());
        }
        let mut ids = BTreeSet::new();
        for source in &self.sources {
            check_ai_text(
                "sources.id",
                &source.id,
                MAX_SOURCE_FIELD_CHARS,
                &mut problems,
            );
            check_ai_text(
                "sources.ref",
                &source.page,
                MAX_SOURCE_FIELD_CHARS,
                &mut problems,
            );
            check_ai_text(
                "sources.used_for",
                &source.used_for,
                MAX_SOURCE_FIELD_CHARS,
                &mut problems,
            );
            let valid_id = SOURCE_ID_RE.is_match(&source.id);
            if !valid_id {
                problems.push(format!("invalid source id `{}`", source.id));
            }
            if !ids.insert(source.id.as_str()) {
                problems.push(format!("duplicate source id `{}`", source.id));
            }
            if pages.get(&source.page).is_none() {
                problems.push(format!(
                    "source {}: not a consulted page",
                    if valid_id { source.id.as_str() } else { "?" }
                ));
                feedback.push(if valid_id {
                    format!(
                        "la source {} indique dans `ref` une page absente de <pages_consultees>",
                        source.id
                    )
                } else {
                    "une source indique dans `ref` une page absente de <pages_consultees>"
                        .to_owned()
                });
            }
        }
        let unknown: Vec<&str> = self.cited().difference(&ids).copied().collect();
        if !unknown.is_empty() {
            problems.push(format!("cited sources missing from sources: {unknown:?}"));
            let list: Vec<String> = unknown.iter().map(|id| format!("[{id}]")).collect();
            feedback.push(format!(
                "{} cité(s) dans les textes sans entrée dans `sources`",
                list.join(", ")
            ));
        }
        if problems.is_empty() {
            return Ok(());
        }
        let feedback = (!feedback.is_empty()).then(|| {
            format!(
                "<retour_validation>\nTa réponse précédente a été refusée :\n{}\nIdentifiants \
                 de pages valides : {}. Chaque source porte dans `ref` l'un de ces identifiants, \
                 et chaque référence [Sx] citée dans les textes figure dans `sources`. Corrige ces \
                 points et réponds de nouveau par l'objet JSON complet.\n</retour_validation>",
                feedback
                    .iter()
                    .map(|line| format!("- {line} ;"))
                    .collect::<Vec<_>>()
                    .join("\n"),
                pages.ids().join(", ")
            )
        });
        Err(Rejection {
            problem: problems.join("; "),
            feedback,
        })
    }

    /// The analysis to publish: each source takes the title, the publisher and the URL of the
    /// page it cites. To call on a draft that passed [`AnalysisDraft::check`] with `pages`; a
    /// source whose page is unknown is dropped.
    pub fn resolve(self, pages: &ConsultedPages) -> AnalysisOutput {
        let sources = self
            .sources
            .iter()
            .filter_map(|source| {
                pages.get(&source.page).map(|page| Source {
                    id: source.id.clone(),
                    title: page.title.clone(),
                    publisher: page.publisher.clone(),
                    url: page.url.clone(),
                    used_for: source.used_for.clone(),
                })
            })
            .collect();
        self.with_sources(sources)
    }
}

/// Output of `prompts/second_opinion.md`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SecondOpinionOutput {
    pub decision: Decision,
    /// `none` when accepted, otherwise a `reason_code` of the section.
    pub reason_code: String,
    pub explanation: String,
}

impl SecondOpinionOutput {
    /// The reason must be `none` for an acceptance, a reason code of the screened section for
    /// a rejection.
    pub fn check(&self, section: &[Criterion]) -> Result<(), String> {
        if self.explanation.trim().is_empty() {
            return Err("empty explanation".to_owned());
        }
        let mut problems = Vec::new();
        check_ai_text(
            "explanation",
            &self.explanation,
            MAX_LONG_TEXT_CHARS,
            &mut problems,
        );
        if !problems.is_empty() {
            return Err(problems.join("; "));
        }
        match self.decision {
            Decision::Accept if self.reason_code != "none" => {
                Err("an acceptance needs reason_code none".to_owned())
            }
            Decision::Reject if !section.iter().any(|c| c.reason_code == self.reason_code) => Err(
                format!("reason_code `{}` is not in the section", self.reason_code),
            ),
            _ => Ok(()),
        }
    }

    /// Reason code of a rejection.
    pub fn rejection_reason(&self) -> Option<&str> {
        (self.decision == Decision::Reject).then_some(self.reason_code.as_str())
    }
}

/// Output of `prompts/report_review.md`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReportReviewOutput {
    pub outcome: ReviewOutcome,
    /// Serious content (defamation, threat, hatred, terrorism, child abuse): hidden at once and
    /// queued for human escalation (§7.5).
    pub escalate: bool,
    pub explanation: String,
    /// Instructions for the re-analysis when `update`, empty otherwise.
    pub corrections: String,
}

impl ReportReviewOutput {
    /// An update must say what to correct.
    pub fn check(&self) -> Result<(), String> {
        if self.explanation.trim().is_empty() {
            return Err("empty explanation".to_owned());
        }
        let mut problems = Vec::new();
        check_ai_text(
            "explanation",
            &self.explanation,
            MAX_LONG_TEXT_CHARS,
            &mut problems,
        );
        check_ai_text(
            "corrections",
            &self.corrections,
            MAX_LONG_TEXT_CHARS,
            &mut problems,
        );
        if !problems.is_empty() {
            return Err(problems.join("; "));
        }
        if self.outcome == ReviewOutcome::Update
            && !self.escalate
            && self.corrections.trim().is_empty()
        {
            return Err("an update needs corrections".to_owned());
        }
        Ok(())
    }

    /// Applies the rules of the schema: an escalation always hides the idea, and corrections
    /// only accompany an update (`report_reviews` CHECK).
    pub fn normalized(mut self) -> Self {
        if self.escalate {
            self.outcome = ReviewOutcome::Hide;
        }
        if self.outcome == ReviewOutcome::Update {
            self.corrections = self.corrections.trim().to_owned();
        } else {
            self.corrections.clear();
        }
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::prompts::test_prompts;
    use serde_json::{Value, json};

    /// A published analysis, as stored before and after the citation by identifier.
    fn analysis_json() -> Value {
        serde_json::from_str(include_str!("../../tests/fixtures/mistral/analysis.json")).unwrap()
    }

    /// The model's answer: the same analysis, sources citing R1 and R2.
    fn draft_json() -> Value {
        serde_json::from_str(include_str!(
            "../../tests/fixtures/mistral/analysis_draft.json"
        ))
        .unwrap()
    }

    fn draft() -> AnalysisDraft {
        serde_json::from_value(draft_json()).unwrap()
    }

    const CCOMPTES: &str =
        "https://www.ccomptes.fr/fr/publications/la-fonction-achats-des-hopitaux";
    const SANTE: &str = "https://sante.gouv.fr/groupements-hospitaliers-de-territoire";

    /// The pages of the fixture, as the research would have returned them.
    fn pages() -> ConsultedPages {
        ConsultedPages::new([
            ("La fonction achats des hôpitaux publics", CCOMPTES),
            ("Les groupements hospitaliers de territoire", SANTE),
        ])
    }

    fn check(draft: &AnalysisDraft) -> Result<(), String> {
        draft
            .check(&test_prompts().taxonomy, &["c1".to_owned()], &pages())
            .map_err(|rejection| rejection.problem)
    }

    #[test]
    fn consulted_pages_are_numbered_cleaned_and_deduplicated() {
        let pages = ConsultedPages::new([
            ("La fonction achats\n des hôpitaux\u{202E} ", CCOMPTES),
            // The same page (trailing slash, fragment, utm_*, http): left out.
            (
                "Doublon",
                "http://www.ccomptes.fr/fr/publications/la-fonction-achats-des-hopitaux/#a",
            ),
            ("", SANTE),
            // Not publishable: left out.
            ("Locale", "https://192.168.1.1/x"),
            ("Script", "javascript:alert(1)"),
            ("Port", "https://ccomptes.fr:8443/x"),
            ("Contrôle", "https://example.org/a\u{0007}b"),
            (
                "t".repeat(MAX_SOURCE_FIELD_CHARS + 10).as_str(),
                "https://Example.ORG/page",
            ),
        ]);
        assert_eq!(pages.len(), 3);
        assert_eq!(pages.ids(), ["R1", "R2", "R3"]);
        let first = pages.get("R1").unwrap();
        assert_eq!(first.title, "La fonction achats des hôpitaux");
        assert_eq!(first.publisher, "ccomptes.fr");
        assert_eq!(first.url, CCOMPTES);
        // No title: the site stands for it.
        assert_eq!(pages.get("R2").unwrap().title, "sante.gouv.fr");
        let long = pages.get("R3").unwrap();
        assert_eq!(long.title.chars().count(), MAX_SOURCE_FIELD_CHARS);
        assert!(long.title.ends_with('…'));
        assert_eq!(long.publisher, "example.org");
        for unknown in ["R0", "R4", "R01", "r1", "S1", "R", CCOMPTES, ""] {
            assert!(pages.get(unknown).is_none(), "{unknown}");
        }
        assert_eq!(
            pages.for_prompt().lines().next(),
            Some(
                format!("R1 — La fonction achats des hôpitaux — ccomptes.fr — {CCOMPTES}").as_str()
            )
        );
        assert!(ConsultedPages::new([]).is_empty());
    }

    /// The published sources take title, publisher and URL from the consulted pages; the
    /// stored shape does not change, so the analyses published before keep being read
    /// (portal, exports, previous analysis of a re-analysis).
    #[test]
    fn a_checked_draft_resolves_to_the_published_shape() {
        check(&draft()).unwrap();
        let published = draft().resolve(&pages());
        assert_eq!(
            published.sources,
            [
                Source {
                    id: "S1".into(),
                    title: "La fonction achats des hôpitaux publics".into(),
                    publisher: "ccomptes.fr".into(),
                    url: CCOMPTES.into(),
                    used_for: "Montant des achats hospitaliers".into(),
                },
                Source {
                    id: "S2".into(),
                    title: "Les groupements hospitaliers de territoire".into(),
                    publisher: "sante.gouv.fr".into(),
                    url: SANTE.into(),
                    used_for: "Organisation de la fonction achat".into(),
                },
            ]
        );
        let value = serde_json::to_value(&published).unwrap();
        let keys = |v: &Value| -> Vec<String> {
            v.as_object().unwrap().keys().cloned().collect::<Vec<_>>()
        };
        let stored = analysis_json();
        assert_eq!(keys(&value), keys(&stored));
        assert_eq!(keys(&value["sources"][0]), keys(&stored["sources"][0]));
        // An analysis stored before still deserializes as a published one.
        let old: AnalysisOutput = serde_json::from_value(stored).unwrap();
        assert_eq!(old.sources[0].publisher, "Cour des comptes");
        assert_eq!(old.title, published.title);
    }

    /// INJ-3: the model never writes a URL. A wrong identifier (or a URL in its place) is
    /// refused with a feedback that lists the valid identifiers, built from validated
    /// identifiers only.
    #[test]
    fn sources_cite_consulted_pages_by_identifier() {
        for wrong in ["R3", "R0", CCOMPTES, "https://evil.example/plante", "S1"] {
            let mut draft = draft();
            draft.sources[1].page = wrong.into();
            let rejection = draft
                .check(&test_prompts().taxonomy, &["c1".to_owned()], &pages())
                .unwrap_err();
            assert!(
                rejection
                    .problem
                    .contains("source S2: not a consulted page"),
                "{wrong}"
            );
            assert!(
                !rejection.problem.contains(wrong),
                "{wrong}: {}",
                rejection.problem
            );
            let feedback = rejection.feedback.unwrap();
            assert!(feedback.starts_with("<retour_validation>"), "{feedback}");
            assert!(
                feedback.contains("la source S2 indique dans `ref`"),
                "{feedback}"
            );
            assert!(
                feedback.contains("Identifiants de pages valides : R1, R2."),
                "{feedback}"
            );
            assert!(!feedback.contains(wrong), "{wrong}: {feedback}");
        }
        // An invalid id is not echoed in the feedback.
        let mut draft = draft();
        draft.sources[1].id = "<b>".into();
        draft.sources[1].page = "R9".into();
        let rejection = draft
            .check(&test_prompts().taxonomy, &["c1".to_owned()], &pages())
            .unwrap_err();
        let feedback = rejection.feedback.unwrap();
        assert!(
            feedback.contains("une source indique dans `ref`"),
            "{feedback}"
        );
        assert!(!feedback.contains("<b>"), "{feedback}");
        // [Sx] cited without its source: the same feedback path.
        let mut draft = super::tests::draft();
        draft.pros.push("Selon [S9].".into());
        let rejection = draft
            .check(&test_prompts().taxonomy, &["c1".to_owned()], &pages())
            .unwrap_err();
        assert!(rejection.feedback.unwrap().contains("[S9] cité(s)"));
        // Nothing to cite: refused.
        assert!(
            super::tests::draft()
                .check(
                    &test_prompts().taxonomy,
                    &["c1".to_owned()],
                    &ConsultedPages::default()
                )
                .is_err()
        );
        // Other problems: no feedback, the same prompt again.
        let mut draft = super::tests::draft();
        draft.category = "lune".into();
        let rejection = draft
            .check(&test_prompts().taxonomy, &["c1".to_owned()], &pages())
            .unwrap_err();
        assert_eq!(rejection.feedback, None);
    }

    /// INJ-3: whatever the model answers, a published source URL is one of the consulted pages;
    /// a URL found only in the idea text, in the memo or in a page's text cannot become one.
    #[test]
    fn a_published_url_is_always_a_consulted_page() {
        let planted = "https://evil.example/plante";
        // A page whose title carries a URL keeps it as plain title text.
        let pages = ConsultedPages::new([
            (format!("Rapport, voir {planted}").as_str(), CCOMPTES),
            ("Page", SANTE),
        ]);
        let consulted: BTreeSet<&str> = [CCOMPTES, SANTE].into();
        for page in ["R1", "R2", "R3", planted, "", "R1 ", " R2"] {
            let mut draft = draft();
            for source in &mut draft.sources {
                source.page = page.into();
            }
            draft.summary = format!("Selon {planted} [S1].");
            let accepted = draft
                .check(&test_prompts().taxonomy, &["c1".to_owned()], &pages)
                .is_ok();
            let published = draft.resolve(&pages);
            assert_eq!(accepted, page == "R1" || page == "R2", "{page:?}");
            for source in &published.sources {
                assert!(
                    consulted.contains(source.url.as_str()),
                    "{page:?}: {}",
                    source.url
                );
            }
        }
    }

    #[test]
    fn the_schema_constrains_the_reference_per_call() {
        let schema = test_prompts().analysis.output_schema.clone().unwrap();
        let pointer = "/properties/sources/items/properties/ref";
        let constrained = schema.constrained(pointer, &pages().ids()).unwrap();
        assert_eq!(
            constrained.schema.pointer(&format!("{pointer}/enum")),
            Some(&json!(["R1", "R2"]))
        );
        // The file's schema is untouched, and validation keeps it (the checks give feedback).
        assert!(schema.schema.pointer(&format!("{pointer}/enum")).is_none());
        let mut unknown = draft_json();
        unknown["sources"][0]["ref"] = json!("R7");
        constrained.validate(&unknown).unwrap();
        assert!(schema.constrained("/properties/sources", &[]).is_none());
        assert!(schema.constrained("/properties/nope", &[]).is_none());
    }

    #[test]
    fn source_url_keys() {
        assert_eq!(
            source_url_key("https://WWW.Ccomptes.fr:443/fr/rapport/?utm_campaign=a&p=2#x"),
            Ok("www.ccomptes.fr/fr/rapport?p=2".to_owned())
        );
        for bad in [
            "https://user:pw@ccomptes.fr/x",
            "https://ccomptes.fr:8443/x",
            "https://192.168.1.1/x",
            "https://[::1]/x",
            "javascript:alert(1)",
            "ftp://ccomptes.fr/x",
        ] {
            assert!(source_url_key(bad).is_err(), "{bad}");
        }
        let long = format!("https://ccomptes.fr/{}", "a".repeat(MAX_SOURCE_URL_LEN));
        assert!(source_url_key(&long).is_err());
    }

    #[test]
    fn fixture_matches_the_schema_and_the_checks() {
        let schema = test_prompts().analysis.output_schema.clone().unwrap();
        schema.validate(&draft_json()).unwrap();
        check(&draft()).unwrap();
        // A model answer with a URL field is a schema violation (closed objects).
        let mut with_url = draft_json();
        with_url["sources"][0]["url"] = json!(CCOMPTES);
        assert!(schema.validate(&with_url).is_err());
    }

    #[test]
    fn inconsistent_analyses_are_rejected() {
        type Mutation = Box<dyn Fn(&mut AnalysisDraft)>;
        let cases: Vec<(&str, Mutation)> = vec![
            ("unknown category", Box::new(|a| a.category = "lune".into())),
            (
                "not allowed for admin level",
                Box::new(|a| a.admin_level = AdminLevel::Commune),
            ),
            (
                "annual_gain_eur",
                Box::new(|a| a.annual_gain_eur.low = 900_000_000),
            ),
            (
                "implementation_cost_eur",
                Box::new(|a| a.implementation_cost_eur.low = -1),
            ),
            (
                "time_to_effect_months",
                Box::new(|a| a.time_to_effect_months = 601),
            ),
            ("feasibility.score", Box::new(|a| a.feasibility.score = 0)),
            (
                "territory code without a name",
                Box::new(|a| a.territory.insee_code = "75056".into()),
            ),
            (
                "insee_code",
                Box::new(|a| {
                    a.territory = Territory {
                        name: "Paris".into(),
                        insee_code: "75-056".into(),
                    }
                }),
            ),
            (
                "impacts must cover",
                Box::new(|a| {
                    a.impacts.remove("entreprises");
                }),
            ),
            ("at least one source", Box::new(|a| a.sources.clear())),
            (
                "invalid source id",
                Box::new(|a| a.sources[0].id = "1".into()),
            ),
            (
                "duplicate source id",
                Box::new(|a| a.sources[1].id = "S1".into()),
            ),
            (
                "not a consulted page",
                Box::new(|a| a.sources[0].page = "R7".into()),
            ),
            (
                "cited sources missing",
                Box::new(|a| a.pros.push("Selon [S9].".into())),
            ),
            (
                "unknown contributions",
                Box::new(|a| {
                    a.contributions_considered.push(ContributionConsidered {
                        contribution_id: "c2".into(),
                        summary: "Résumé".into(),
                        taken_into_account: true,
                        response: "Prise en compte.".into(),
                    })
                }),
            ),
        ];
        for (expected, mutate) in cases {
            let mut output = draft();
            mutate(&mut output);
            let err = check(&output).unwrap_err();
            assert!(err.contains(expected), "{expected}: {err}");
        }
        // A named local authority with its code is fine; two sources may cite the same page.
        let mut output = draft();
        output.admin_level = AdminLevel::Multi;
        output.territory = Territory {
            name: "Corse-du-Sud".into(),
            insee_code: "2A".into(),
        };
        output.sources[1].page = "R1".into();
        check(&output).unwrap();
    }

    #[test]
    fn second_opinion_reasons_follow_the_section() {
        let section = &test_prompts().screening.idea;
        let accept = SecondOpinionOutput {
            decision: Decision::Accept,
            reason_code: "none".into(),
            explanation: "Conforme.".into(),
        };
        accept.check(section).unwrap();
        assert_eq!(accept.rejection_reason(), None);
        let reject = SecondOpinionOutput {
            decision: Decision::Reject,
            reason_code: "off_topic".into(),
            explanation: "Hors sujet.".into(),
        };
        reject.check(section).unwrap();
        assert_eq!(reject.rejection_reason(), Some("off_topic"));
        // `irrelevant` belongs to contributions and reports, not to ideas.
        let wrong_section = SecondOpinionOutput {
            reason_code: "irrelevant".into(),
            ..reject.clone()
        };
        assert!(wrong_section.check(section).is_err());
        let inconsistent = SecondOpinionOutput {
            reason_code: "off_topic".into(),
            ..accept
        };
        assert!(inconsistent.check(section).is_err());
    }

    /// INJ-6: NUL (refused by PostgreSQL), bidi overrides and oversized strings are invalid
    /// outputs, retried by the client instead of failing the storage.
    #[test]
    fn generated_texts_are_checked_for_characters_and_length() {
        for title in [
            "Mutualiser\u{0}les achats".to_owned(),
            "Mutualiser \u{202E}stahca".to_owned(),
            "Mutualiser\u{E0041}".to_owned(),
            "x".repeat(MAX_TITLE_CHARS + 1),
        ] {
            let mut output = draft();
            output.title = title.clone();
            assert!(check(&output).is_err(), "{title:?}");
        }
        let mut output = draft();
        output.title = "x".repeat(MAX_TITLE_CHARS);
        output.summary = "Ligne 1.\nLigne\t2.".into();
        assert_eq!(check(&output), Ok(()));
        let mut output = draft();
        output.sources[0].used_for = "Montant\u{0007} des achats".into();
        assert!(check(&output).is_err());
        let mut output = draft();
        output.pros[0] = "x".repeat(MAX_ITEM_CHARS + 1);
        assert!(check(&output).is_err());

        let section = &test_prompts().screening.idea;
        let opinion = SecondOpinionOutput {
            decision: Decision::Accept,
            reason_code: "none".into(),
            explanation: "Conforme\u{0}.".into(),
        };
        assert!(opinion.check(section).is_err());
        let review = ReportReviewOutput {
            outcome: ReviewOutcome::Update,
            escalate: false,
            explanation: "Chiffre erroné.".into(),
            corrections: "Corriger \u{202E}le montant.".into(),
        };
        assert!(review.check().is_err());
    }

    #[test]
    fn report_reviews_are_normalized() {
        let review: ReportReviewOutput = serde_json::from_value(json!({
            "outcome": "maintain", "escalate": true,
            "explanation": "Contenu menaçant.", "corrections": "x"
        }))
        .unwrap();
        review.check().unwrap();
        let review = review.normalized();
        assert_eq!(review.outcome, ReviewOutcome::Hide);
        assert_eq!(review.corrections, "");

        let update = ReportReviewOutput {
            outcome: ReviewOutcome::Update,
            escalate: false,
            explanation: "Chiffre erroné.".into(),
            corrections: "  ".into(),
        };
        assert!(update.check().is_err());
        let update = ReportReviewOutput {
            corrections: " Corriger le montant [S1]. ".into(),
            ..update
        };
        update.check().unwrap();
        assert_eq!(update.normalized().corrections, "Corriger le montant [S1].");
        let schema = test_prompts().report_review.output_schema.clone().unwrap();
        schema
            .validate(&json!({
                "outcome": "hide", "escalate": false, "explanation": "x", "corrections": ""
            }))
            .unwrap();
    }
}
