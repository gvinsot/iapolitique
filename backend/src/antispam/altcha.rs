//! ALTCHA proof of work (AGENTS.md §7.2), self-hosted: no third-party captcha.
//!
//! Implements the ALTCHA v1 protocol, the one of the `altcha` widget 2.x used by the frontend
//! (reference implementation: `altcha-lib/v1`,
//! <https://github.com/altcha-org/altcha-lib/blob/main/src/v1/index.ts>). No maintained Rust crate
//! with a significant user base exists (`altcha-lib-rs`, `altcha` have a few thousand
//! downloads), and the protocol fits in a few lines:
//!
//! - challenge: `salt` = 12 random bytes in hex, then `?expires=<unix seconds>&`; a secret
//!   `number` in `[0, maxnumber]`; `challenge = hex(SHA-256(salt + number))`;
//!   `signature = hex(HMAC-SHA-256(ALTCHA_HMAC_KEY, challenge))`;
//! - the widget finds `number` by brute force and submits `base64(JSON { algorithm, challenge,
//!   number, salt, signature, took })`;
//! - verification: algorithm, expiry read from the signed salt, hash, HMAC (constant time), and
//!   no replay: a solved challenge is accepted once, its signature being remembered in memory
//!   until it expires.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use hmac::{Hmac, KeyInit, Mac};
use rand::RngExt as _;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use utoipa::ToSchema;

use crate::config::Secret;
use crate::http::error::ApiError;

/// Only algorithm accepted.
pub const ALGORITHM: &str = "SHA-256";
/// Upper bound of the secret number. The widget tries numbers from 0 and the secret is uniform,
/// so it computes `MAX_NUMBER / 2` hashes on average: about 0.5 to 1 s with WebCrypto on a
/// mid-range phone, a few tenths of a second on a computer.
pub const MAX_NUMBER: u64 = 100_000;
/// Validity of a challenge: time to fill in the form after the widget fetched it.
pub const CHALLENGE_TTL: Duration = Duration::from_secs(20 * 60);
/// Remembered signatures (replay protection) beyond which verifications are refused until
/// some expire. Each entry takes about 50 bytes.
const MAX_REMEMBERED: usize = 500_000;
/// Largest accepted payload, base64 included.
const MAX_PAYLOAD_LEN: usize = 2_048;

type HmacSha256 = Hmac<Sha256>;

/// Challenge sent to the widget (`GET /api/altcha/challenge`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct AltchaChallenge {
    /// Always `SHA-256`.
    #[schema(example = "SHA-256")]
    pub algorithm: String,
    /// Hex SHA-256 of `salt` followed by the secret number.
    pub challenge: String,
    /// Upper bound of the secret number.
    pub maxnumber: u64,
    /// Random salt with its signed expiry: `<hex>?expires=<unix seconds>&`.
    pub salt: String,
    /// Hex HMAC-SHA-256 of `challenge`.
    pub signature: String,
}

/// Solution submitted by the widget, base64-encoded JSON.
#[derive(Debug, Clone, Deserialize)]
struct Payload {
    algorithm: String,
    challenge: String,
    number: u64,
    salt: String,
    signature: String,
}

/// Rejected solution. Messages are in French: they are shown to the visitor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum AltchaError {
    #[error("La vérification anti-robot est absente ou illisible. Veuillez réessayer.")]
    Malformed,
    #[error("La vérification anti-robot a expiré. Veuillez réessayer.")]
    Expired,
    #[error("La vérification anti-robot a échoué. Veuillez réessayer.")]
    Invalid,
    #[error("Cette vérification anti-robot a déjà été utilisée. Veuillez réessayer.")]
    Replayed,
    #[error("Service momentanément surchargé, veuillez réessayer dans quelques minutes.")]
    Overloaded,
}

impl AltchaError {
    pub const fn code(self) -> &'static str {
        match self {
            Self::Malformed => "altcha_missing",
            Self::Expired => "altcha_expired",
            Self::Invalid => "altcha_invalid",
            Self::Replayed => "altcha_replayed",
            Self::Overloaded => "altcha_overloaded",
        }
    }
}

impl From<AltchaError> for ApiError {
    fn from(err: AltchaError) -> Self {
        match err {
            AltchaError::Overloaded => {
                ApiError::service_unavailable(err.code(), err.to_string()).with_retry_after(60)
            }
            _ => ApiError::bad_request(err.code(), err.to_string()),
        }
    }
}

/// Issues and verifies challenges with `ALTCHA_HMAC_KEY`.
pub struct Altcha {
    key: Secret,
    max_number: u64,
    ttl: Duration,
    /// Signature (raw HMAC) of the accepted solutions → expiry (unix seconds).
    used: Mutex<HashMap<[u8; 32], u64>>,
}

impl std::fmt::Debug for Altcha {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Altcha")
            .field("max_number", &self.max_number)
            .field("ttl", &self.ttl)
            .finish_non_exhaustive()
    }
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

fn sha256_hex(input: &str) -> String {
    hex(&Sha256::digest(input.as_bytes()))
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes
        .iter()
        .fold(String::with_capacity(bytes.len() * 2), |mut s, b| {
            let _ = write!(s, "{b:02x}");
            s
        })
}

fn unhex_32(text: &str) -> Option<[u8; 32]> {
    if text.len() != 64 || !text.is_ascii() {
        return None;
    }
    let mut out = [0u8; 32];
    for (i, byte) in out.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&text[2 * i..2 * i + 2], 16).ok()?;
    }
    Some(out)
}

/// `expires` parameter of a salt `<hex>?expires=<unix>&…`.
fn salt_expiry(salt: &str) -> Option<u64> {
    let (_, params) = salt.split_once('?')?;
    params.split('&').find_map(|pair| {
        let (key, value) = pair.split_once('=')?;
        (key == "expires" || key == "expire")
            .then(|| value.parse().ok())
            .flatten()
    })
}

impl Altcha {
    pub fn new(key: Secret) -> Self {
        Self::with_params(key, MAX_NUMBER, CHALLENGE_TTL)
    }

    pub fn with_params(key: Secret, max_number: u64, ttl: Duration) -> Self {
        Self {
            key,
            max_number,
            ttl,
            used: Mutex::new(HashMap::new()),
        }
    }

    fn mac(&self) -> HmacSha256 {
        HmacSha256::new_from_slice(self.key.expose().as_bytes()).expect("HMAC accepts any key")
    }

    /// A new challenge, valid for [`CHALLENGE_TTL`].
    pub fn create_challenge(&self) -> AltchaChallenge {
        let mut salt = [0u8; 12];
        rand::fill(&mut salt[..]);
        let number = rand::rng().random_range(0..=self.max_number);
        let expires = unix_now() + self.ttl.as_secs();
        self.challenge_for(&hex(&salt), expires, number)
    }

    fn challenge_for(&self, salt_hex: &str, expires: u64, number: u64) -> AltchaChallenge {
        // The trailing `&` prevents parameter splicing (as in altcha-lib).
        let salt = format!("{salt_hex}?expires={expires}&");
        let challenge = sha256_hex(&format!("{salt}{number}"));
        let mut mac = self.mac();
        mac.update(challenge.as_bytes());
        AltchaChallenge {
            algorithm: ALGORITHM.to_owned(),
            signature: hex(&mac.finalize().into_bytes()),
            challenge,
            maxnumber: self.max_number,
            salt,
        }
    }

    /// Verifies the base64 payload of the widget and consumes it (a payload is accepted once).
    pub fn verify(&self, payload: &str) -> Result<(), AltchaError> {
        self.verify_at(payload, unix_now())
    }

    fn verify_at(&self, payload: &str, now: u64) -> Result<(), AltchaError> {
        let payload = payload.trim();
        if payload.is_empty() || payload.len() > MAX_PAYLOAD_LEN {
            return Err(AltchaError::Malformed);
        }
        let bytes = BASE64.decode(payload).map_err(|_| AltchaError::Malformed)?;
        let payload: Payload =
            serde_json::from_slice(&bytes).map_err(|_| AltchaError::Malformed)?;
        if payload.algorithm != ALGORITHM {
            return Err(AltchaError::Invalid);
        }
        // The expiry is part of the hashed and signed salt: checked once the HMAC matches.
        let expires = salt_expiry(&payload.salt).ok_or(AltchaError::Invalid)?;
        if sha256_hex(&format!("{}{}", payload.salt, payload.number)) != payload.challenge {
            return Err(AltchaError::Invalid);
        }
        let signature = unhex_32(&payload.signature).ok_or(AltchaError::Invalid)?;
        let mut mac = self.mac();
        mac.update(payload.challenge.as_bytes());
        mac.verify_slice(&signature)
            .map_err(|_| AltchaError::Invalid)?;
        if expires < now || expires > now + self.ttl.as_secs() + 60 {
            return Err(AltchaError::Expired);
        }
        self.consume(signature, expires, now)
    }

    /// Remembers an accepted signature until it expires; refuses a second use.
    fn consume(&self, signature: [u8; 32], expires: u64, now: u64) -> Result<(), AltchaError> {
        let mut used = self.used.lock().unwrap_or_else(|e| e.into_inner());
        if used.len() >= MAX_REMEMBERED / 2 {
            used.retain(|_, expiry| *expiry >= now);
        }
        if used.contains_key(&signature) {
            return Err(AltchaError::Replayed);
        }
        if used.len() >= MAX_REMEMBERED {
            tracing::warn!("ALTCHA replay set full: verifications refused until entries expire");
            return Err(AltchaError::Overloaded);
        }
        used.insert(signature, expires);
        Ok(())
    }

    #[cfg(test)]
    fn remembered(&self) -> usize {
        self.used.lock().unwrap().len()
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    const KEY: &str = "0123456789abcdef0123456789abcdef";

    fn altcha() -> Altcha {
        Altcha::with_params(Secret::new(KEY), 1_000, CHALLENGE_TTL)
    }

    /// Solves a challenge like the widget does.
    pub(crate) fn solve(challenge: &AltchaChallenge) -> String {
        let number = (0..=challenge.maxnumber)
            .find(|n| sha256_hex(&format!("{}{n}", challenge.salt)) == challenge.challenge)
            .expect("solvable challenge");
        let payload = serde_json::json!({
            "algorithm": challenge.algorithm,
            "challenge": challenge.challenge,
            "number": number,
            "salt": challenge.salt,
            "signature": challenge.signature,
            "took": 42,
        });
        BASE64.encode(payload.to_string())
    }

    #[test]
    fn matches_the_reference_implementation() {
        // Computed with altcha-lib 1.x (v1 protocol): createChallenge({ hmacKey: KEY,
        // salt: "a1b2c3", number: 12345, expires: new Date(1_900_000_000_000) }).
        let challenge =
            Altcha::new(Secret::new(KEY)).challenge_for("a1b2c3", 1_900_000_000, 12_345);
        assert_eq!(
            challenge,
            AltchaChallenge {
                algorithm: "SHA-256".into(),
                challenge: "ede21934a01645208a65f47ae70d329b5955f2541ca4dec9a925ae8cc7c408ea"
                    .into(),
                maxnumber: MAX_NUMBER,
                salt: "a1b2c3?expires=1900000000&".into(),
                signature: "48f23ac945acfecb0ebe3d29e09450d37f9be77ac3881bcec3eecdd78a99c27f"
                    .into(),
            }
        );
        // Payload built like the widget, accepted by altcha-lib's verifySolution.
        let payload = "eyJhbGdvcml0aG0iOiJTSEEtMjU2IiwiY2hhbGxlbmdlIjoiZWRlMjE5MzRhMDE2NDUyMDhhNjVmNDdhZTcwZDMyOWI1OTU1ZjI1NDFjYTRkZWM5YTkyNWFlOGNjN2M0MDhlYSIsIm51bWJlciI6MTIzNDUsInNhbHQiOiJhMWIyYzM/ZXhwaXJlcz0xOTAwMDAwMDAwJiIsInNpZ25hdHVyZSI6IjQ4ZjIzYWM5NDVhY2ZlY2IwZWJlM2QyOWUwOTQ1MGQzN2Y5YmU3N2FjMzg4MWJjZWMzZWVjZGQ3OGE5OWMyN2YiLCJ0b29rIjoxMH0=";
        let altcha = Altcha::new(Secret::new(KEY));
        altcha.verify_at(payload, 1_900_000_000 - 600).unwrap();
        assert_eq!(
            altcha.verify_at(payload, 1_900_000_000 - 600),
            Err(AltchaError::Replayed)
        );
        assert_eq!(
            sha256_hex("abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn solved_challenge_is_accepted_once() {
        let altcha = altcha();
        let challenge = altcha.create_challenge();
        assert!(challenge.salt.contains("?expires="));
        assert!(challenge.salt.ends_with('&'));
        let payload = solve(&challenge);
        altcha.verify(&payload).unwrap();
        assert_eq!(altcha.verify(&payload), Err(AltchaError::Replayed));
        assert_eq!(altcha.remembered(), 1);
    }

    #[test]
    fn forged_or_altered_payloads_are_rejected() {
        let altcha = altcha();
        let challenge = altcha.create_challenge();
        let valid: serde_json::Value =
            serde_json::from_slice(&BASE64.decode(solve(&challenge)).unwrap()).unwrap();
        let encode = |value: &serde_json::Value| BASE64.encode(value.to_string());

        let mut wrong_number = valid.clone();
        wrong_number["number"] = serde_json::json!(valid["number"].as_u64().unwrap() + 1);
        assert_eq!(
            altcha.verify(&encode(&wrong_number)),
            Err(AltchaError::Invalid)
        );

        // Another key signed it.
        let other = Altcha::with_params(
            Secret::new("another-key-another-key-another-k"),
            1_000,
            CHALLENGE_TTL,
        );
        let foreign = solve(&other.create_challenge());
        assert_eq!(altcha.verify(&foreign), Err(AltchaError::Invalid));

        // Extending the expiry changes the hash.
        let mut extended = valid.clone();
        extended["salt"] = serde_json::json!(
            valid["salt"]
                .as_str()
                .unwrap()
                .replace("expires=", "expires=9")
        );
        assert_eq!(altcha.verify(&encode(&extended)), Err(AltchaError::Invalid));

        let mut algorithm = valid.clone();
        algorithm["algorithm"] = serde_json::json!("SHA-1");
        assert_eq!(
            altcha.verify(&encode(&algorithm)),
            Err(AltchaError::Invalid)
        );

        let mut bad_signature = valid.clone();
        bad_signature["signature"] = serde_json::json!("zz");
        assert_eq!(
            altcha.verify(&encode(&bad_signature)),
            Err(AltchaError::Invalid)
        );

        for malformed in ["", "%%%", &BASE64.encode("{}"), &"A".repeat(4_000)] {
            assert_eq!(
                altcha.verify(malformed),
                Err(AltchaError::Malformed),
                "{malformed:.20}"
            );
        }
        // Still valid after the failed attempts.
        altcha.verify(&encode(&valid)).unwrap();
    }

    #[test]
    fn expired_challenges_are_rejected() {
        let altcha = altcha();
        let now = unix_now();
        let challenge = altcha.challenge_for("00ff", now - 1, 7);
        assert_eq!(
            altcha.verify_at(&solve(&challenge), now),
            Err(AltchaError::Expired)
        );
        // An expiry too far in the future was not issued by this server's settings.
        let challenge = altcha.challenge_for("00ff", now + 365 * 86_400, 7);
        assert_eq!(
            altcha.verify_at(&solve(&challenge), now),
            Err(AltchaError::Expired)
        );
        let challenge = altcha.challenge_for("00ff", now + 60, 7);
        altcha.verify_at(&solve(&challenge), now).unwrap();
    }

    #[test]
    fn expired_signatures_are_forgotten() {
        let altcha = altcha();
        let now = unix_now();
        for i in 0..3u64 {
            let challenge = altcha.challenge_for(&format!("{i:02x}"), now + 10, i);
            altcha.verify_at(&solve(&challenge), now).unwrap();
        }
        assert_eq!(altcha.remembered(), 3);
        altcha
            .used
            .lock()
            .unwrap()
            .retain(|_, expiry| *expiry >= now + 11);
        assert_eq!(altcha.remembered(), 0);
    }

    #[test]
    fn salt_parameters() {
        assert_eq!(salt_expiry("ab?expires=12&"), Some(12));
        assert_eq!(salt_expiry("ab?x=1&expire=13"), Some(13));
        assert_eq!(salt_expiry("ab"), None);
        assert_eq!(salt_expiry("ab?expires=soon&"), None);
        assert_eq!(unhex_32(&"0".repeat(64)), Some([0; 32]));
        assert_eq!(unhex_32("00"), None);
        assert_eq!(unhex_32(&"g".repeat(64)), None);
    }

    #[test]
    fn errors_become_french_api_errors() {
        let err = ApiError::from(AltchaError::Expired);
        assert_eq!(err.status, axum::http::StatusCode::BAD_REQUEST);
        assert_eq!(err.code, "altcha_expired");
        assert!(err.message.contains("expiré"));
        let err = ApiError::from(AltchaError::Overloaded);
        assert_eq!(err.status, axum::http::StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(err.retry_after, Some(60));
        assert!(!format!("{:?}", altcha()).contains(KEY));
    }
}
