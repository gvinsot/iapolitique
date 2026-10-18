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
    /// prompt: the analysis may only report on those.
    pub fn check(&self, taxonomy: &Taxonomy, contribution_ids: &[String]) -> Result<(), String> {
        let mut problems = Vec::new();
        if self.title.trim().is_empty() || self.summary.trim().is_empty() {
            problems.push("empty title or summary".to_owned());
        }
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

    fn check(output: &AnalysisOutput) -> Result<(), String> {
        output.check(&test_prompts().taxonomy, &["c1".to_owned()])
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
