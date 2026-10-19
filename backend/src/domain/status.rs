//! State machines (AGENTS.md §5.2). The transitions declared here are the **only** allowed
//! ones: database code must call [`StateMachine::transition`] before persisting a new status,
//! and record it in the matching history table.

/// A status change that the state machine forbids.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("invalid {entity} transition from `{from}` to `{to}`")]
pub struct InvalidTransition {
    pub entity: &'static str,
    pub from: &'static str,
    pub to: &'static str,
}

/// Common behaviour of the status enums.
pub trait StateMachine: Copy + Eq + 'static {
    /// Name of the entity, used in error messages.
    const ENTITY: &'static str;

    /// Status of a freshly created entity.
    const INITIAL: Self;

    /// Statuses reachable in one step from `self`.
    fn allowed_next(self) -> &'static [Self];

    fn label(self) -> &'static str;

    fn can_transition_to(self, next: Self) -> bool {
        self.allowed_next().contains(&next)
    }

    fn is_terminal(self) -> bool {
        self.allowed_next().is_empty()
    }

    /// Validates the transition and returns the new status.
    fn transition(self, next: Self) -> Result<Self, InvalidTransition> {
        if self.can_transition_to(next) {
            Ok(next)
        } else {
            Err(InvalidTransition {
                entity: Self::ENTITY,
                from: self.label(),
                to: next.label(),
            })
        }
    }
}

/// Statuses of the entities screened by the decision model (§5.3): ideas, contributions and
/// reports share the same screening steps.
pub trait ScreenedStatus: StateMachine {
    /// Received, waiting for (or retrying) the decision model.
    const SUBMITTED: Self;
    const SCREENING: Self;
    /// Gray zone: waiting for the Mistral second opinion.
    const SECOND_OPINION: Self;
    const ACCEPTED: Self;
    const REJECTED: Self;
}

text_enum! {
    /// Lifecycle of an idea.
    ///
    /// ```text
    /// submitted ─▶ screening ─┬─▶ rejected ─▶ purged (or accepted: overturned by the admin)
    ///                         ├─▶ second_opinion ─▶ rejected | accepted
    ///                         ├─▶ accepted ─▶ dedup ─┬─▶ merged
    ///                         │                      └─▶ queued_analysis ─▶ analyzing ─┬─▶ published ⇄ hidden ─▶ purged
    ///                         │                                                        └─▶ analysis_failed ─▶ queued_analysis
    ///                         └─▶ submitted (decision service unavailable, retried by a job)
    /// published | hidden ─▶ withdrawn (deleted by its author) ─▶ purged (12 months later)
    /// ```
    ///
    /// Re-analyses of a published (or hidden) idea do not change its status: the idea stays
    /// visible with its current analysis until the new one replaces it.
    pub enum IdeaStatus {
        /// Received, waiting for (or retrying) the decision model.
        Submitted => "submitted",
        /// Being evaluated by the decision model.
        Screening => "screening",
        /// Gray zone: waiting for the Mistral second opinion.
        SecondOpinion => "second_opinion",
        /// Refused; the text is purged 30 days later.
        Rejected => "rejected",
        /// Text purged (refused 30 days ago, hidden 12 months ago, or merged into a purged
        /// idea): only aggregated data remains.
        Purged => "purged",
        /// Kept by the screening, waiting for duplicate detection.
        Accepted => "accepted",
        /// Duplicate detection in progress (embedding + nearest neighbour).
        Dedup => "dedup",
        /// Duplicate of a published idea, merged into it as a contribution; purged with it if
        /// that idea is purged after being hidden.
        Merged => "merged",
        /// Waiting for the Mistral analysis.
        QueuedAnalysis => "queued_analysis",
        /// Mistral analysis in progress.
        Analyzing => "analyzing",
        /// Analysis failed after all retries: needs an admin action.
        AnalysisFailed => "analysis_failed",
        /// Visible on the public portal.
        Published => "published",
        /// Hidden after a report review or an admin action.
        Hidden => "hidden",
        /// Published (or hidden) idea deleted by its author (§7.1): gone from the public site,
        /// the API and the exports with everything attached to it, kept for the admin only, and
        /// purged 12 months later (§7.3). Never republished.
        Withdrawn => "withdrawn",
    }
}

impl StateMachine for IdeaStatus {
    const ENTITY: &'static str = "idea";
    const INITIAL: Self = Self::Submitted;

    fn allowed_next(self) -> &'static [Self] {
        use IdeaStatus::*;
        match self {
            Submitted => &[Screening],
            Screening => &[Submitted, Rejected, SecondOpinion, Accepted],
            SecondOpinion => &[Rejected, Accepted],
            // A refusal overturned by the administrator after a human review (§7.1).
            Rejected => &[Purged, Accepted],
            Purged => &[],
            // From any accepted status, the administrator may refuse the idea (§8), published
            // or not: it leaves the site and is purged 30 days later (§7.3).
            Accepted => &[Dedup, Rejected],
            Dedup => &[Merged, QueuedAnalysis, Rejected],
            // Merged duplicates are purged with the hidden idea they were merged into.
            Merged => &[Purged],
            QueuedAnalysis => &[Analyzing, Rejected],
            Analyzing => &[Published, AnalysisFailed, QueuedAnalysis, Rejected],
            AnalysisFailed => &[QueuedAnalysis, Rejected],
            // Deleted by its author: withdrawn, not erased (§7.1).
            Published => &[Hidden, Withdrawn, Rejected],
            // Hidden 12 months: the text is purged (§7.3).
            Hidden => &[Published, Purged, Withdrawn, Rejected],
            // Not reversible, by the author or by the admin: purged 12 months later.
            Withdrawn => &[Purged],
        }
    }

    fn label(self) -> &'static str {
        self.as_str()
    }
}

impl ScreenedStatus for IdeaStatus {
    const SUBMITTED: Self = Self::Submitted;
    const SCREENING: Self = Self::Screening;
    const SECOND_OPINION: Self = Self::SecondOpinion;
    const ACCEPTED: Self = Self::Accepted;
    const REJECTED: Self = Self::Rejected;
}

impl IdeaStatus {
    /// Whether the administrator may refuse the idea (§8): accepted, whether published or not.
    pub const fn admin_rejectable(self) -> bool {
        matches!(
            self,
            Self::Accepted
                | Self::Dedup
                | Self::QueuedAnalysis
                | Self::Analyzing
                | Self::AnalysisFailed
                | Self::Published
                | Self::Hidden
        )
    }

    /// Whether the idea appears on the public portal.
    pub const fn is_public(self) -> bool {
        matches!(self, Self::Published)
    }

    /// Whether the idea has an analysis that may be (re)computed: published or hidden ideas.
    pub const fn has_publication(self) -> bool {
        matches!(self, Self::Published | Self::Hidden)
    }
}

text_enum! {
    /// Lifecycle of a contribution (improvement or objection, §5.5) or of a merged duplicate.
    pub enum ContributionStatus {
        Submitted => "submitted",
        Screening => "screening",
        SecondOpinion => "second_opinion",
        /// Kept; will be taken into account by the next analysis of the idea.
        Accepted => "accepted",
        Rejected => "rejected",
        /// Taken into account by an analysis of the idea.
        Integrated => "integrated",
        Purged => "purged",
    }
}

impl StateMachine for ContributionStatus {
    const ENTITY: &'static str = "contribution";
    const INITIAL: Self = Self::Submitted;

    fn allowed_next(self) -> &'static [Self] {
        use ContributionStatus::*;
        match self {
            Submitted => &[Screening],
            Screening => &[Submitted, Rejected, SecondOpinion, Accepted],
            SecondOpinion => &[Rejected, Accepted],
            Accepted => &[Integrated],
            Rejected => &[Purged, Accepted],
            Integrated | Purged => &[],
        }
    }

    fn label(self) -> &'static str {
        self.as_str()
    }
}

impl ScreenedStatus for ContributionStatus {
    const SUBMITTED: Self = Self::Submitted;
    const SCREENING: Self = Self::Screening;
    const SECOND_OPINION: Self = Self::SecondOpinion;
    const ACCEPTED: Self = Self::Accepted;
    const REJECTED: Self = Self::Rejected;
}

text_enum! {
    /// Lifecycle of a report (§5.6).
    pub enum ReportStatus {
        Submitted => "submitted",
        Screening => "screening",
        SecondOpinion => "second_opinion",
        /// Kept; waiting for the grouped AI review of the idea.
        Accepted => "accepted",
        Rejected => "rejected",
        /// Handled by a report review (`maintain`, `update` or `hide`).
        Reviewed => "reviewed",
        Purged => "purged",
    }
}

impl StateMachine for ReportStatus {
    const ENTITY: &'static str = "report";
    const INITIAL: Self = Self::Submitted;

    fn allowed_next(self) -> &'static [Self] {
        use ReportStatus::*;
        match self {
            Submitted => &[Screening],
            Screening => &[Submitted, Rejected, SecondOpinion, Accepted],
            SecondOpinion => &[Rejected, Accepted],
            Accepted => &[Reviewed],
            Rejected => &[Purged, Accepted],
            Reviewed | Purged => &[],
        }
    }

    fn label(self) -> &'static str {
        self.as_str()
    }
}

impl ScreenedStatus for ReportStatus {
    const SUBMITTED: Self = Self::Submitted;
    const SCREENING: Self = Self::Screening;
    const SECOND_OPINION: Self = Self::SecondOpinion;
    const ACCEPTED: Self = Self::Accepted;
    const REJECTED: Self = Self::Rejected;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    /// Checks every (from, to) pair against an explicit list of expected transitions.
    fn assert_exact_transitions<S>(all: &[S], expected: &[(S, S)])
    where
        S: StateMachine + Ord + std::fmt::Debug,
    {
        let expected: BTreeSet<(S, S)> = expected.iter().copied().collect();
        for &from in all {
            for &to in all {
                let allowed = expected.contains(&(from, to));
                assert_eq!(
                    from.can_transition_to(to),
                    allowed,
                    "{from:?} -> {to:?} should be {}",
                    if allowed { "allowed" } else { "forbidden" }
                );
                match from.transition(to) {
                    Ok(next) => {
                        assert!(allowed);
                        assert_eq!(next, to);
                    }
                    Err(err) => {
                        assert!(!allowed);
                        assert_eq!(err.entity, S::ENTITY);
                        assert_eq!(err.from, from.label());
                        assert_eq!(err.to, to.label());
                    }
                }
            }
        }
    }

    /// Every status must be reachable from the initial one.
    fn assert_all_reachable<S>(all: &[S])
    where
        S: StateMachine + Ord + std::fmt::Debug,
    {
        let mut seen = BTreeSet::from([S::INITIAL]);
        let mut stack = vec![S::INITIAL];
        while let Some(status) = stack.pop() {
            for &next in status.allowed_next() {
                if seen.insert(next) {
                    stack.push(next);
                }
            }
        }
        let all: BTreeSet<S> = all.iter().copied().collect();
        assert_eq!(seen, all, "unreachable statuses");
    }

    #[test]
    fn idea_transitions_match_agents_md() {
        use IdeaStatus::*;
        assert_exact_transitions(
            IdeaStatus::ALL,
            &[
                (Submitted, Screening),
                (Screening, Submitted),
                (Screening, Rejected),
                (Screening, SecondOpinion),
                (Screening, Accepted),
                (SecondOpinion, Rejected),
                (SecondOpinion, Accepted),
                (Rejected, Purged),
                (Rejected, Accepted),
                (Accepted, Dedup),
                (Accepted, Rejected),
                (Dedup, Merged),
                (Dedup, QueuedAnalysis),
                (Dedup, Rejected),
                (QueuedAnalysis, Analyzing),
                (QueuedAnalysis, Rejected),
                (Analyzing, Published),
                (Analyzing, AnalysisFailed),
                (Analyzing, QueuedAnalysis),
                (Analyzing, Rejected),
                (AnalysisFailed, QueuedAnalysis),
                (AnalysisFailed, Rejected),
                (Merged, Purged),
                (Published, Hidden),
                (Published, Withdrawn),
                (Published, Rejected),
                (Hidden, Published),
                (Hidden, Purged),
                (Hidden, Withdrawn),
                (Hidden, Rejected),
                (Withdrawn, Purged),
            ],
        );
        assert_all_reachable(IdeaStatus::ALL);
    }

    #[test]
    fn idea_terminal_statuses() {
        let terminal: Vec<_> = IdeaStatus::ALL
            .iter()
            .filter(|s| s.is_terminal())
            .copied()
            .collect();
        assert_eq!(terminal, vec![IdeaStatus::Purged]);
    }

    #[test]
    fn idea_visibility() {
        for status in IdeaStatus::ALL {
            assert_eq!(status.is_public(), *status == IdeaStatus::Published);
            assert_eq!(
                status.has_publication(),
                matches!(status, IdeaStatus::Published | IdeaStatus::Hidden)
            );
        }
    }

    #[test]
    fn idea_transition_error_message() {
        let err = IdeaStatus::Published
            .transition(IdeaStatus::Accepted)
            .unwrap_err();
        assert_eq!(
            err.to_string(),
            "invalid idea transition from `published` to `accepted`"
        );
    }

    #[test]
    fn contribution_transitions() {
        use ContributionStatus::*;
        assert_exact_transitions(
            ContributionStatus::ALL,
            &[
                (Submitted, Screening),
                (Screening, Submitted),
                (Screening, Rejected),
                (Screening, SecondOpinion),
                (Screening, Accepted),
                (SecondOpinion, Rejected),
                (SecondOpinion, Accepted),
                (Accepted, Integrated),
                (Rejected, Purged),
                (Rejected, Accepted),
            ],
        );
        assert_all_reachable(ContributionStatus::ALL);
    }

    #[test]
    fn report_transitions() {
        use ReportStatus::*;
        assert_exact_transitions(
            ReportStatus::ALL,
            &[
                (Submitted, Screening),
                (Screening, Submitted),
                (Screening, Rejected),
                (Screening, SecondOpinion),
                (Screening, Accepted),
                (SecondOpinion, Rejected),
                (SecondOpinion, Accepted),
                (Accepted, Reviewed),
                (Rejected, Purged),
                (Rejected, Accepted),
            ],
        );
        assert_all_reachable(ReportStatus::ALL);
    }

    /// The screening path is allowed for every screened entity.
    fn assert_screening_path<S: ScreenedStatus + std::fmt::Debug>() {
        assert_eq!(S::INITIAL, S::SUBMITTED);
        for (from, to) in [
            (S::SUBMITTED, S::SCREENING),
            (S::SCREENING, S::SUBMITTED),
            (S::SCREENING, S::ACCEPTED),
            (S::SCREENING, S::REJECTED),
            (S::SCREENING, S::SECOND_OPINION),
            (S::SECOND_OPINION, S::ACCEPTED),
            (S::SECOND_OPINION, S::REJECTED),
        ] {
            assert!(from.can_transition_to(to), "{from:?} -> {to:?}");
        }
    }

    #[test]
    fn screened_statuses_share_the_screening_path() {
        assert_screening_path::<IdeaStatus>();
        assert_screening_path::<ContributionStatus>();
        assert_screening_path::<ReportStatus>();
    }

    #[test]
    fn statuses_have_stable_text() {
        assert_eq!(IdeaStatus::QueuedAnalysis.as_str(), "queued_analysis");
        assert_eq!(IdeaStatus::AnalysisFailed.as_str(), "analysis_failed");
        assert_eq!(IdeaStatus::SecondOpinion.as_str(), "second_opinion");
        assert_eq!(IdeaStatus::Withdrawn.as_str(), "withdrawn");
        assert_eq!(IdeaStatus::ALL.len(), 14);
        assert_eq!(ContributionStatus::ALL.len(), 7);
        assert_eq!(ReportStatus::ALL.len(), 7);
    }
}
