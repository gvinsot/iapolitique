//! URL slugs for published ideas, built from their (French) title.
//!
//! `"Réduire le coût des véhicules de l'État"` → `"reduire-le-cout-des-vehicules-de-l-etat"`.
//! Uniqueness is enforced by the database; on conflict call [`with_suffix`].

/// Maximum length of a slug, suffix included.
pub const MAX_SLUG_LEN: usize = 80;

/// Slug used when the title contains no letter or digit.
pub const FALLBACK_SLUG: &str = "idee";

/// ASCII transliteration of a Latin character, or `None` when it has no letter equivalent.
fn transliterate(c: char) -> Option<&'static str> {
    Some(match c {
        'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' | 'ā' | 'ă' | 'ą' | 'À' | 'Á' | 'Â' | 'Ã' | 'Ä' | 'Å'
        | 'Ā' | 'Ă' | 'Ą' => "a",
        'æ' | 'Æ' => "ae",
        'ç' | 'ć' | 'ĉ' | 'ċ' | 'č' | 'Ç' | 'Ć' | 'Ĉ' | 'Ċ' | 'Č' => "c",
        'ď' | 'đ' | 'Ď' | 'Đ' => "d",
        'è' | 'é' | 'ê' | 'ë' | 'ē' | 'ĕ' | 'ė' | 'ę' | 'ě' | 'È' | 'É' | 'Ê' | 'Ë' | 'Ē' | 'Ĕ'
        | 'Ė' | 'Ę' | 'Ě' => "e",
        'ĝ' | 'ğ' | 'ġ' | 'ģ' | 'Ĝ' | 'Ğ' | 'Ġ' | 'Ģ' => "g",
        'ĥ' | 'ħ' | 'Ĥ' | 'Ħ' => "h",
        'ì' | 'í' | 'î' | 'ï' | 'ĩ' | 'ī' | 'ĭ' | 'į' | 'ı' | 'Ì' | 'Í' | 'Î' | 'Ï' | 'Ĩ' | 'Ī'
        | 'Ĭ' | 'Į' | 'İ' => "i",
        'ĳ' | 'Ĳ' => "ij",
        'ĵ' | 'Ĵ' => "j",
        'ķ' | 'Ķ' => "k",
        'ĺ' | 'ļ' | 'ľ' | 'ŀ' | 'ł' | 'Ĺ' | 'Ļ' | 'Ľ' | 'Ŀ' | 'Ł' => "l",
        'ñ' | 'ń' | 'ņ' | 'ň' | 'Ñ' | 'Ń' | 'Ņ' | 'Ň' => "n",
        'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ø' | 'ō' | 'ŏ' | 'ő' | 'Ò' | 'Ó' | 'Ô' | 'Õ' | 'Ö' | 'Ø'
        | 'Ō' | 'Ŏ' | 'Ő' => "o",
        'œ' | 'Œ' => "oe",
        'ŕ' | 'ŗ' | 'ř' | 'Ŕ' | 'Ŗ' | 'Ř' => "r",
        'ś' | 'ŝ' | 'ş' | 'š' | 'Ś' | 'Ŝ' | 'Ş' | 'Š' => "s",
        'ß' => "ss",
        'ţ' | 'ť' | 'ŧ' | 'Ţ' | 'Ť' | 'Ŧ' => "t",
        'ù' | 'ú' | 'û' | 'ü' | 'ũ' | 'ū' | 'ŭ' | 'ů' | 'ű' | 'ų' | 'Ù' | 'Ú' | 'Û' | 'Ü' | 'Ũ'
        | 'Ū' | 'Ŭ' | 'Ů' | 'Ű' | 'Ų' => "u",
        'ŵ' | 'Ŵ' => "w",
        'ý' | 'ÿ' | 'ŷ' | 'Ý' | 'Ÿ' | 'Ŷ' => "y",
        'ź' | 'ż' | 'ž' | 'Ź' | 'Ż' | 'Ž' => "z",
        _ => return None,
    })
}

/// Builds a slug: lowercase ASCII letters and digits separated by single dashes, at most
/// [`MAX_SLUG_LEN`] characters, cut at a word boundary when possible.
pub fn slugify(title: &str) -> String {
    let mut slug = String::with_capacity(title.len());
    let mut pending_dash = false;
    for c in title.chars() {
        let piece: Option<&str> = if c.is_ascii_alphanumeric() {
            None
        } else {
            match transliterate(c) {
                Some(ascii) => Some(ascii),
                None => {
                    // Separator (space, apostrophe, punctuation, unsupported script…).
                    pending_dash = !slug.is_empty();
                    continue;
                }
            }
        };
        if pending_dash {
            slug.push('-');
            pending_dash = false;
        }
        match piece {
            None => slug.push(c.to_ascii_lowercase()),
            Some(ascii) => slug.push_str(ascii),
        }
    }
    let slug = truncate_at_word(&slug, MAX_SLUG_LEN);
    if slug.is_empty() {
        FALLBACK_SLUG.to_owned()
    } else {
        slug.to_owned()
    }
}

/// Appends `-n` to `base` (n ≥ 2) while keeping the result within [`MAX_SLUG_LEN`].
pub fn with_suffix(base: &str, n: u32) -> String {
    let suffix = format!("-{n}");
    let room = MAX_SLUG_LEN.saturating_sub(suffix.len());
    let base = truncate_at_word(base, room);
    let base = if base.is_empty() { FALLBACK_SLUG } else { base };
    format!("{base}{suffix}")
}

/// Truncates an ASCII slug to `max` bytes, preferably at a dash, never leaving a trailing dash.
fn truncate_at_word(slug: &str, max: usize) -> &str {
    if slug.len() <= max {
        return slug.trim_end_matches('-');
    }
    let cut = &slug[..max];
    let cut = match cut.rfind('-') {
        // Keep at least half of the room to avoid absurdly short slugs.
        Some(pos) if pos >= max / 2 && !slug[max..].starts_with('-') => &cut[..pos],
        _ => cut,
    };
    cut.trim_end_matches('-')
}

/// Whether `s` is a syntactically valid slug (used to validate path parameters).
pub fn is_valid_slug(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= MAX_SLUG_LEN
        && !s.starts_with('-')
        && !s.ends_with('-')
        && !s.contains("--")
        && s.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transliterates_french_accents() {
        assert_eq!(
            slugify("Réduire le coût des véhicules de l'État"),
            "reduire-le-cout-des-vehicules-de-l-etat"
        );
        assert_eq!(slugify("Œuvres d’art à Noël"), "oeuvres-d-art-a-noel");
        assert_eq!(slugify("ÇA, Ù, Ÿ, æ, ß"), "ca-u-y-ae-ss");
        assert_eq!(
            slugify("Économiser 10 % sur l'énergie des écoles (2025)"),
            "economiser-10-sur-l-energie-des-ecoles-2025"
        );
    }

    #[test]
    fn collapses_separators_and_trims() {
        assert_eq!(slugify("  --Hello   World!!  "), "hello-world");
        assert_eq!(slugify("a\tb\nc"), "a-b-c");
        assert_eq!(slugify("TVA→taux réduit"), "tva-taux-reduit");
    }

    #[test]
    fn falls_back_when_nothing_is_left() {
        assert_eq!(slugify(""), FALLBACK_SLUG);
        assert_eq!(slugify("!!! ???"), FALLBACK_SLUG);
        assert_eq!(slugify("日本語"), FALLBACK_SLUG);
    }

    #[test]
    fn truncates_at_a_word_boundary() {
        let title = "Mutualiser les achats de fournitures entre les communes d'une même intercommunalité pour obtenir de meilleurs prix";
        let slug = slugify(title);
        assert!(slug.len() <= MAX_SLUG_LEN, "{slug}");
        assert!(!slug.ends_with('-'));
        assert_eq!(
            slug,
            "mutualiser-les-achats-de-fournitures-entre-les-communes-d-une-meme"
        );
        assert!(is_valid_slug(&slug));
    }

    #[test]
    fn truncates_a_single_long_word() {
        let slug = slugify(&"a".repeat(200));
        assert_eq!(slug.len(), MAX_SLUG_LEN);
    }

    #[test]
    fn exact_word_end_at_the_limit_is_kept() {
        let word = "b".repeat(MAX_SLUG_LEN - 2);
        let title = format!("{word} c d");
        // The cut falls right before a dash: the whole first word is kept.
        assert_eq!(slugify(&title), format!("{word}-c"));
    }

    #[test]
    fn suffix_keeps_the_limit() {
        assert_eq!(with_suffix("reduire-les-couts", 2), "reduire-les-couts-2");
        let long = slugify(&"mot ".repeat(40));
        let suffixed = with_suffix(&long, 12);
        assert!(suffixed.len() <= MAX_SLUG_LEN);
        assert!(suffixed.ends_with("-12"));
        assert!(is_valid_slug(&suffixed));
        assert_eq!(with_suffix("", 3), "idee-3");
    }

    #[test]
    fn slug_validation() {
        assert!(is_valid_slug("reduire-le-cout-2"));
        assert!(!is_valid_slug(""));
        assert!(!is_valid_slug("-a"));
        assert!(!is_valid_slug("a-"));
        assert!(!is_valid_slug("a--b"));
        assert!(!is_valid_slug("Majuscule"));
        assert!(!is_valid_slug("é"));
        assert!(!is_valid_slug(&"a".repeat(MAX_SLUG_LEN + 1)));
    }

    #[test]
    fn slugs_are_always_valid() {
        for title in [
            "L'État",
            "  x  ",
            "€€€ 1 000 000 € !",
            "Rénovation énergétique : 5 Md€ d’économies ?",
        ] {
            assert!(is_valid_slug(&slugify(title)), "{title}");
        }
    }
}
