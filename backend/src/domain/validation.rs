//! Normalization and validation of texts typed by anonymous visitors.
//!
//! The normalized text is what gets stored, screened and analysed: line endings are unified,
//! control and invisible formatting characters (zero-width, bidirectional overrides…) are
//! removed, full-width ASCII forms become ASCII, horizontal whitespace is collapsed, blank lines
//! are limited to one in a row and the whole text is trimmed. Lengths are counted in Unicode
//! characters after normalization.

/// Inclusive bounds of a text length, in characters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextLimits {
    pub min: usize,
    pub max: usize,
}

/// Idea submission text (AGENTS.md §5.1): 50 to 4 000 characters.
pub const IDEA_TEXT_LIMITS: TextLimits = TextLimits {
    min: 50,
    max: 4_000,
};

/// Contribution text (improvement or objection, AGENTS.md §5.5): 30 to 2 000 characters.
pub const CONTRIBUTION_TEXT_LIMITS: TextLimits = TextLimits {
    min: 30,
    max: 2_000,
};

/// Optional details of a report (§5.6): up to 1 000 characters.
pub const REPORT_DETAILS_LIMITS: TextLimits = TextLimits { min: 0, max: 1_000 };

/// Optional message of a human review request (§7.1): up to 1 000 characters.
pub const REVIEW_MESSAGE_LIMITS: TextLimits = TextLimits { min: 0, max: 1_000 };

/// Optional territory of an idea (§5.1).
pub const TERRITORY_MAX_CHARS: usize = 120;
/// Optional name of the author of a report (§7.5).
pub const REPORTER_NAME_MAX_CHARS: usize = 100;
/// Optional e-mail of the author of a report (§7.5), RFC 5321 bound.
pub const REPORTER_EMAIL_MAX_CHARS: usize = 254;

/// A text outside its limits. Messages are in French: they are shown to the author.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TextError {
    #[error("Le texte est vide.")]
    Empty,
    #[error("Le texte doit contenir au moins {min} caractères (actuellement {actual}).")]
    TooShort { min: usize, actual: usize },
    #[error("Le texte doit contenir au plus {max} caractères (actuellement {actual}).")]
    TooLong { max: usize, actual: usize },
}

impl TextError {
    /// Stable machine-readable code for the API error body.
    pub const fn code(&self) -> &'static str {
        match self {
            Self::Empty => "text_empty",
            Self::TooShort { .. } => "text_too_short",
            Self::TooLong { .. } => "text_too_long",
        }
    }
}

/// Invisible characters that can hide content or reorder text, removed from user input.
fn is_invisible_format_char(c: char) -> bool {
    matches!(
        c,
        '\u{00AD}'                  // soft hyphen
            | '\u{061C}'            // arabic letter mark
            | '\u{180E}'            // mongolian vowel separator
            | '\u{200B}'..='\u{200F}' // zero-width chars, LRM, RLM
            | '\u{202A}'..='\u{202E}' // bidi embeddings and overrides
            | '\u{2060}'..='\u{2064}' // word joiner, invisible operators
            | '\u{2066}'..='\u{2069}' // bidi isolates
            | '\u{FEFF}'            // byte order mark
            | '\u{FFF9}'..='\u{FFFB}' // interlinear annotations
    )
}

/// Normalizes a text without checking its length.
pub fn normalize_text(raw: &str) -> String {
    let mut lines: Vec<String> = Vec::new();
    let mut current = String::with_capacity(raw.len().min(16 * 1024));
    let mut chars = raw.chars().peekable();
    let mut last_was_space = false;

    while let Some(c) = chars.next() {
        let c = match c {
            '\r' => {
                if chars.peek() == Some(&'\n') {
                    chars.next();
                }
                '\n'
            }
            '\u{2028}' | '\u{2029}' | '\u{0085}' => '\n',
            // Full-width ASCII forms (`＠`, `０６`…) become ASCII, so that the masking of
            // personal data (§7.5) sees them; NFKC would also rewrite `m²` or `½`.
            '\u{FF01}'..='\u{FF5E}' => char::from_u32(c as u32 - 0xFEE0).unwrap_or(c),
            c => c,
        };
        if c == '\n' {
            lines.push(std::mem::take(&mut current).trim_end().to_owned());
            last_was_space = false;
            continue;
        }
        if c.is_control() && c != '\t' {
            continue;
        }
        if is_invisible_format_char(c) {
            continue;
        }
        // Tabs and every Unicode space (no-break spaces included) become one plain space.
        if c == '\t' || c.is_whitespace() {
            if !last_was_space {
                current.push(' ');
                last_was_space = true;
            }
            continue;
        }
        current.push(c);
        last_was_space = false;
    }
    lines.push(current.trim_end().to_owned());

    let mut out = String::with_capacity(raw.len());
    let mut blank_run = 0usize;
    for line in lines {
        let line = line.trim_start_matches(' ').to_owned();
        if line.is_empty() {
            blank_run += 1;
            continue;
        }
        if !out.is_empty() {
            out.push_str(if blank_run > 0 { "\n\n" } else { "\n" });
        }
        blank_run = 0;
        out.push_str(&line);
    }
    out
}

/// Normalizes a text and checks its length against `limits`.
pub fn validate_text(raw: &str, limits: TextLimits) -> Result<String, TextError> {
    let text = normalize_text(raw);
    let len = text.chars().count();
    if len == 0 && limits.min > 0 {
        Err(TextError::Empty)
    } else if len < limits.min {
        Err(TextError::TooShort {
            min: limits.min,
            actual: len,
        })
    } else if len > limits.max {
        Err(TextError::TooLong {
            max: limits.max,
            actual: len,
        })
    } else {
        Ok(text)
    }
}

/// Validates the text of a new idea (50 to 4 000 characters after normalization).
pub fn validate_idea_text(raw: &str) -> Result<String, TextError> {
    validate_text(raw, IDEA_TEXT_LIMITS)
}

/// Validates the text of a contribution (30 to 2 000 characters after normalization).
pub fn validate_contribution_text(raw: &str) -> Result<String, TextError> {
    validate_text(raw, CONTRIBUTION_TEXT_LIMITS)
}

/// Validates an optional free text (`None` or blank: `None`).
pub fn validate_optional_text(
    raw: Option<&str>,
    limits: TextLimits,
) -> Result<Option<String>, TextError> {
    match raw {
        None => Ok(None),
        Some(raw) => validate_text(raw, limits).map(|text| Some(text).filter(|t| !t.is_empty())),
    }
}

/// A short form field (territory, name, e-mail) that cannot be accepted. Messages are in
/// French: they are shown to the visitor.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum FieldError {
    #[error("{label} doit contenir au plus {max} caractères (actuellement {actual}).")]
    TooLong {
        /// French name of the field, capitalized (« Le territoire »).
        label: &'static str,
        max: usize,
        actual: usize,
    },
    #[error("L'adresse électronique indiquée n'est pas valide.")]
    InvalidEmail,
}

impl FieldError {
    pub const fn code(&self) -> &'static str {
        match self {
            Self::TooLong { .. } => "field_too_long",
            Self::InvalidEmail => "invalid_email",
        }
    }
}

/// Normalizes an optional single-line field: line breaks become spaces, blank becomes `None`.
pub fn validate_line(
    raw: Option<&str>,
    label: &'static str,
    max: usize,
) -> Result<Option<String>, FieldError> {
    let Some(raw) = raw else {
        return Ok(None);
    };
    let line = normalize_text(raw)
        .split('\n')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    let actual = line.chars().count();
    if actual == 0 {
        Ok(None)
    } else if actual > max {
        Err(FieldError::TooLong { label, max, actual })
    } else {
        Ok(Some(line))
    }
}

/// Plausible e-mail address: `local@domain.tld`, no whitespace, at most 254 characters. The
/// address is never contacted automatically; this only catches typing mistakes.
pub fn validate_email(raw: Option<&str>) -> Result<Option<String>, FieldError> {
    let Some(email) = validate_line(raw, "L'adresse électronique", REPORTER_EMAIL_MAX_CHARS)?
    else {
        return Ok(None);
    };
    let plausible = match email.split_once('@') {
        Some((local, domain)) => {
            !local.is_empty()
                && local.chars().count() <= 64
                && !domain.contains('@')
                && !email.contains(char::is_whitespace)
                && domain.contains('.')
                && domain.split('.').all(|label| {
                    !label.is_empty() && !label.starts_with('-') && !label.ends_with('-')
                })
        }
        None => false,
    };
    if plausible {
        Ok(Some(email))
    } else {
        Err(FieldError::InvalidEmail)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const VALID: &str = "Mutualiser les achats de fournitures entre les communes voisines.";

    #[test]
    fn keeps_a_clean_text_unchanged() {
        assert_eq!(validate_idea_text(VALID).unwrap(), VALID);
    }

    #[test]
    fn trims_and_unifies_line_endings() {
        let raw = format!("  \r\n{VALID}\r\nSecond line.\rThird.  \n\n");
        assert_eq!(
            validate_idea_text(&raw).unwrap(),
            format!("{VALID}\nSecond line.\nThird.")
        );
    }

    #[test]
    fn limits_blank_lines_to_one() {
        let raw = format!("{VALID}\n\n\n\n  \n\t\nNext paragraph.");
        assert_eq!(normalize_text(&raw), format!("{VALID}\n\nNext paragraph."));
    }

    #[test]
    fn strips_control_and_invisible_characters() {
        let raw = "A\u{0000}B\u{0007}C\u{001B}[31mD\u{200B}E\u{202E}F\u{2066}G\u{FEFF}H\u{00AD}I\u{007F}J";
        assert_eq!(normalize_text(raw), "ABC[31mDEFGHIJ");
    }

    #[test]
    fn collapses_horizontal_whitespace() {
        assert_eq!(
            normalize_text("a \t  b\u{00A0}\u{00A0}c\u{3000}d"),
            "a b c d"
        );
        assert_eq!(normalize_text("   indented\n    line"), "indented\nline");
    }

    #[test]
    fn unicode_line_separators_become_newlines() {
        assert_eq!(normalize_text("a\u{2028}b\u{2029}c\u{0085}d"), "a\nb\nc\nd");
    }

    #[test]
    fn folds_full_width_ascii() {
        assert_eq!(
            normalize_text("jean＠exemple．fr ０６ １２ ３４ ５６ ７８"),
            "jean@exemple.fr 06 12 34 56 78"
        );
        assert_eq!(normalize_text("10 m² ½"), "10 m² ½");
    }

    #[test]
    fn keeps_french_typography() {
        let text = "« Économies » : l’État, 10 % — œuvre… ¿";
        assert_eq!(normalize_text(text), text);
    }

    #[test]
    fn rejects_empty_and_short_texts() {
        assert_eq!(validate_idea_text("  \n\u{200B} "), Err(TextError::Empty));
        let err = validate_idea_text("Trop court").unwrap_err();
        assert_eq!(
            err,
            TextError::TooShort {
                min: 50,
                actual: 10
            }
        );
        assert_eq!(err.code(), "text_too_short");
        assert_eq!(
            err.to_string(),
            "Le texte doit contenir au moins 50 caractères (actuellement 10)."
        );
    }

    #[test]
    fn counts_characters_not_bytes() {
        // 50 accented characters = 100 bytes: accepted.
        let fifty = "é".repeat(50);
        assert_eq!(validate_idea_text(&fifty).unwrap().chars().count(), 50);
        let err = validate_idea_text(&"é".repeat(49)).unwrap_err();
        assert_eq!(
            err,
            TextError::TooShort {
                min: 50,
                actual: 49
            }
        );
    }

    #[test]
    fn enforces_the_upper_bound_after_normalization() {
        assert!(validate_idea_text(&"x".repeat(4_000)).is_ok());
        let err = validate_idea_text(&"x".repeat(4_001)).unwrap_err();
        assert_eq!(
            err,
            TextError::TooLong {
                max: 4_000,
                actual: 4_001
            }
        );
        assert_eq!(err.code(), "text_too_long");
        // Padding with spaces does not count once collapsed.
        let padded = format!("{}{}", "x ".repeat(1_990), " ".repeat(5_000));
        assert!(validate_idea_text(&padded).is_ok());
    }

    #[test]
    fn contribution_limits() {
        assert!(validate_contribution_text(&"x".repeat(30)).is_ok());
        assert_eq!(
            validate_contribution_text(&"x".repeat(29)),
            Err(TextError::TooShort {
                min: 30,
                actual: 29
            })
        );
        assert!(validate_contribution_text(&"x".repeat(2_001)).is_err());
    }

    #[test]
    fn optional_texts() {
        assert_eq!(
            validate_optional_text(None, REPORT_DETAILS_LIMITS),
            Ok(None)
        );
        assert_eq!(
            validate_optional_text(Some(" \n "), REPORT_DETAILS_LIMITS),
            Ok(None)
        );
        assert_eq!(
            validate_optional_text(Some(" Le chiffre est faux. "), REPORT_DETAILS_LIMITS),
            Ok(Some("Le chiffre est faux.".to_owned()))
        );
        assert!(validate_optional_text(Some(&"x".repeat(1_001)), REPORT_DETAILS_LIMITS).is_err());
    }

    #[test]
    fn single_line_fields() {
        assert_eq!(validate_line(None, "Le territoire", 10), Ok(None));
        assert_eq!(validate_line(Some("   "), "Le territoire", 10), Ok(None));
        assert_eq!(
            validate_line(Some(" Saint-\r\n  Étienne\u{200B} "), "Le territoire", 20),
            Ok(Some("Saint- Étienne".to_owned()))
        );
        let err = validate_line(Some("abcdefghijk"), "Le territoire", 10).unwrap_err();
        assert_eq!(
            err.to_string(),
            "Le territoire doit contenir au plus 10 caractères (actuellement 11)."
        );
        assert_eq!(err.code(), "field_too_long");
    }

    #[test]
    fn email_addresses() {
        for ok in [
            "a@b.fr",
            " prenom.nom+tag@exemple.co.uk ",
            "élise@exemple.fr",
        ] {
            assert!(validate_email(Some(ok)).unwrap().is_some(), "{ok}");
        }
        assert_eq!(validate_email(Some("")), Ok(None));
        for bad in [
            "pas une adresse",
            "@exemple.fr",
            "a@",
            "a@b",
            "a@@b.fr",
            "a@b..fr",
            "a@-b.fr",
            "a b@c.fr",
        ] {
            assert_eq!(
                validate_email(Some(bad)),
                Err(FieldError::InvalidEmail),
                "{bad}"
            );
        }
        let long = format!("{}@exemple.fr", "a".repeat(250));
        assert!(matches!(
            validate_email(Some(&long)),
            Err(FieldError::TooLong { .. })
        ));
    }

    #[test]
    fn optional_text_may_be_empty() {
        let limits = TextLimits { min: 0, max: 10 };
        assert_eq!(validate_text("   ", limits), Ok(String::new()));
        assert_eq!(validate_text(" ok ", limits), Ok("ok".to_owned()));
    }
}
