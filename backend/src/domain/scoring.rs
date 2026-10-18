//! Composite score (AGENTS.md §6.3), implemented **only here**.
//!
//! ```text
//! net_annual   = annual_gain.median − implementation_cost.median / 5   (cost spread over 5 years)
//! feasibility  = feasibility / 5
//! confidence   = { low: 0.5, medium: 0.75, high: 1.0 }
//! delay        = 1 / (1 + time_to_effect_months / 24)
//! score        = net_annual × feasibility × confidence × delay
//! ```
//!
//! All factors are rational, so the score is computed **exactly** with integers and rounded
//! once (half away from zero) to whole euros: the ranking never depends on floating point
//! noise. The `*_factor` fields are only informative (display).
//!
//! Any change to the formula **must** bump [`SCORING_VERSION`] and update the public
//! methodology page in the same commit (§6.5).

use serde::Serialize;
use utoipa::ToSchema;

use super::types::Confidence;

/// Version of the formula, stored with every analysis.
pub const SCORING_VERSION: &str = "1";

/// Years over which the one-off implementation cost is spread.
pub const AMORTIZATION_YEARS: i64 = 5;

/// Highest feasibility grade.
pub const MAX_FEASIBILITY: u8 = 5;

/// Delay (months) at which the delay factor is halved.
pub const DELAY_HALF_LIFE_MONTHS: u32 = 24;

/// Upper bound accepted for `time_to_effect_months` (50 years).
pub const MAX_TIME_TO_EFFECT_MONTHS: u32 = 600;

/// The formula as shown on the methodology page (French, public).
pub const FORMULA_FR: &str = "\
gain_net_annuel = gain_annuel.médian − coût_de_mise_en_œuvre.médian / 5   (coût amorti sur 5 ans)
faisabilité     = faisabilité / 5
confiance       = { faible : 0,5 ; moyenne : 0,75 ; élevée : 1,0 }
délai           = 1 / (1 + délai_avant_effets_en_mois / 24)
score           = gain_net_annuel × faisabilité × confiance × délai";

/// Inputs of the score, taken from an analysis (§6.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScoreInput {
    /// Median of `annual_gain_eur`.
    pub annual_gain_median_eur: i64,
    /// Median of `implementation_cost_eur`.
    pub implementation_cost_median_eur: i64,
    /// `feasibility`, 1 to 5.
    pub feasibility: u8,
    pub confidence: Confidence,
    pub time_to_effect_months: u32,
}

/// Score and its components, displayed on every idea.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, ToSchema)]
pub struct ScoreBreakdown {
    /// Version of the formula ([`SCORING_VERSION`]).
    pub scoring_version: &'static str,
    /// Annual gain minus the amortized implementation cost, in euros (may be negative).
    pub net_annual_eur: i64,
    /// `feasibility / 5`.
    pub feasibility_factor: f64,
    /// 0.5, 0.75 or 1.0.
    pub confidence_factor: f64,
    /// `1 / (1 + months / 24)`.
    pub delay_factor: f64,
    /// Final score in euros per year, used for the default ranking.
    pub score: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ScoringError {
    #[error("feasibility must be between 1 and 5, got {0}")]
    Feasibility(u8),
    #[error("amounts must not be negative")]
    NegativeAmount,
    #[error("time to effect must be at most {MAX_TIME_TO_EFFECT_MONTHS} months, got {0}")]
    Delay(u32),
}

/// Confidence factor as the fraction `numerator / 4`.
const fn confidence_quarters(confidence: Confidence) -> i128 {
    match confidence {
        Confidence::Low => 2,
        Confidence::Medium => 3,
        Confidence::High => 4,
    }
}

/// Integer division rounded half away from zero (`den > 0`).
fn div_round(num: i128, den: i128) -> i128 {
    debug_assert!(den > 0);
    let half = den / 2;
    if num >= 0 {
        (num + half) / den
    } else {
        -((-num + half) / den)
    }
}

fn clamp_to_i64(value: i128) -> i64 {
    i64::try_from(value).unwrap_or(if value < 0 { i64::MIN } else { i64::MAX })
}

/// Computes the score of an analysis.
pub fn compute(input: &ScoreInput) -> Result<ScoreBreakdown, ScoringError> {
    if !(1..=MAX_FEASIBILITY).contains(&input.feasibility) {
        return Err(ScoringError::Feasibility(input.feasibility));
    }
    if input.annual_gain_median_eur < 0 || input.implementation_cost_median_eur < 0 {
        return Err(ScoringError::NegativeAmount);
    }
    if input.time_to_effect_months > MAX_TIME_TO_EFFECT_MONTHS {
        return Err(ScoringError::Delay(input.time_to_effect_months));
    }

    let years = i128::from(AMORTIZATION_YEARS);
    let half_life = i128::from(DELAY_HALF_LIFE_MONTHS);
    let max_feasibility = i128::from(MAX_FEASIBILITY);

    // net_annual × 5 = 5 × gain − cost
    let net_times_years = years * i128::from(input.annual_gain_median_eur)
        - i128::from(input.implementation_cost_median_eur);
    let feasibility = i128::from(input.feasibility);
    let quarters = confidence_quarters(input.confidence);
    let months = i128::from(input.time_to_effect_months);

    // score = (net×5)/5 × f/5 × q/4 × 24/(24+m)
    let numerator = net_times_years * feasibility * quarters * half_life;
    let denominator = years * max_feasibility * 4 * (half_life + months);

    Ok(ScoreBreakdown {
        scoring_version: SCORING_VERSION,
        net_annual_eur: clamp_to_i64(div_round(net_times_years, years)),
        feasibility_factor: f64::from(input.feasibility) / f64::from(MAX_FEASIBILITY),
        confidence_factor: quarters as f64 / 4.0,
        delay_factor: f64::from(DELAY_HALF_LIFE_MONTHS)
            / f64::from(DELAY_HALF_LIFE_MONTHS + input.time_to_effect_months),
        score: clamp_to_i64(div_round(numerator, denominator)),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(
        gain: i64,
        cost: i64,
        feasibility: u8,
        confidence: Confidence,
        months: u32,
    ) -> ScoreInput {
        ScoreInput {
            annual_gain_median_eur: gain,
            implementation_cost_median_eur: cost,
            feasibility,
            confidence,
            time_to_effect_months: months,
        }
    }

    /// Straightforward floating point transcription of AGENTS.md §6.3, used as an oracle.
    fn reference(i: &ScoreInput) -> f64 {
        let net = i.annual_gain_median_eur as f64 - i.implementation_cost_median_eur as f64 / 5.0;
        let feasibility = f64::from(i.feasibility) / 5.0;
        let confidence = match i.confidence {
            Confidence::Low => 0.5,
            Confidence::Medium => 0.75,
            Confidence::High => 1.0,
        };
        let delay = 1.0 / (1.0 + f64::from(i.time_to_effect_months) / 24.0);
        net * feasibility * confidence * delay
    }

    #[test]
    fn perfect_idea_scores_its_net_gain() {
        let s = compute(&input(1_000_000, 0, 5, Confidence::High, 0)).unwrap();
        assert_eq!(s.score, 1_000_000);
        assert_eq!(s.net_annual_eur, 1_000_000);
        assert_eq!(s.feasibility_factor, 1.0);
        assert_eq!(s.confidence_factor, 1.0);
        assert_eq!(s.delay_factor, 1.0);
        assert_eq!(s.scoring_version, SCORING_VERSION);
    }

    #[test]
    fn worked_example() {
        // net = 10 M − 5 M / 5 = 9 M ; × 3/5 × 0.75 × 1/(1 + 12/24) = 9 M × 0.6 × 0.75 × 2/3
        let s = compute(&input(10_000_000, 5_000_000, 3, Confidence::Medium, 12)).unwrap();
        assert_eq!(s.net_annual_eur, 9_000_000);
        assert_eq!(s.score, 2_700_000);
        assert!((s.feasibility_factor - 0.6).abs() < 1e-12);
        assert!((s.confidence_factor - 0.75).abs() < 1e-12);
        assert!((s.delay_factor - 2.0 / 3.0).abs() < 1e-12);
    }

    #[test]
    fn confidence_factors() {
        let base = |c| compute(&input(1_000, 0, 5, c, 0)).unwrap();
        assert_eq!(base(Confidence::Low).score, 500);
        assert_eq!(base(Confidence::Medium).score, 750);
        assert_eq!(base(Confidence::High).score, 1_000);
        assert_eq!(base(Confidence::Low).confidence_factor, 0.5);
    }

    #[test]
    fn delay_halves_the_score_at_24_months() {
        let s = compute(&input(1_000_000, 0, 5, Confidence::High, 24)).unwrap();
        assert_eq!(s.score, 500_000);
        assert_eq!(s.delay_factor, 0.5);
        let s = compute(&input(1_000_000, 0, 5, Confidence::High, 72)).unwrap();
        assert_eq!(s.score, 250_000);
    }

    #[test]
    fn implementation_cost_is_spread_over_five_years() {
        let s = compute(&input(100, 500, 5, Confidence::High, 0)).unwrap();
        assert_eq!(s.net_annual_eur, 0);
        assert_eq!(s.score, 0);
    }

    #[test]
    fn expensive_idea_has_a_negative_score() {
        let s = compute(&input(100, 1_000, 5, Confidence::High, 0)).unwrap();
        assert_eq!(s.net_annual_eur, -100);
        assert_eq!(s.score, -100);
    }

    #[test]
    fn rounds_half_away_from_zero() {
        // net = 1 − 0 ; × 1/5 × 0.5 × 1 = 0.1 -> 0
        assert_eq!(
            compute(&input(1, 0, 1, Confidence::Low, 0)).unwrap().score,
            0
        );
        // 5 × 1/5 × 0.5 = 0.5 -> 1
        assert_eq!(
            compute(&input(5, 0, 1, Confidence::Low, 0)).unwrap().score,
            1
        );
        // net = (0 − 3) / 5 = −0.6 -> −1 ; score = −0.6 × 1 = −0.6 -> −1
        let s = compute(&input(0, 3, 5, Confidence::High, 0)).unwrap();
        assert_eq!(s.net_annual_eur, -1);
        assert_eq!(s.score, -1);
        // −0.5 -> −1
        assert_eq!(div_round(-1, 2), -1);
        assert_eq!(div_round(1, 2), 1);
        assert_eq!(div_round(-4, 10), 0);
    }

    #[test]
    fn matches_the_floating_point_formula() {
        let gains = [0, 1, 999, 1_000_000, 37_500_000, 2_000_000_000];
        let costs = [0, 7, 1_000_000, 50_000_000];
        let months = [0, 1, 6, 12, 24, 36, 120, 600];
        for gain in gains {
            for cost in costs {
                for feasibility in 1..=5 {
                    for confidence in Confidence::ALL {
                        for m in months {
                            let i = input(gain, cost, feasibility, *confidence, m);
                            let exact = compute(&i).unwrap().score as f64;
                            let float = reference(&i);
                            assert!(
                                (exact - float).abs() <= 0.5 + float.abs() * 1e-12,
                                "{i:?}: {exact} vs {float}"
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn ranking_is_monotonic_in_each_factor() {
        let score = |i: ScoreInput| compute(&i).unwrap().score;
        let base = input(10_000_000, 1_000_000, 3, Confidence::Medium, 12);
        assert!(
            score(ScoreInput {
                feasibility: 4,
                ..base
            }) > score(base)
        );
        assert!(
            score(ScoreInput {
                confidence: Confidence::High,
                ..base
            }) > score(base)
        );
        assert!(
            score(ScoreInput {
                time_to_effect_months: 6,
                ..base
            }) > score(base)
        );
        assert!(
            score(ScoreInput {
                annual_gain_median_eur: 11_000_000,
                ..base
            }) > score(base)
        );
        assert!(
            score(ScoreInput {
                implementation_cost_median_eur: 5_000_000,
                ..base
            }) < score(base)
        );
    }

    #[test]
    fn huge_amounts_do_not_overflow() {
        let s = compute(&input(i64::MAX, 0, 5, Confidence::High, 0)).unwrap();
        assert_eq!(s.score, i64::MAX);
        let s = compute(&input(0, i64::MAX, 5, Confidence::High, 0)).unwrap();
        assert_eq!(s.net_annual_eur, -(i64::MAX / 5));
    }

    #[test]
    fn rejects_invalid_inputs() {
        assert_eq!(
            compute(&input(1, 0, 0, Confidence::High, 0)),
            Err(ScoringError::Feasibility(0))
        );
        assert_eq!(
            compute(&input(1, 0, 6, Confidence::High, 0)),
            Err(ScoringError::Feasibility(6))
        );
        assert_eq!(
            compute(&input(-1, 0, 3, Confidence::High, 0)),
            Err(ScoringError::NegativeAmount)
        );
        assert_eq!(
            compute(&input(1, -1, 3, Confidence::High, 0)),
            Err(ScoringError::NegativeAmount)
        );
        assert_eq!(
            compute(&input(1, 0, 3, Confidence::High, 601)),
            Err(ScoringError::Delay(601))
        );
    }

    #[test]
    fn formula_text_mentions_every_factor() {
        for word in ["médian", "/ 5", "0,5", "0,75", "1,0", "/ 24"] {
            assert!(FORMULA_FR.contains(word), "{word}");
        }
    }
}
