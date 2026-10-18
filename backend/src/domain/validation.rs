//! Normalization and validation of texts typed by anonymous visitors.
//!
//! The normalized text is what gets stored, screened and analysed: line endings are unified,
//! control and invisible formatting characters (zero-width, bidirectional overrides…) are
//! removed, horizontal whitespace is collapsed, blank lines are limited to one in a row and the
//! whole text is trimmed. Lengths are counted in Unicode characters after normalization.

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
    fn optional_text_may_be_empty() {
        let limits = TextLimits { min: 0, max: 10 };
        assert_eq!(validate_text("   ", limits), Ok(String::new()));
        assert_eq!(validate_text(" ok ", limits), Ok("ok".to_owned()));
    }
}
