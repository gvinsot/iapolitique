//! Typed outputs of Mistral (AGENTS.md §6.2, §13.4) and the checks that a JSON schema cannot
//! express. The answers are first validated against their schema of `prompts/`, then
//! deserialized here, then checked: a failed check is an invalid output, retried like a schema
//! violation (§6.1).

use std::collections::{BTreeMap, BTreeSet};
use std::sync::LazyLock;

use regex::Regex;
use serde::{Deserialize, Serialize};

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

/// URLs that an analysis may cite (INJ-3): the pages consulted by its web research and the
/// sources of the analysis it replaces. Any other URL (planted by a visitor text, a
/// contribution or a web page) is refused.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AllowedSources(BTreeSet<String>);

impl AllowedSources {
    pub fn new<'a>(urls: impl IntoIterator<Item = &'a str>) -> Self {
        Self(
            urls.into_iter()
                .filter_map(|url| source_url_key(url).ok())
                .collect(),
        )
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    fn check(&self, url: &str) -> Result<(), &'static str> {
        let key = source_url_key(url)?;
        if self.0.contains(&key) {
            Ok(())
        } else {
            Err("URL not consulted by the web research")
        }
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Source {
    pub id: String,
    pub title: String,
    pub publisher: String,
    pub url: String,
    pub used_for: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContributionConsidered {
    pub contribution_id: String,
    pub summary: String,
    pub taken_into_account: bool,
    pub response: String,
}

/// Output of `prompts/analysis.md` (§6.2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnalysisOutput {
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
    pub sources: Vec<Source>,
    pub contributions_considered: Vec<ContributionConsidered>,
    pub changes_since_previous: String,
}

impl AnalysisOutput {
    /// Characters and lengths of every generated string (INJ-6).
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
        for source in &self.sources {
            check_ai_text("sources.id", &source.id, MAX_SOURCE_FIELD_CHARS, p);
            check_ai_text("sources.title", &source.title, MAX_SOURCE_FIELD_CHARS, p);
            check_ai_text(
                "sources.publisher",
                &source.publisher,
                MAX_SOURCE_FIELD_CHARS,
                p,
            );
            check_ai_text(
                "sources.used_for",
                &source.used_for,
                MAX_SOURCE_FIELD_CHARS,
                p,
            );
            check_ai_text("sources.url", &source.url, MAX_SOURCE_URL_LEN, p);
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

    /// Checks of §13.4 beyond the schema. `contribution_ids` are the contributions given to the
    /// prompt: the analysis may only report on those. `allowed` are the URLs it may cite.
    pub fn check(
        &self,
        taxonomy: &Taxonomy,
        contribution_ids: &[String],
        allowed: &AllowedSources,
    ) -> Result<(), String> {
        let mut problems = Vec::new();
        if self.title.trim().is_empty() || self.summary.trim().is_empty() {
            problems.push("empty title or summary".to_owned());
        }
        self.check_texts(&mut problems);
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

        if self.sources.is_empty() {
            problems.push("at least one source is required".to_owned());
        }
        let mut ids = BTreeSet::new();
        for source in &self.sources {
            if !SOURCE_ID_RE.is_match(&source.id) {
                problems.push(format!("invalid source id `{}`", source.id));
            }
            if !ids.insert(source.id.as_str()) {
                problems.push(format!("duplicate source id `{}`", source.id));
            }
            if !(source.url.starts_with("https://") || source.url.starts_with("http://")) {
                problems.push(format!("source {} needs an http(s) URL", source.id));
            } else if let Err(err) = allowed.check(&source.url) {
                problems.push(format!("source {}: {err}", source.id));
            }
        }
        let cited: BTreeSet<&str> = self
            .texts()
            .into_iter()
            .flat_map(|t| SOURCE_REF_RE.captures_iter(t))
            .filter_map(|c| c.get(1).map(|m| m.as_str()))
            .collect();
        let unknown: Vec<&str> = cited.difference(&ids).copied().collect();
        if !unknown.is_empty() {
            problems.push(format!("cited sources missing from sources: {unknown:?}"));
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
        if problems.is_empty() {
            Ok(())
        } else {
            Err(problems.join("; "))
        }
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

    fn analysis_json() -> Value {
        serde_json::from_str(include_str!("../../tests/fixtures/mistral/analysis.json")).unwrap()
    }

    fn analysis() -> AnalysisOutput {
        serde_json::from_value(analysis_json()).unwrap()
    }

    /// The URLs of the fixture, as the research would have returned them.
    fn allowed() -> AllowedSources {
        AllowedSources::new(
            analysis()
                .sources
                .iter()
                .map(|s| s.url.as_str())
                .collect::<Vec<_>>(),
        )
    }

    fn check(output: &AnalysisOutput) -> Result<(), String> {
        output.check(&test_prompts().taxonomy, &["c1".to_owned()], &allowed())
    }

    /// INJ-3: a source must be a page consulted by the web research.
    #[test]
    fn sources_must_come_from_the_research() {
        let mut output = analysis();
        output.sources[0].url = "https://ccomptes-rapports.example/2025/achats".into();
        let err = check(&output).unwrap_err();
        assert!(err.contains("not consulted by the web research"), "{err}");
        assert!(
            output
                .check(
                    &test_prompts().taxonomy,
                    &["c1".to_owned()],
                    &AllowedSources::default()
                )
                .is_err()
        );

        // Scheme, default port, fragment, trailing slash and utm_* parameters do not count.
        let mut output = analysis();
        let original = output.sources[0].url.clone();
        output.sources[0].url = format!("{original}/?utm_source=x#section");
        assert_eq!(check(&output), Ok(()));
        output.sources[0].url = original.replacen("https://", "http://", 1);
        assert_eq!(check(&output), Ok(()));
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
        schema.validate(&analysis_json()).unwrap();
        check(&analysis()).unwrap();
    }

    #[test]
    fn inconsistent_analyses_are_rejected() {
        type Mutation = Box<dyn Fn(&mut AnalysisOutput)>;
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
                "http(s) URL",
                Box::new(|a| a.sources[0].url = "ccomptes.fr".into()),
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
            let mut output = analysis();
            mutate(&mut output);
            let err = check(&output).unwrap_err();
            assert!(err.contains(expected), "{expected}: {err}");
        }
        // A named local authority with its code is fine.
        let mut output = analysis();
        output.admin_level = AdminLevel::Multi;
        output.territory = Territory {
            name: "Corse-du-Sud".into(),
            insee_code: "2A".into(),
        };
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
            let mut output = analysis();
            output.title = title.clone();
            assert!(check(&output).is_err(), "{title:?}");
        }
        let mut output = analysis();
        output.title = "x".repeat(MAX_TITLE_CHARS);
        output.summary = "Ligne 1.\nLigne\t2.".into();
        assert_eq!(check(&output), Ok(()));
        let mut output = analysis();
        output.sources[0].publisher = "Cour\u{0007} des comptes".into();
        assert!(check(&output).is_err());
        let mut output = analysis();
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
