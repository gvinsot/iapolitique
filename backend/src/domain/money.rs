//! Money helpers.
//!
//! - Idea amounts (gains, costs, scores) are **whole euros** in `i64` (`BIGINT` in the
//!   database), never floats (AGENTS.md §4.3).
//! - AI costs and unit prices are tiny: they are counted in **micro-euros** ([`MicroEur`]).

use std::fmt;
use std::iter::Sum;
use std::ops::Add;

/// An amount in millionths of a euro.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MicroEur(pub i64);

impl MicroEur {
    pub const ZERO: Self = Self(0);
    pub const PER_EUR: i64 = 1_000_000;

    pub const fn from_eur(euros: i64) -> Self {
        Self(euros.saturating_mul(Self::PER_EUR))
    }

    pub const fn saturating_add(self, other: Self) -> Self {
        Self(self.0.saturating_add(other.0))
    }

    /// Cost of `units` at `self` per million units (e.g. a price per million tokens), rounded
    /// **up** to the next micro-euro so that budgets are never underestimated.
    pub fn per_million(self, units: u64) -> Self {
        let product = i128::from(self.0) * i128::from(units);
        let cost = div_ceil_i128(product, 1_000_000);
        Self(i64::try_from(cost).unwrap_or(i64::MAX))
    }

    /// Cost of `count` calls at `self` per call.
    pub fn times(self, count: u64) -> Self {
        let product = i128::from(self.0) * i128::from(count);
        Self(i64::try_from(product).unwrap_or(i64::MAX))
    }
}

impl Add for MicroEur {
    type Output = Self;

    fn add(self, rhs: Self) -> Self {
        self.saturating_add(rhs)
    }
}

impl Sum for MicroEur {
    fn sum<I: Iterator<Item = Self>>(iter: I) -> Self {
        iter.fold(Self::ZERO, Add::add)
    }
}

impl fmt::Display for MicroEur {
    /// Decimal euros without trailing zeros, e.g. `0.0125 €`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let sign = if self.0 < 0 { "-" } else { "" };
        let abs = self.0.unsigned_abs();
        let whole = abs / 1_000_000;
        let frac = abs % 1_000_000;
        if frac == 0 {
            write!(f, "{sign}{whole} €")
        } else {
            let frac = format!("{frac:06}");
            write!(f, "{sign}{whole}.{} €", frac.trim_end_matches('0'))
        }
    }
}

fn div_ceil_i128(a: i128, b: i128) -> i128 {
    let q = a / b;
    if a % b != 0 && ((a > 0) == (b > 0)) {
        q + 1
    } else {
        q
    }
}

/// Error parsing a decimal amount.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ParseAmountError {
    #[error("empty amount")]
    Empty,
    #[error("not a decimal number")]
    Invalid,
    #[error("more than 6 decimals")]
    TooPrecise,
    #[error("amount too large")]
    Overflow,
}

/// Parses a decimal euro amount (`12`, `12.5`, `0,03`, `-1.25`) into micro-euros, exactly.
pub fn parse_eur_to_micro(input: &str) -> Result<MicroEur, ParseAmountError> {
    let s = input.trim();
    if s.is_empty() {
        return Err(ParseAmountError::Empty);
    }
    let (negative, digits) = match s.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, s.strip_prefix('+').unwrap_or(s)),
    };
    let (whole, frac) = match digits.split_once(['.', ',']) {
        Some((w, f)) => (w, f),
        None => (digits, ""),
    };
    let is_digits = |p: &str| p.bytes().all(|b| b.is_ascii_digit());
    if (whole.is_empty() && frac.is_empty()) || !is_digits(whole) || !is_digits(frac) {
        return Err(ParseAmountError::Invalid);
    }
    if frac.len() > 6 {
        return Err(ParseAmountError::TooPrecise);
    }
    let whole: i64 = if whole.is_empty() {
        0
    } else {
        whole.parse().map_err(|_| ParseAmountError::Overflow)?
    };
    let frac_micro: i64 = if frac.is_empty() {
        0
    } else {
        format!("{frac:0<6}")
            .parse()
            .map_err(|_| ParseAmountError::Invalid)?
    };
    let micro = whole
        .checked_mul(MicroEur::PER_EUR)
        .and_then(|w| w.checked_add(frac_micro))
        .ok_or(ParseAmountError::Overflow)?;
    Ok(MicroEur(if negative { -micro } else { micro }))
}

/// Narrow no-break space, the French thousands separator.
const THOUSANDS_SEP: char = '\u{202F}';
/// No-break space before the currency symbol.
const NBSP: char = '\u{00A0}';

/// Formats whole euros the French way: `1 234 567 €` (narrow no-break spaces).
pub fn format_eur(euros: i64) -> String {
    let digits = euros.unsigned_abs().to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3 + 4);
    if euros < 0 {
        out.push('-');
    }
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(THOUSANDS_SEP);
        }
        out.push(c);
    }
    out.push(NBSP);
    out.push('€');
    out
}

/// Formats whole euros compactly: `1,2 Md€`, `350 M€`, `12 k€`, `800 €`.
///
/// One decimal is kept below 10 units of the chosen scale (`2,5 M€`), none above (`35 M€`).
pub fn format_eur_compact(euros: i64) -> String {
    const SCALES: [(u128, &str); 3] = [(1_000_000_000, "Md€"), (1_000_000, "M€"), (1_000, "k€")];
    let abs = u128::from(euros.unsigned_abs());
    let sign = if euros < 0 { "-" } else { "" };
    for (divisor, unit) in SCALES {
        // Tenths of the unit, rounded half up. The largest scale worth at least one unit after
        // rounding wins, so 999 999 € reads `1 M€` rather than `1000 k€`; amounts below 1 000 €
        // are never rounded up to `1 k€`.
        let tenths = (abs * 10 + divisor / 2) / divisor;
        if tenths >= 10 && (abs >= divisor || divisor > 1_000) {
            let text = if tenths < 100 && tenths % 10 != 0 {
                format!("{},{}", tenths / 10, tenths % 10)
            } else {
                ((tenths + 5) / 10).to_string()
            };
            return format!("{sign}{text}{NBSP}{unit}");
        }
    }
    format!("{sign}{abs}{NBSP}€")
}

/// A low / median / high estimate in whole euros (§6.2).
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, utoipa::ToSchema,
)]
pub struct EurRange {
    pub low: i64,
    pub median: i64,
    pub high: i64,
}

/// An inconsistent [`EurRange`].
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RangeError {
    #[error("amounts must not be negative")]
    Negative,
    #[error("expected low <= median <= high")]
    Unordered,
}

impl EurRange {
    pub fn new(low: i64, median: i64, high: i64) -> Result<Self, RangeError> {
        let range = Self { low, median, high };
        range.validate().map(|()| range)
    }

    pub fn validate(&self) -> Result<(), RangeError> {
        if self.low < 0 || self.median < 0 || self.high < 0 {
            Err(RangeError::Negative)
        } else if self.low > self.median || self.median > self.high {
            Err(RangeError::Unordered)
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_decimal_amounts_exactly() {
        assert_eq!(parse_eur_to_micro("12"), Ok(MicroEur(12_000_000)));
        assert_eq!(parse_eur_to_micro("12.5"), Ok(MicroEur(12_500_000)));
        assert_eq!(parse_eur_to_micro(" 0,03 "), Ok(MicroEur(30_000)));
        assert_eq!(parse_eur_to_micro(".5"), Ok(MicroEur(500_000)));
        assert_eq!(parse_eur_to_micro("3."), Ok(MicroEur(3_000_000)));
        assert_eq!(parse_eur_to_micro("+1"), Ok(MicroEur(1_000_000)));
        assert_eq!(parse_eur_to_micro("-1.25"), Ok(MicroEur(-1_250_000)));
        assert_eq!(parse_eur_to_micro("0.000001"), Ok(MicroEur(1)));
    }

    #[test]
    fn rejects_invalid_amounts() {
        assert_eq!(parse_eur_to_micro(""), Err(ParseAmountError::Empty));
        assert_eq!(parse_eur_to_micro("abc"), Err(ParseAmountError::Invalid));
        assert_eq!(parse_eur_to_micro("1.2.3"), Err(ParseAmountError::Invalid));
        assert_eq!(parse_eur_to_micro("."), Err(ParseAmountError::Invalid));
        assert_eq!(parse_eur_to_micro("1e3"), Err(ParseAmountError::Invalid));
        assert_eq!(parse_eur_to_micro("--1"), Err(ParseAmountError::Invalid));
        assert_eq!(
            parse_eur_to_micro("0.0000001"),
            Err(ParseAmountError::TooPrecise)
        );
        assert_eq!(
            parse_eur_to_micro("99999999999999999999"),
            Err(ParseAmountError::Overflow)
        );
    }

    #[test]
    fn computes_token_costs_rounding_up() {
        // 2 €/Mtok, 1 500 tokens = 0.003 € = 3 000 µ€.
        assert_eq!(MicroEur::from_eur(2).per_million(1_500), MicroEur(3_000));
        // 0.1 €/Mtok, 1 token = 0.1 µ€ -> rounded up to 1 µ€.
        assert_eq!(MicroEur(100_000).per_million(1), MicroEur(1));
        assert_eq!(MicroEur(100_000).per_million(0), MicroEur::ZERO);
        assert_eq!(MicroEur(30_000).times(3), MicroEur(90_000));
        assert_eq!(MicroEur(i64::MAX).times(2), MicroEur(i64::MAX));
    }

    #[test]
    fn sums_and_displays_micro_euros() {
        let total: MicroEur = [MicroEur(1), MicroEur(2_000_000), MicroEur(12_499)]
            .into_iter()
            .sum();
        assert_eq!(total, MicroEur(2_012_500));
        assert_eq!(total.to_string(), "2.0125 €");
        assert_eq!(MicroEur::from_eur(5).to_string(), "5 €");
        assert_eq!(MicroEur(-500_000).to_string(), "-0.5 €");
        assert_eq!(MicroEur(i64::MAX) + MicroEur(1), MicroEur(i64::MAX));
    }

    #[test]
    fn formats_euros_the_french_way() {
        assert_eq!(format_eur(0), "0\u{a0}€");
        assert_eq!(format_eur(999), "999\u{a0}€");
        assert_eq!(format_eur(1_000), "1\u{202f}000\u{a0}€");
        assert_eq!(format_eur(1_234_567), "1\u{202f}234\u{202f}567\u{a0}€");
        assert_eq!(format_eur(-45_000), "-45\u{202f}000\u{a0}€");
        assert_eq!(
            format_eur(i64::MIN),
            "-9\u{202f}223\u{202f}372\u{202f}036\u{202f}854\u{202f}775\u{202f}808\u{a0}€"
        );
    }

    #[test]
    fn formats_compact_amounts() {
        assert_eq!(format_eur_compact(800), "800\u{a0}€");
        assert_eq!(format_eur_compact(1_000), "1\u{a0}k€");
        assert_eq!(format_eur_compact(12_400), "12\u{a0}k€");
        assert_eq!(format_eur_compact(2_500_000), "2,5\u{a0}M€");
        assert_eq!(format_eur_compact(9_960_000), "10\u{a0}M€");
        assert_eq!(format_eur_compact(350_000_000), "350\u{a0}M€");
        assert_eq!(format_eur_compact(1_240_000_000), "1,2\u{a0}Md€");
        assert_eq!(format_eur_compact(-3_000_000), "-3\u{a0}M€");
        assert_eq!(format_eur_compact(999_999), "1\u{a0}M€");
        assert_eq!(format_eur_compact(940_000), "940\u{a0}k€");
        assert_eq!(format_eur_compact(999), "999\u{a0}€");
    }

    #[test]
    fn validates_ranges() {
        assert!(EurRange::new(1, 2, 3).is_ok());
        assert!(EurRange::new(0, 0, 0).is_ok());
        assert_eq!(EurRange::new(3, 2, 4), Err(RangeError::Unordered));
        assert_eq!(EurRange::new(1, 2, 1), Err(RangeError::Unordered));
        assert_eq!(EurRange::new(-1, 2, 3), Err(RangeError::Negative));
    }
}
