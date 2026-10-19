//! Statement of reasons of an automated decision against an author (DSA art. 17(3), AGENTS.md
//! §7.1): measure taken, reason, rule at stake (article of `/regles`), automated decision
//! notice, date of the decision and erasure date. Recourse (human review, contact, court) is
//! described by the tracking page and the rules page.

use chrono::NaiveDate;
use serde::Serialize;
use sqlx::PgPool;
use utoipa::ToSchema;

use super::Target;
use crate::ai::prompts::Prompts;
use crate::domain::rules::{self, RuleReference};
use crate::domain::text_enum;
use crate::domain::types::TargetKind;
use crate::jobs::purge::{
    HIDDEN_TEXT_RETENTION_MONTHS, REJECTED_TEXT_RETENTION_DAYS, SCHEDULE as PURGE_SCHEDULE,
};

/// Message of a refusal whose reason code is no longer in the grid.
const UNKNOWN_REASON_MESSAGE: &str =
    "Votre texte ne respecte pas les règles de publication du site.";
/// Reason code of a masking.
const HIDDEN_REASON_CODE: &str = "hidden";
/// `ideas.hidden_reason` of a masking decided by the administrator.
const ADMIN_HIDDEN_REASON: &str = crate::domain::types::HiddenReason::Admin.as_str();

/// Masking of an idea: reason, day of the masking, first day the daily purge erases the text
/// (the next purge run, Paris time, after `hidden_at + 12 months`, §7.3), whether a human review
/// is pending (the purge then waits). `$1` id, `$2` retention months, `$3`/`$4` purge hour and
/// minute.
const HIDDEN_SQL: &str = "SELECT hidden_reason, (masked_at AT TIME ZONE 'Europe/Paris')::date,
        ((masked_at + make_interval(months => $2)) AT TIME ZONE 'Europe/Paris')::date
        + CASE WHEN ((masked_at + make_interval(months => $2)) AT TIME ZONE 'Europe/Paris')::time
                    >= make_time($3, $4, 0)
               THEN 1 ELSE 0 END,
        EXISTS (SELECT 1 FROM review_requests q
                WHERE q.target_kind = 'idea' AND q.target_id = i.id AND q.status = 'pending')
    FROM (SELECT id, hidden_reason, coalesce(hidden_at, status_changed_at) AS masked_at
          FROM ideas WHERE id = $1 AND status = 'hidden') i";
const HIDDEN_AFTER_REPORT_MESSAGE: &str = "L'idée a été masquée après l'examen automatisé d'un \
    signalement : elle n'apparaît plus sur le site ni dans les exports suivants.";
const HIDDEN_BY_ADMIN_MESSAGE: &str = "L'idée a été masquée par l'éditeur du site : elle \
    n'apparaît plus sur le site ni dans les exports suivants.";

text_enum! {
    /// Measure taken against the text.
    pub enum Measure {
        /// Refused by the screening: not published.
        Rejected => "rejected",
        /// Published idea masked after a report review or by the administrator.
        Hidden => "hidden",
    }
}

/// Statement of reasons, given with a refusal and on the tracking page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct StatementOfReasons {
    pub measure: Measure,
    /// `reason_code` of `prompts/screening.yaml` (refusal) or `hidden` (masking).
    #[schema(example = "off_topic")]
    pub reason_code: String,
    /// Explanation in French, with an invitation to rephrase for a refusal.
    pub reason_message: String,
    /// Article of the rules applied.
    pub rule: RuleReference,
    /// The decision was taken automatically by an AI, without human review (`false`: decided
    /// by the administrator).
    pub automated: bool,
    /// The decision followed a Mistral second opinion (gray zone).
    pub second_opinion: bool,
    /// Day of the decision (Europe/Paris).
    pub decided_on: NaiveDate,
    /// Day from which the text is erased, when an erasure is scheduled.
    #[schema(required = true)]
    pub purge_on: Option<NaiveDate>,
}

/// French message of a refusal reason of a section.
pub fn reason_message(prompts: &Prompts, kind: TargetKind, reason_code: &str) -> String {
    prompts
        .screening
        .section(kind)
        .iter()
        .find(|c| c.reason_code == reason_code)
        .map_or_else(
            || UNKNOWN_REASON_MESSAGE.to_owned(),
            |c| c.reason_message.clone(),
        )
}

/// Refusal of a text of `$table`: reason, day of the decision, first day the daily purge erases
/// the text (the next purge run, Paris time, after `rejected_at + retention`), and whether a
/// second opinion took part, whether a human review is pending (the purge then waits), and
/// whether the administrator decided it (`$by_admin`: a column of `ideas` only, §8).
/// `$1` id, `$2` retention days, `$3`/`$4` purge hour and minute, `$5` target kind.
/// Compile-time constant SQL (AGENTS.md §14.1).
macro_rules! rejected_sql {
    ($table:literal, $by_admin:literal) => {
        concat!(
            "SELECT t.rejection_reason_code, (t.rejected_at AT TIME ZONE 'Europe/Paris')::date,
                    ((t.rejected_at + make_interval(days => $2)) AT TIME ZONE 'Europe/Paris')::date
                    + CASE WHEN ((t.rejected_at + make_interval(days => $2))
                                 AT TIME ZONE 'Europe/Paris')::time >= make_time($3, $4, 0)
                           THEN 1 ELSE 0 END,
                    EXISTS (SELECT 1 FROM screenings s
                            WHERE s.target_kind = $5 AND s.target_id = t.id
                              AND s.second_opinion_decision = 'reject'),
                    EXISTS (SELECT 1 FROM review_requests q
                            WHERE q.target_kind = $5 AND q.target_id = t.id
                              AND q.status = 'pending'), ",
            $by_admin,
            " FROM ",
            $table,
            " t WHERE t.id = $1 AND t.status = 'rejected' AND t.rejected_at IS NOT NULL"
        )
    };
}

fn rejected_sql(kind: TargetKind) -> &'static str {
    match kind {
        TargetKind::Idea => rejected_sql!("ideas", "t.rejected_by_admin"),
        TargetKind::Contribution => rejected_sql!("contributions", "false"),
        TargetKind::Report => rejected_sql!("reports", "false"),
    }
}

/// Statement of the decision against the text, `None` when there is none (pending, accepted,
/// published…).
pub async fn load(
    pool: &PgPool,
    prompts: &Prompts,
    target: Target,
) -> sqlx::Result<Option<StatementOfReasons>> {
    let rejected: Option<(Option<String>, NaiveDate, NaiveDate, bool, bool, bool)> =
        sqlx::query_as(rejected_sql(target.kind))
            .bind(target.id)
            .bind(REJECTED_TEXT_RETENTION_DAYS)
            .bind(i32::from(PURGE_SCHEDULE.hour))
            .bind(i32::from(PURGE_SCHEDULE.minute))
            .bind(target.kind.as_str())
            .fetch_optional(pool)
            .await?;
    if let Some((reason_code, decided_on, purge_on, second_opinion, review_pending, by_admin)) =
        rejected
    {
        let reason_code = reason_code.unwrap_or_else(|| "rejected".to_owned());
        let rule = rules::article_for_reason(&reason_code).unwrap_or(rules::CRITERIA_ARTICLE);
        return Ok(Some(StatementOfReasons {
            measure: Measure::Rejected,
            reason_message: reason_message(prompts, target.kind, &reason_code),
            reason_code,
            rule: rule.into(),
            // Refused by the administrator (§8): a human decision, whatever the screening said.
            automated: !by_admin,
            second_opinion: second_opinion && !by_admin,
            decided_on,
            // A pending human review keeps the text until the administrator handles it.
            purge_on: (!review_pending).then_some(purge_on),
        }));
    }
    if target.kind != TargetKind::Idea {
        return Ok(None);
    }
    let hidden: Option<(Option<String>, NaiveDate, NaiveDate, bool)> = sqlx::query_as(HIDDEN_SQL)
        .bind(target.id)
        .bind(HIDDEN_TEXT_RETENTION_MONTHS)
        .bind(i32::from(PURGE_SCHEDULE.hour))
        .bind(i32::from(PURGE_SCHEDULE.minute))
        .fetch_optional(pool)
        .await?;
    Ok(
        hidden.map(|(reason, decided_on, purge_on, review_pending)| {
            let by_admin = reason.as_deref() == Some(ADMIN_HIDDEN_REASON);
            StatementOfReasons {
                measure: Measure::Hidden,
                reason_code: HIDDEN_REASON_CODE.to_owned(),
                reason_message: if by_admin {
                    HIDDEN_BY_ADMIN_MESSAGE
                } else {
                    HIDDEN_AFTER_REPORT_MESSAGE
                }
                .to_owned(),
                rule: rules::MEASURES_ARTICLE.into(),
                automated: !by_admin,
                second_opinion: false,
                decided_on,
                // A pending human review keeps the text until the administrator handles it.
                purge_on: (!review_pending).then_some(purge_on),
            }
        }),
    )
}
