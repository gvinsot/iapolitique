//! Closed enumerations of the product (AGENTS.md §5, §6.2, §7, §8). Open lists (categories,
//! impact publics) live in `prompts/taxonomy.yaml` and are validated against it at runtime.

text_enum! {
    /// Administrative level targeted by an idea (§6.2 `admin_level`).
    pub enum AdminLevel {
        Etat => "etat",
        SecuriteSociale => "securite_sociale",
        Region => "region",
        Departement => "departement",
        Intercommunalite => "intercommunalite",
        Commune => "commune",
        /// Several levels at once.
        Multi => "multi",
    }
}

text_enum! {
    /// Nature of the budget effect (§6.2 `kind`).
    pub enum IdeaKind {
        /// Lower spending.
        Savings => "savings",
        /// Higher revenue.
        Revenue => "revenue",
        Mixed => "mixed",
    }
}

text_enum! {
    /// Quality of the available data behind an estimate (§6.2 `confidence`).
    pub enum Confidence {
        Low => "low",
        Medium => "medium",
        High => "high",
    }
}

text_enum! {
    /// Legal instrument needed to implement an idea (§6.2 `legal_vehicle`).
    pub enum LegalVehicle {
        Loi => "loi",
        LoiDeFinances => "loi_de_finances",
        Decret => "decret",
        Arrete => "arrete",
        Deliberation => "deliberation",
        Circulaire => "circulaire",
        OrganisationInterne => "organisation_interne",
        Autre => "autre",
    }
}

text_enum! {
    /// Direction of the impact of an idea on a public (§6.2 `impacts`).
    pub enum ImpactDirection {
        Positive => "positive",
        Neutral => "neutral",
        Negative => "negative",
        Uncertain => "uncertain",
    }
}

text_enum! {
    /// Kind of contribution attached to a published idea (§5.4, §5.5). Keys of
    /// `contribution_kinds` in `prompts/taxonomy.yaml`, plus `doublon` for merged duplicates.
    pub enum ContributionKind {
        Improvement => "amelioration",
        Objection => "objection",
        /// A new idea detected as a duplicate and merged into the existing one.
        Duplicate => "doublon",
    }
}

text_enum! {
    /// Reason selected by the author of a report (§5.6): keys of `report_reasons` in
    /// `prompts/taxonomy.yaml`.
    pub enum ReportReason {
        CalculationError => "erreur_chiffrage",
        WrongSource => "source_erronee",
        ProblematicContent => "contenu_problematique",
        Duplicate => "doublon",
        Other => "autre",
    }
}

text_enum! {
    /// Conclusion of the AI review of the reports of an idea (§5.6).
    pub enum ReviewOutcome {
        /// The idea and its analysis stay as they are.
        Maintain => "maintain",
        /// A new analysis is requested.
        Update => "update",
        /// The idea is hidden from the portal.
        Hide => "hide",
    }
}

text_enum! {
    /// Kind of entity screened by the decision model or reachable through a tracking link.
    pub enum TargetKind {
        Idea => "idea",
        Contribution => "contribution",
        Report => "report",
    }
}

text_enum! {
    /// Verdict of a single screening criterion given its thresholds (§5.3).
    pub enum CriterionVerdict {
        Pass => "pass",
        Fail => "fail",
        /// Between `reject_below` and `accept_above`.
        Gray => "gray",
    }
}

text_enum! {
    /// Result of the decision model stage of a screening.
    pub enum ScreeningOutcome {
        Accepted => "accepted",
        Rejected => "rejected",
        /// At least one criterion in the gray zone and none failed: second opinion needed.
        GrayZone => "gray_zone",
    }
}

text_enum! {
    /// Binary decision: second opinion result, final screening decision or admin label.
    pub enum Decision {
        Accept => "accept",
        Reject => "reject",
    }
}

text_enum! {
    /// What triggered an analysis (§5.7).
    pub enum AnalysisTrigger {
        Initial => "initial",
        Contribution => "contribution",
        Merge => "merge",
        Report => "report",
        Admin => "admin",
    }
}

text_enum! {
    /// Who caused a status change.
    pub enum Actor {
        /// The anonymous author (submission).
        Author => "author",
        /// Synchronous processing in the api (screening).
        Api => "api",
        /// Background job.
        Worker => "worker",
        /// Administrator through the admin API.
        Admin => "admin",
    }
}

text_enum! {
    /// Purpose of an AI call, recorded in `ai_calls` (§6.4).
    pub enum AiPurpose {
        Screening => "screening",
        SecondOpinion => "second_opinion",
        AnalysisResearch => "analysis_research",
        AnalysisStructuring => "analysis_structuring",
        Embedding => "embedding",
        ReportReview => "report_review",
    }
}

text_enum! {
    /// Service that answered an AI call, recorded in `ai_calls` (§6.4).
    pub enum AiProvider {
        /// Local decision model (d3-mini), free.
        DecisionService => "decision_service",
        Mistral => "mistral",
    }
}

text_enum! {
    /// Outcome of an AI call.
    pub enum AiCallStatus {
        Success => "success",
        /// The provider answered with an error status.
        Error => "error",
        Timeout => "timeout",
        /// The answer did not match the expected schema.
        InvalidOutput => "invalid_output",
    }
}

text_enum! {
    /// Format of a generated open data export (§7.4).
    pub enum ExportFormat {
        Json => "json",
        Csv => "csv",
    }
}

impl ExportFormat {
    pub const fn content_type(self) -> &'static str {
        match self {
            Self::Json => "application/json; charset=utf-8",
            Self::Csv => "text/csv; charset=utf-8",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn admin_levels_match_agents_md() {
        let texts: Vec<_> = AdminLevel::ALL.iter().map(|l| l.as_str()).collect();
        assert_eq!(
            texts,
            [
                "etat",
                "securite_sociale",
                "region",
                "departement",
                "intercommunalite",
                "commune",
                "multi"
            ]
        );
    }

    #[test]
    fn legal_vehicles_match_agents_md() {
        let texts: Vec<_> = LegalVehicle::ALL.iter().map(|l| l.as_str()).collect();
        assert_eq!(
            texts,
            [
                "loi",
                "loi_de_finances",
                "decret",
                "arrete",
                "deliberation",
                "circulaire",
                "organisation_interne",
                "autre"
            ]
        );
    }

    #[test]
    fn report_reasons_and_outcomes() {
        assert_eq!(ReportReason::ALL.len(), 5);
        assert_eq!(
            "erreur_chiffrage".parse::<ReportReason>(),
            Ok(ReportReason::CalculationError)
        );
        assert_eq!("hide".parse::<ReviewOutcome>(), Ok(ReviewOutcome::Hide));
    }

    #[test]
    fn export_content_types() {
        assert!(ExportFormat::Csv.content_type().starts_with("text/csv"));
        assert!(
            ExportFormat::Json
                .content_type()
                .starts_with("application/json")
        );
    }
}
