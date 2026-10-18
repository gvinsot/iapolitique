//! Publication rules (`/regles`, the CGU of AGENTS.md §7.5) as referenced by the API: version
//! of the rules accepted with each publishable text, and the article that a refusal or a
//! masking applies, for the statement of reasons (DSA art. 17, AGENTS.md §7.1).
//!
//! The articles and anchors mirror `frontend/src/lib/rules.ts` and the rules page; the reason
//! codes are those of `prompts/screening.yaml` (a test checks that each one has its article).

use serde::Serialize;
use utoipa::ToSchema;

/// Version of the rules (CGU) accepted with each idea and contribution (`cgu_version`). Bump it
/// with the rules page whenever the accepted clauses change; `GET /api/legal` serves it.
pub const CGU_VERSION: &str = "1.0";

/// Path of the rules page.
pub const RULES_PATH: &str = "/regles";

/// An article of the rules page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuleArticle {
    /// Article number, e.g. `4.3`.
    pub number: &'static str,
    /// Anchor of the article in the rules page.
    pub anchor: &'static str,
    pub title: &'static str,
}

/// Article 4: acceptance and rejection criteria (fallback for an unknown reason code).
pub const CRITERIA_ARTICLE: RuleArticle = RuleArticle {
    number: "4",
    anchor: "criteres",
    title: "Critères d'acceptation et de rejet",
};

/// Article 7: measures and durations (masking after a report review).
pub const MEASURES_ARTICLE: RuleArticle = RuleArticle {
    number: "7",
    anchor: "mesures",
    title: "Mesures et durées",
};

/// Numbered criteria of article 4, by `reason_code` of `screening.yaml`.
const CRITERIA: &[(&str, RuleArticle)] = &[
    (
        "injection",
        RuleArticle {
            number: "4.1",
            anchor: "critere-injection",
            title: "Pas de consigne adressée à une IA",
        },
    ),
    (
        "disrespectful",
        RuleArticle {
            number: "4.2",
            anchor: "critere-respect",
            title: "Respect des personnes",
        },
    ),
    (
        "targeted",
        RuleArticle {
            number: "4.3",
            anchor: "critere-ciblage",
            title: "Aucun ciblage d'une personne, d'un parti ou d'un groupe",
        },
    ),
    (
        "personal_data",
        RuleArticle {
            number: "4.4",
            anchor: "critere-donnees-personnelles",
            title: "Aucun particulier identifiable",
        },
    ),
    (
        "off_topic",
        RuleArticle {
            number: "4.5",
            anchor: "critere-sujet",
            title: "Une mesure pour les finances publiques",
        },
    ),
    (
        "unlawful",
        RuleArticle {
            number: "4.6",
            anchor: "critere-droits-fondamentaux",
            title: "Respect des droits fondamentaux",
        },
    ),
    (
        "unrealistic",
        RuleArticle {
            number: "4.7",
            anchor: "critere-realisme",
            title: "Réalisme",
        },
    ),
    (
        "irrelevant",
        RuleArticle {
            number: "4.8",
            anchor: "critere-pertinence",
            title: "Pertinence par rapport à l'idée",
        },
    ),
];

/// Article of the criterion behind a refusal reason code.
pub fn article_for_reason(reason_code: &str) -> Option<RuleArticle> {
    CRITERIA
        .iter()
        .find(|(code, _)| *code == reason_code)
        .map(|(_, article)| *article)
}

/// Rule at stake in a decision, as sent to the author.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct RuleReference {
    /// Article number in the rules page, e.g. `4.3`.
    #[schema(example = "4.5")]
    pub article: String,
    #[schema(example = "Une mesure pour les finances publiques")]
    pub title: String,
    /// Relative link to the article, e.g. `/regles#critere-sujet`.
    #[schema(example = "/regles#critere-sujet")]
    pub url: String,
}

impl From<RuleArticle> for RuleReference {
    fn from(article: RuleArticle) -> Self {
        Self {
            article: article.number.to_owned(),
            title: article.title.to_owned(),
            url: format!("{RULES_PATH}#{}", article.anchor),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::prompts::test_prompts;

    #[test]
    fn every_reason_code_of_the_grid_has_an_article() {
        let prompts = test_prompts();
        for code in prompts.screening.reason_codes() {
            assert!(
                article_for_reason(code).is_some(),
                "reason code `{code}` of screening.yaml has no article in domain::rules"
            );
        }
    }

    #[test]
    fn articles_are_unique_and_linkable() {
        let mut anchors: Vec<&str> = CRITERIA.iter().map(|(_, a)| a.anchor).collect();
        anchors.sort_unstable();
        anchors.dedup();
        assert_eq!(anchors.len(), CRITERIA.len());
        let reference = RuleReference::from(article_for_reason("off_topic").unwrap());
        assert_eq!(reference.article, "4.5");
        assert_eq!(reference.url, "/regles#critere-sujet");
        assert_eq!(article_for_reason("unknown"), None);
        assert_eq!(RuleReference::from(MEASURES_ARTICLE).url, "/regles#mesures");
    }
}
