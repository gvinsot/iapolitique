//! Anonymous tracking links (AGENTS.md §7.1).
//!
//! A tracking token is 256 random bits encoded in unpadded base64url (43 characters). It is
//! returned **once** to the author; only the SHA-256 of its text is stored. A lookup hashes the
//! submitted token and searches the hash through the primary key: the comparison happens on a
//! hash the caller cannot steer, so the response time reveals nothing about valid tokens.
//! Malformed and unknown tokens are indistinguishable for the caller ([`token_hash`] returns
//! `None`, the API answers the same 404).
//!
//! Tokens are never logged: [`TrackingToken`] has no `Display` and a redacted `Debug`.

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};
use sqlx::PgExecutor;
use uuid::Uuid;

use crate::domain::types::TargetKind;

/// Random bytes of a token.
pub const TOKEN_BYTES: usize = 32;
/// Length of the base64url text of a token.
pub const TOKEN_LEN: usize = 43;

/// SHA-256 of a token, as stored in `tracking_tokens.token_hash`.
pub type TokenHash = [u8; 32];

/// A freshly issued token, to return to the author once.
#[derive(Clone, PartialEq, Eq)]
pub struct TrackingToken(String);

impl TrackingToken {
    /// Draws a new random token.
    pub fn generate() -> Self {
        let mut bytes = [0u8; TOKEN_BYTES];
        rand::fill(&mut bytes[..]);
        Self(URL_SAFE_NO_PAD.encode(bytes))
    }

    /// The text given to the author (placed in the fragment of the tracking URL).
    pub fn expose(&self) -> &str {
        &self.0
    }

    /// Consumes the token to put it in the response body.
    pub fn into_inner(self) -> String {
        self.0
    }

    pub fn hash(&self) -> TokenHash {
        Sha256::digest(self.0.as_bytes()).into()
    }
}

impl std::fmt::Debug for TrackingToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("TrackingToken([REDACTED])")
    }
}

/// Hash of a token submitted by a visitor, `None` when it cannot be a token we issued.
pub fn token_hash(token: &str) -> Option<TokenHash> {
    let token = token.trim();
    if token.len() != TOKEN_LEN {
        return None;
    }
    // Decoding checks the alphabet and the unused trailing bits: one text per token.
    let bytes = URL_SAFE_NO_PAD.decode(token).ok()?;
    if bytes.len() != TOKEN_BYTES {
        return None;
    }
    Some(Sha256::digest(token.as_bytes()).into())
}

/// What a token gives access to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TokenTarget {
    pub kind: TargetKind,
    pub id: Uuid,
    pub created_at: DateTime<Utc>,
}

/// Stores the hash of a new token.
pub async fn insert<'e>(
    executor: impl PgExecutor<'e>,
    token: &TrackingToken,
    kind: TargetKind,
    target_id: Uuid,
) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO tracking_tokens (token_hash, target_kind, target_id) VALUES ($1, $2, $3)",
    )
    .bind(&token.hash()[..])
    .bind(kind.as_str())
    .bind(target_id)
    .execute(executor)
    .await
    .map(|_| ())
}

/// Target of a valid, unexpired token.
pub async fn find<'e>(
    executor: impl PgExecutor<'e>,
    hash: &TokenHash,
) -> sqlx::Result<Option<TokenTarget>> {
    let row: Option<(String, Uuid, DateTime<Utc>)> = sqlx::query_as(
        "SELECT target_kind, target_id, created_at FROM tracking_tokens
         WHERE token_hash = $1 AND (expires_at IS NULL OR expires_at > now())",
    )
    .bind(&hash[..])
    .fetch_optional(executor)
    .await?;
    row.map(|(kind, id, created_at)| {
        let kind = kind
            .parse()
            .map_err(|err: crate::domain::UnknownVariant| sqlx::Error::Decode(err.into()))?;
        Ok(TokenTarget {
            kind,
            id,
            created_at,
        })
    })
    .transpose()
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::PgPool;

    #[test]
    fn tokens_are_256_bit_base64url() {
        let token = TrackingToken::generate();
        assert_eq!(token.expose().len(), TOKEN_LEN);
        assert!(
            token
                .expose()
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        );
        assert_eq!(
            URL_SAFE_NO_PAD.decode(token.expose()).unwrap().len(),
            TOKEN_BYTES
        );
        assert_ne!(token, TrackingToken::generate());
        assert_eq!(token_hash(token.expose()), Some(token.hash()));
        assert_eq!(format!("{token:?}"), "TrackingToken([REDACTED])");
    }

    #[test]
    fn malformed_tokens_have_no_hash() {
        let token = TrackingToken::generate().into_inner();
        assert!(token_hash(&format!(" {token}\n")).is_some());
        for bad in [
            String::new(),
            token[..42].to_owned(),
            format!("{token}A"),
            format!("{}=", &token[..42]),
            format!("{}+", &token[..42]),
            "é".repeat(22),
            // Unused trailing bits set: not a canonical encoding.
            format!("{}B", "A".repeat(42)),
        ] {
            assert_eq!(token_hash(&bad), None, "{bad:?}");
        }
    }

    #[sqlx::test(migrator = "crate::db::MIGRATOR")]
    async fn only_the_hash_is_stored(pool: PgPool) {
        let token = TrackingToken::generate();
        let target = Uuid::new_v4();
        insert(&pool, &token, TargetKind::Contribution, target)
            .await
            .unwrap();
        let (stored,): (Vec<u8>,) = sqlx::query_as("SELECT token_hash FROM tracking_tokens")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(stored, token.hash());
        assert!(
            !stored
                .windows(8)
                .any(|w| token.expose().as_bytes().starts_with(w))
        );

        let found = find(&pool, &token_hash(token.expose()).unwrap())
            .await
            .unwrap()
            .unwrap();
        assert_eq!((found.kind, found.id), (TargetKind::Contribution, target));
        let other = TrackingToken::generate();
        assert_eq!(find(&pool, &other.hash()).await.unwrap(), None);

        sqlx::query("UPDATE tracking_tokens SET expires_at = now() - interval '1 second'")
            .execute(&pool)
            .await
            .unwrap();
        assert_eq!(find(&pool, &token.hash()).await.unwrap(), None);
    }
}
