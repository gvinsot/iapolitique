//! Deterministic masking of personal data in visitor texts (AGENTS.md §7.5).
//!
//! Applied **before** any call to Mistral and before publication: e-mail addresses, French
//! phone numbers, IBANs, social security numbers (NIR), postal addresses and URLs that point to
//! a person are replaced by [`MASK`]. The published « original » text is the masked one.
//!
//! Every pattern is matched on the original text, overlapping matches are merged, then each
//! span is replaced once: the result does not depend on the order of the patterns, and masking a
//! masked text changes nothing. The patterns favour precision on the figures that ideas are made
//! of (amounts like `12 000 €`, years, article numbers `L. 123-4`, percentages, INSEE codes):
//! the `personal_data` screening criterion (§5.3) catches what a pattern cannot (names…).

use std::sync::LazyLock;

use regex::Regex;

/// Replacement of every masked span.
pub const MASK: &str = "[donnée masquée]";

/// A masked text and the number of masked spans.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Redacted {
    pub text: String,
    pub count: usize,
}

impl Redacted {
    pub fn is_masked(&self) -> bool {
        self.count > 0
    }
}

/// What a span was masked for (tests and diagnostics; never stored with the text).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum PiiKind {
    Url,
    Email,
    Iban,
    Nir,
    Phone,
    Address,
}

fn re(pattern: &str) -> Regex {
    Regex::new(pattern).expect("valid redaction pattern")
}

/// `http(s)://…`, `www.…` or `host.tld/…` up to the next whitespace or closing bracket.
static URL_RE: LazyLock<Regex> = LazyLock::new(|| {
    re(r#"(?i)\b(?:https?://|www\.|(?:[a-z0-9-]+\.)+[a-z]{2,}/)[^\s<>"'«»()\[\]{}]+"#)
});
/// Hosts whose URLs designate a person (profiles of social networks…).
static PERSONAL_HOST_RE: LazyLock<Regex> = LazyLock::new(|| {
    re(
        r"(?i)^(?:[a-z0-9-]+\.)*(?:facebook\.com|fb\.com|fb\.me|m\.me|instagram\.com|twitter\.com|x\.com|linkedin\.com|lnkd\.in|tiktok\.com|snapchat\.com|threads\.net|bsky\.app|pinterest\.[a-z.]+|github\.com|gitlab\.com|medium\.com|t\.me|wa\.me|vk\.com|copainsdavant\.linternaute\.com|onlyfans\.com|patreon\.com|twitch\.tv|about\.me|linktr\.ee)$",
    )
});
/// Paths and query parameters that identify a person on any host.
static PERSONAL_PATH_RE: LazyLock<Regex> = LazyLock::new(|| {
    re(
        r"(?i)(?:/@[^/?#]+|/(?:users?|u|profiles?|profil|people|membres?|members?|channel)/[^/?#]+|[?&](?:e?mail|user(?:name|_?id)?|uid|login|pseudo|nom|name|prenom|first_?name|last_?name|tel|telephone|phone|mobile|account)=)",
    )
});
/// Plain or `mailto:` e-mail address.
static EMAIL_RE: LazyLock<Regex> = LazyLock::new(|| {
    re(r"(?i)(?:mailto:)?[\p{L}\p{N}._%+\-]+@[\p{L}\p{N}\-]+(?:\.[\p{L}\p{N}\-]+)*\.[a-z]{2,}\b")
});
/// Obfuscated e-mail address: `jean.dupont [at] exemple [point] fr`, `jean arobase exemple
/// point com`, `jean @ exemple.fr`, `jean＠exemple.fr`, `jean@exemple[.]com`. Written in words,
/// the domain must end with a common top-level domain, so that prose does not match.
static OBFUSCATED_EMAIL_RE: LazyLock<Regex> = LazyLock::new(|| {
    let at =
        r"(?:\s*(?:[\[(]\s*(?:at|arobase|@|＠|﹫)\s*[\])]|[@＠﹫])\s*|\s+(?:at|arobase|chez)\s+)";
    let dot = r"(?:\s*(?:\.|[\[(]\s*(?:\.|dot|point)\s*[\])])\s*|\s+(?:dot|point)\s+)";
    re(&format!(
        r"(?i)[\p{{L}}\p{{N}}._%+\-]+{at}[\p{{L}}\p{{N}}\-]+(?:{dot}[\p{{L}}\p{{N}}\-]+)*{dot}(?:com|fr|net|org|eu|be|ch|ca|info|io|me|biz|de|uk|es|it|re)\b"
    ))
});
/// IBAN candidate: country code, check digits, then groups of up to 4 characters.
/// Groups may be separated by a space, `-`, `.` or `/`. A separator inside the country code and
/// check digits (`FR 76 …`) is accepted in uppercase only: case-insensitive, ordinary French
/// words followed by figures would pass the checksum about once in 97.
static IBAN_RE: LazyLock<Regex> = LazyLock::new(|| {
    re(
        r"(?i:\b[a-z]{2}\d{2}(?:[ .\-/]?[a-z0-9]{1,4}){3,8}\b)|\b[A-Z]{2}[ .\-/]\d{2}(?:[ .\-/]?[A-Z0-9]{1,4}){3,8}\b",
    )
});
/// French RIB without its IBAN prefix: bank (5 digits), branch (5), account (11), key (2).
static RIB_RE: LazyLock<Regex> =
    LazyLock::new(|| re(r"(?i)\b\d{5}[ .\-]?\d{5}[ .\-]?[a-z0-9]{11}[ .\-]?\d{2}\b"));
/// NIR: sex, year, month, department (2A/2B for Corsica), commune, order, optional key.
static NIR_RE: LazyLock<Regex> = LazyLock::new(|| {
    re(
        r"(?i)\b[1-478][ .\-/]?\d{2}[ .\-/]?(?:0[1-9]|1[0-2]|[2-3]\d|4[0-2]|[5-9]\d)[ .\-/]?(?:\d{2}|2[ab])[ .\-/]?\d{3}[ .\-/]?\d{3}(?:[ .\-/]?\d{2})?\b",
    )
});
/// French phone number, national or international format (metropolitan and overseas codes),
/// with `+33 0…`, `(+33)` and `/` between pairs of digits (`06/12/34/56/78`: `/` is accepted
/// only there, so that dates stay visible).
static PHONE_RE: LazyLock<Regex> = LazyLock::new(|| {
    let codes = "(?:33|262|269|508|590|594|596|681|687|689)";
    re(&format!(
        r"(?:(?:\(\s*(?:\+|00)\s*{codes}\s*\)|(?:\+|00)[ .\-]?{codes})[ .\-]?(?:\(0\)[ .\-]?|0[ .\-]?)?[1-9](?:[ .\-]?\d){{8}}|\b0[1-9](?:[ .\-]?\d){{8}}|\b0[1-9](?:/\d\d){{4}})\b"
    ))
});
/// Street address: number, optional `bis`/`ter`, street type, then the street name, optionally
/// followed by a postal code and a town. The name continues with capitalised words only, so
/// the rest of the sentence is kept. Unambiguous street types (`rue`, `avenue`…) accept a
/// lowercase first word (`rue des tilleuls`); ambiguous ones (`place`, `cours`, `route`…) need
/// a capitalised name (`place de la Mairie`, not `1 place de parking`), as do abbreviations
/// (`12 av Foch`, `12 pl. de la Mairie`; `r`/`r.` are left out: `R. 4127-1` is an article).
/// Also `lieu-dit Les Granges` without a number and postal boxes (`BP 123 75002 Paris`).
static ADDRESS_RE: LazyLock<Regex> = LazyLock::new(|| {
    let number = r"\b\d{1,4}(?:\s?(?:bis|ter|quater|[a-d])\b)?\s*,?\s+";
    let particles = r"(?:(?:de\s+la|des|du|de|aux|au|les|la|le)\s+|(?:de\s+l|d|l)['’]\s*)?";
    let name_word = r"[\p{L}\p{N}][\p{L}\p{N}'’\-]*";
    let capital_word = r"(?:\p{Lu}|\d)[\p{L}\p{N}'’\-]*";
    let tail = format!(
        r"(?:\s+(?:{particles}{capital_word}|{capital_word}))*(?:\s*,?\s*\d{{5}}\s+\p{{Lu}}[\p{{L}}'’\-]*(?:[ \-]\p{{Lu}}[\p{{L}}'’\-]*)*)?"
    );
    re(&format!(
        r"(?:(?i:lieu-dit|lieudit)\s+{particles}{capital_word}{tail})|(?:\b(?i:BP|CS)\s?\d{{1,5}}\s*,?\s*\d{{5}}\s+\p{{Lu}}[\p{{L}}'’\-]*)|(?:{number}(?i:rue|avenue|av\.|bd|boulevard|bld|allées?|allees?|impasse|faubourg|fbg|ruelle|venelle|lotissement|chemin|sente)\s+{particles}{name_word}{tail})|(?:{number}(?i:av|bvd|blvd|ch\.?|rte|pl\.?|imp\.?|sq\.?|domaine|parc|place|cours|route|quai|square|passage|sentier|voie|résidence|residence|cité|cite|hameau|lieu-dit|esplanade|promenade|parvis|villa|traverse|montée|mail|rond-point|clos)\s+{particles}{capital_word}{tail})"
    ))
});

fn is_digit_at(text: &str, index: usize, before: bool) -> bool {
    if before {
        text[..index].chars().next_back()
    } else {
        text[index..].chars().next()
    }
    .is_some_and(|c| c.is_ascii_digit())
}

/// Whether the digits of a candidate number touch other digits (`1 234 06 12 34 56 78`…), or a
/// decimal separator followed by digits: it is then part of a longer figure. Glue is exactly one
/// separator between two digits; a separator next to a space (`1. 06…`, `…78. 3 euros`,
/// `…78 - 2 fois`, `…78, 2 fois`) is a boundary between the number and another figure.
fn is_isolated_number(text: &str, start: usize, end: usize) -> bool {
    let mut before = text[..start].chars().rev();
    let glued_before = matches!(before.next(), Some(' ' | '.' | '-' | ','))
        && before.next().is_some_and(|c| c.is_ascii_digit());
    let mut after = text[end..].chars();
    let glued_after = matches!(after.next(), Some(' ' | '.' | '-' | ','))
        && after.next().is_some_and(|c| c.is_ascii_digit());
    !is_digit_at(text, start, true)
        && !is_digit_at(text, end, false)
        && !glued_before
        && !glued_after
}

/// ISO 13616 checksum: the rearranged IBAN, letters as numbers, modulo 97 equals 1.
fn iban_checksum_ok(compact: &str) -> bool {
    if compact.len() < 15 || compact.len() > 34 {
        return false;
    }
    let (head, rest) = compact.split_at(4);
    let mut remainder: u32 = 0;
    for c in rest.chars().chain(head.chars()) {
        let value = match c.to_digit(36) {
            Some(v) => v,
            None => return false,
        };
        let digits = if value < 10 { 10 } else { 100 };
        remainder = (remainder * digits + value) % 97;
    }
    remainder == 1
}

fn is_iban(candidate: &str) -> bool {
    let compact: String = candidate
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_uppercase())
        .collect();
    if compact.len() < 15 || !compact[4..].chars().any(|c| c.is_ascii_digit()) {
        return false;
    }
    // French IBANs (FR, MC) have 27 characters: masked even with a typo in the key.
    iban_checksum_ok(&compact)
        || ((compact.starts_with("FR") || compact.starts_with("MC")) && compact.len() == 27)
}

/// Length of the longest prefix of an IBAN candidate that is an IBAN, cut at a word end: the
/// pattern may have swallowed the next word (`FR76 … 189 merci`).
fn iban_len(candidate: &str) -> Option<usize> {
    candidate
        .char_indices()
        .filter(|(_, c)| !c.is_ascii_alphanumeric())
        .map(|(i, _)| i)
        .chain([candidate.len()])
        .filter(|&end| is_iban(&candidate[..end]))
        .max()
}

/// French RIB key: `97 - (89 × bank + 15 × branch + 3 × account) mod 97`, the letters of the
/// account counting as digits (A, J → 1 … I, R, Z → 9).
fn is_rib(candidate: &str) -> bool {
    let digits: Vec<u64> = candidate
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|c| match c.to_ascii_uppercase() {
            d @ '0'..='9' => u64::from(d as u8 - b'0'),
            'A' | 'J' => 1,
            'B' | 'K' | 'S' => 2,
            'C' | 'L' | 'T' => 3,
            'D' | 'M' | 'U' => 4,
            'E' | 'N' | 'V' => 5,
            'F' | 'O' | 'W' => 6,
            'G' | 'P' | 'X' => 7,
            'H' | 'Q' | 'Y' => 8,
            _ => 9,
        })
        .collect();
    if digits.len() != 23 {
        return false;
    }
    let number = |range: std::ops::Range<usize>| digits[range].iter().fold(0, |n, d| n * 10 + d);
    let (bank, branch, account, key) =
        (number(0..5), number(5..10), number(10..21), number(21..23));
    97 - (89 * bank + 15 * branch + 3 * account) % 97 == key
}

/// `52 av. J.-C.` is a date, not an address.
fn is_before_christ(text: &str, end: usize) -> bool {
    text[..end].ends_with(['J', 'j'])
        && text[end..]
            .trim_start_matches('.')
            .trim_start()
            .starts_with(['-', '‑'])
}

fn is_personal_url(url: &str) -> bool {
    let without_scheme = url
        .split_once("://")
        .map_or(url, |(_, rest)| rest)
        .trim_end_matches(['.', ',', ';', ':', '!', '?']);
    let (host, path) = match without_scheme.find(['/', '?', '#']) {
        Some(i) => (&without_scheme[..i], &without_scheme[i..]),
        None => (without_scheme, ""),
    };
    let host = host.rsplit('@').next().unwrap_or(host);
    let host = host.split(':').next().unwrap_or(host);
    let profile_path = !path.trim_matches(['/', '?', '#']).is_empty();
    let (lower_host, lower_path) = (host.to_ascii_lowercase(), path.to_ascii_lowercase());
    // Hosts that also serve impersonal pages: only their person paths.
    let host_is = |domain: &str| {
        lower_host == domain
            || lower_host
                .strip_suffix(domain)
                .is_some_and(|sub| sub.ends_with('.'))
    };
    if (host_is("youtube.com") && lower_path.starts_with("/c/"))
        || (host_is("societe.com") && lower_path.starts_with("/dirigeant/"))
        || (host_is("doctolib.fr") && lower_path.trim_end_matches('/').matches('/').count() >= 3)
    {
        return true;
    }
    (PERSONAL_HOST_RE.is_match(host) && profile_path)
        || PERSONAL_PATH_RE.is_match(path)
        || without_scheme.contains('@')
}

/// Spans of personal data, possibly overlapping, with their kind.
pub fn find(text: &str) -> Vec<(usize, usize, PiiKind)> {
    let mut spans = Vec::new();
    for m in URL_RE.find_iter(text) {
        // Trailing punctuation belongs to the sentence, not to the URL.
        let url = m.as_str().trim_end_matches(['.', ',', ';', ':', '!', '?']);
        if is_personal_url(url) {
            spans.push((m.start(), m.start() + url.len(), PiiKind::Url));
        }
    }
    let plain: Vec<(usize, usize)> = EMAIL_RE
        .find_iter(text)
        .map(|m| (m.start(), m.end()))
        .collect();
    spans.extend(
        plain
            .iter()
            .map(|&(start, end)| (start, end, PiiKind::Email)),
    );
    // An obfuscated match inside a plain address is the same address.
    spans.extend(
        OBFUSCATED_EMAIL_RE
            .find_iter(text)
            .filter(|m| {
                !plain
                    .iter()
                    .any(|&(start, end)| start <= m.start() && m.end() <= end)
            })
            .map(|m| (m.start(), m.end(), PiiKind::Email)),
    );
    spans.extend(IBAN_RE.find_iter(text).filter_map(|m| {
        iban_len(m.as_str()).map(|len| (m.start(), m.start() + len, PiiKind::Iban))
    }));
    spans.extend(
        RIB_RE
            .find_iter(text)
            .filter(|m| is_rib(m.as_str()))
            .map(|m| (m.start(), m.end(), PiiKind::Iban)),
    );
    spans.extend(
        NIR_RE
            .find_iter(text)
            .filter(|m| is_isolated_number(text, m.start(), m.end()))
            .map(|m| (m.start(), m.end(), PiiKind::Nir)),
    );
    spans.extend(
        PHONE_RE
            .find_iter(text)
            .filter(|m| is_isolated_number(text, m.start(), m.end()))
            .map(|m| (m.start(), m.end(), PiiKind::Phone)),
    );
    spans.extend(
        ADDRESS_RE
            .find_iter(text)
            .filter(|m| !is_before_christ(text, m.end()))
            .map(|m| (m.start(), m.end(), PiiKind::Address)),
    );
    spans.sort_unstable();
    spans
}

/// Masks every personal data span of `text` with [`MASK`].
pub fn redact(text: &str) -> Redacted {
    let mut merged: Vec<(usize, usize)> = Vec::new();
    for (start, end, _) in find(text) {
        match merged.last_mut() {
            Some(last) if start <= last.1 => last.1 = last.1.max(end),
            _ => merged.push((start, end)),
        }
    }
    let mut out = String::with_capacity(text.len());
    let mut cursor = 0;
    for (start, end) in &merged {
        out.push_str(&text[cursor..*start]);
        out.push_str(MASK);
        cursor = *end;
    }
    out.push_str(&text[cursor..]);
    Redacted {
        text: out,
        count: merged.len(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Masked texts, `[]` standing for [`MASK`].
    fn check(cases: &[(&str, &str)]) {
        for (input, expected) in cases {
            let expected = expected.replace("[]", MASK);
            let redacted = redact(input);
            assert_eq!(redacted.text, expected, "input: {input:?}");
            assert_eq!(
                redacted.count,
                expected.matches(MASK).count(),
                "input: {input:?}"
            );
            // Idempotent.
            assert_eq!(
                redact(&redacted.text).text,
                redacted.text,
                "input: {input:?}"
            );
        }
    }

    fn untouched(cases: &[&str]) {
        for input in cases {
            let redacted = redact(input);
            assert_eq!(redacted.text, *input, "false positive: {:?}", find(input));
            assert!(!redacted.is_masked());
        }
    }

    #[test]
    fn emails() {
        check(&[
            ("Écrivez à jean.dupont@example.fr.", "Écrivez à []."),
            (
                "Contact : Prénom.Nom+tag@sous.domaine.gouv.fr ou rien",
                "Contact : [] ou rien",
            ),
            ("mailto:marie@exemple.com", "[]"),
            ("élodie.martin@laposte.net", "[]"),
            ("jean.dupont [at] exemple [point] fr", "[]"),
            ("jean(at)exemple(dot)com merci", "[] merci"),
            ("deux : a@b.fr, c@d.org", "deux : [], []"),
        ]);
    }

    #[test]
    fn phone_numbers() {
        check(&[
            ("Appelez le 06 12 34 56 78.", "Appelez le []."),
            ("Tél. 0612345678", "Tél. []"),
            ("Tél : 06.12.34.56.78", "Tél : []"),
            ("01-23-45-67-89 (bureau)", "[] (bureau)"),
            ("+33 6 12 34 56 78", "[]"),
            ("+33612345678", "[]"),
            ("+33 (0)1 23 45 67 89", "[]"),
            ("0033 6 12 34 56 78", "[]"),
            ("+262 692 12 34 56 à La Réunion", "[] à La Réunion"),
            ("0692 12 34 56", "[]"),
            ("06 12 34 56 78, 2 fois", "[], 2 fois"),
            // PRIV-1: a figure in the next sentence or a list number is not glue.
            (
                "Appelez le 06 12 34 56 78. 3 euros suffisent",
                "Appelez le []. 3 euros suffisent",
            ),
            ("Contacts : 1. 06 12 34 56 78", "Contacts : 1. []"),
            ("06 12 34 56 78 - 2 fois", "[] - 2 fois"),
            // PRIV-1: common international and slashed forms.
            ("+33 06 12 34 56 78", "[]"),
            ("+33 0 6 12 34 56 78", "[]"),
            ("(+33) 6 12 34 56 78", "[]"),
            ("(0033) 6 12 34 56 78", "[]"),
            ("06/12/34/56/78", "[]"),
        ]);
    }

    /// PRIV-1: e-mail obfuscations written in words or with spaces around `@`.
    #[test]
    fn obfuscated_emails() {
        check(&[
            ("jean.dupont arobase gmail point com", "[]"),
            ("jean.dupont @ gmail.com", "[]"),
            ("jean.dupont at gmail dot com", "[]"),
            ("jean.dupont@gmail[.]com", "[]"),
            ("jean.dupont＠gmail.com", "[]"),
            ("jean.dupont (arobase) gmail (point) fr", "[]"),
            (
                "appelez-moi au +33 06 12 34 56 78 ou écrivez à jean.dupont arobase gmail point com.",
                "appelez-moi au [] ou écrivez à [].",
            ),
        ]);
    }

    #[test]
    fn ibans() {
        check(&[
            (
                "IBAN FR76 3000 6000 0112 3456 7890 189 merci",
                "IBAN [] merci",
            ),
            ("FR7630006000011234567890189", "[]"),
            ("fr76 3000 6000 0112 3456 7890 189", "[]"),
            // Typo in the French key: still masked.
            ("FR77 3000 6000 0112 3456 7890 189", "[]"),
            ("DE89 3704 0044 0532 0130 00", "[]"),
            ("BE68539007547034", "[]"),
            // PRIV-2: separators inside the country code, dots and slashes, bare RIB.
            ("FR 76 3000 6000 0112 3456 7890 189", "[]"),
            ("FR76.3000.6000.0112.3456.7890.189", "[]"),
            ("IBAN : FR76/3000/6000/0112/3456/7890/189", "IBAN : []"),
            ("RIB 30006 00001 12345678901 89", "RIB []"),
        ]);
    }

    #[test]
    fn social_security_numbers() {
        check(&[
            ("Mon NIR : 1 85 05 78 006 084 36.", "Mon NIR : []."),
            ("185057800608436", "[]"),
            ("2 95 12 2A 123 456 78", "[]"),
            ("2.95.12.2B.123.456", "[]"),
            ("numéro 1850578006084", "numéro []"),
            // PRIV-2: slashes.
            ("1/85/05/78/006/084/36", "[]"),
        ]);
    }

    #[test]
    fn postal_addresses() {
        check(&[
            ("J'habite 12 rue de la Paix, 75002 Paris.", "J'habite []."),
            ("au 3 bis avenue Victor Hugo", "au []"),
            ("au 3, boulevard Saint-Michel à Paris", "au [] à Paris"),
            ("15 allée des tilleuls", "[]"),
            ("7 impasse du Moulin 33000 Bordeaux", "[]"),
            ("2 place de la Mairie", "[]"),
            ("40 route de Lyon", "[]"),
            ("10 rue du Général Leclerc 92130 Issy-les-Moulineaux", "[]"),
            // The rest of the sentence is kept.
            (
                "Le 12 rue de la Paix est trop cher.",
                "Le [] est trop cher.",
            ),
            ("au 8 rue d'Alsace près de la gare", "au [] près de la gare"),
            // PRIV-2: abbreviations and other street types.
            ("J'habite au 12 av Foch à Lyon", "J'habite au [] à Lyon"),
            ("12 chemin des vignes", "[]"),
            ("12 bvd Voltaire", "[]"),
            ("12 rte de Lyon", "[]"),
            ("12 pl. de la Mairie", "[]"),
            ("15 allées Jean Jaurès", "[]"),
            ("3 sente des Prés", "[]"),
            ("Lieu-dit Les Granges 24000 Périgueux", "[]"),
            ("BP 123 75002 Paris", "[]"),
        ]);
    }

    #[test]
    fn personal_urls() {
        check(&[
            (
                "Voir https://www.facebook.com/jean.dupont.75 svp",
                "Voir [] svp",
            ),
            ("https://fr.linkedin.com/in/marie-martin-123/.", "[]."),
            ("www.instagram.com/prenom_nom", "[]"),
            ("https://x.com/pseudo", "[]"),
            ("https://exemple.fr/annuaire?nom=dupont&prenom=jean", "[]"),
            ("https://mastodon.social/@quelquun", "[]"),
            ("https://forum.exemple.fr/users/jdupont", "[]"),
            ("https://exemple.fr/contact?email=a@b.fr", "[]"),
            // PRIV-2: without a scheme or `www.`, and host-specific profile paths.
            ("Mon profil : instagram.com/jdupont75", "Mon profil : []"),
            ("facebook.com/jean.dupont", "[]"),
            ("https://youtube.com/c/jdupont", "[]"),
            ("https://www.societe.com/dirigeant/jean-dupont-123", "[]"),
            (
                "https://www.doctolib.fr/medecin-generaliste/lyon/jean-dupont",
                "[]",
            ),
        ]);
    }

    #[test]
    fn mixed_text_counts_each_span_once() {
        let redacted = redact(
            "Je suis joignable au 06 12 34 56 78 ou à jean@exemple.fr, \
             au 4 rue des Lilas 69003 Lyon.",
        );
        assert_eq!(
            redacted.text,
            format!("Je suis joignable au {MASK} ou à {MASK}, au {MASK}.")
        );
        assert_eq!(redacted.count, 3);
        let kinds: Vec<PiiKind> = find("06 12 34 56 78 et a@b.fr")
            .iter()
            .map(|s| s.2)
            .collect();
        assert_eq!(kinds, [PiiKind::Phone, PiiKind::Email]);
    }

    #[test]
    fn figures_of_ideas_are_not_masked() {
        untouched(&[
            "Économiser 12 000 € par an et par commune.",
            "Un gain de 1 200 000 000 € (1,2 Md€) dès 2027.",
            "Un coût de 2 000 000 000 000 € de dette publique.",
            "Entre 2024 et 2025, la dépense a crû de 3,5 %.",
            "Modifier l'article L. 123-4 du code de la sécurité sociale.",
            "Article L123-4, R. 4127-1 et D. 162-1-7 du code.",
            "Le décret n° 2021-1234 du 12 octobre 2021.",
            "La loi n° 2023-270 du 14 avril 2023.",
            "Le taux passe de 20 % à 5,5 %.",
            "Appeler le 3949 ou le 15 en urgence.",
            "La commune 75056 (code INSEE) et le département 2A.",
            "SIREN 552 100 554 de l'opérateur.",
            "Le 12/03/2024, 1er janvier 2025.",
            "75001 Paris, 13001 Marseille.",
            "Réserver 1 place de parking par agent, 2 cours de soutien.",
            "Les 3 routes nationales et les 12 rues piétonnes.",
            "Environ 0,45 % du PIB, soit 12,5 milliards.",
            "Source : https://www.ccomptes.fr/fr/publications/la-situation-des-finances-publiques",
            "Voir https://www.insee.fr/fr/statistiques/1234567?sommaire=2024.",
            "Covid19 et le PLFSS 2025.",
            "Un total de 185 057 800 € répartis sur 6 084 communes.",
            "Les 1234567890123 euros fictifs.",
            "Le compte FR2025 de l'État.",
            "Augmenter de +33 % la taxe.",
            "Téléphone 06 12 34 56 78 90 12 (numéro trop long)",
            "Le 01/02/2024 15 personnes",
            "Les 12/03/2024 et 13/03/2024.",
            "Rendez-vous chez Paul point final.",
            "Il est at home.",
            "Taux de 2 @ 3 %",
            "Un gain de 1 234 06 12 34 56 € fictif.",
            "Le point com de la bulle internet.",
            "Voir insee.fr/fr/statistiques/1234567",
            "Lire legifrance.gouv.fr/loda/id/JORFTEXT000",
            "En 2024/2025/2026/2027 le budget",
            "Les 12 chemins de randonnée",
            "Le 12 r de la Loi",
            "2 pl de parking",
            "Le 12/03/2024 à 14 h.",
            "Né en 52 av. J.-C. déjà",
            // Wrong RIB key.
            "Le lot 30006 00001 12345678901 88 fictif",
            "https://www.doctolib.fr/centre-de-sante",
        ]);
    }

    #[test]
    fn empty_and_already_masked_texts() {
        assert_eq!(
            redact(""),
            Redacted {
                text: String::new(),
                count: 0
            }
        );
        let once = redact("06 12 34 56 78");
        assert_eq!(redact(&once.text).count, 0);
    }

    #[test]
    fn iban_checksum() {
        assert!(iban_checksum_ok("FR7630006000011234567890189"));
        assert!(!iban_checksum_ok("FR7730006000011234567890189"));
        assert!(!iban_checksum_ok("FR76"));
        assert!(!iban_checksum_ok("FR76300060000112345678901**"));
    }
}
