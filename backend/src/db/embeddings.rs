//! `idea_embeddings`: `mistral-embed` vectors of the ideas, for duplicate detection (AGENTS.md
//! §5.4). Vectors are exchanged with pgvector in its text form (`[0.1,0.2,…]` cast to
//! `vector`), so no extra crate is needed.

use sha2::{Digest, Sha256};
use sqlx::{PgExecutor, PgPool};
use uuid::Uuid;

use crate::ai::mistral::EMBEDDING_DIMENSIONS;

/// SHA-256 of an embedded text (`idea_embeddings.content_sha256`).
pub fn content_hash(text: &str) -> Vec<u8> {
    Sha256::digest(text.as_bytes()).to_vec()
}

/// pgvector text form of a vector. Non finite values are refused by the caller (Mistral
/// returns finite floats; `parse_embeddings` checks the dimensions).
pub fn to_pgvector(vector: &[f32]) -> String {
    let mut text = String::with_capacity(vector.len() * 12 + 2);
    text.push('[');
    for (i, x) in vector.iter().enumerate() {
        if i > 0 {
            text.push(',');
        }
        text.push_str(&x.to_string());
    }
    text.push(']');
    text
}

/// Parses the pgvector text form.
fn from_pgvector(text: &str) -> Option<Vec<f32>> {
    let inner = text.trim().strip_prefix('[')?.strip_suffix(']')?;
    inner
        .split(',')
        .map(|x| x.trim().parse::<f32>().ok())
        .collect()
}

/// The stored embedding of the idea when it was computed from the text of hash `hash` with
/// `model` (no need to ask Mistral again).
pub async fn current<'e>(
    executor: impl PgExecutor<'e>,
    idea_id: Uuid,
    model: &str,
    hash: &[u8],
) -> sqlx::Result<Option<Vec<f32>>> {
    let text: Option<String> = sqlx::query_scalar(
        "SELECT embedding::text FROM idea_embeddings
         WHERE idea_id = $1 AND model = $2 AND content_sha256 = $3",
    )
    .bind(idea_id)
    .bind(model)
    .bind(hash)
    .fetch_optional(executor)
    .await?;
    Ok(text.as_deref().and_then(from_pgvector))
}

/// Creates or replaces the embedding of an idea.
pub async fn upsert<'e>(
    executor: impl PgExecutor<'e>,
    idea_id: Uuid,
    model: &str,
    vector: &[f32],
    hash: &[u8],
) -> sqlx::Result<()> {
    debug_assert_eq!(vector.len(), EMBEDDING_DIMENSIONS);
    sqlx::query(
        "INSERT INTO idea_embeddings (idea_id, model, embedding, content_sha256)
         VALUES ($1, $2, $3::vector, $4)
         ON CONFLICT (idea_id) DO UPDATE SET model = EXCLUDED.model,
             embedding = EXCLUDED.embedding, content_sha256 = EXCLUDED.content_sha256,
             created_at = now()",
    )
    .bind(idea_id)
    .bind(model)
    .bind(to_pgvector(vector))
    .bind(hash)
    .execute(executor)
    .await
    .map(|_| ())
}

/// Nearest published idea of `vector` (embedding of `idea_id` by `model`) by cosine distance,
/// with the cosine similarity; `None` when no published idea has an embedding of this model.
///
/// The HNSW index is used with an iterative scan (pgvector ≥ 0.8): the nearest vectors may
/// belong to ideas that are not published (merged duplicates…), the scan goes on until enough
/// published ones are found. The relaxed order of such a scan is restored by sorting again.
pub async fn nearest_published(
    pool: &PgPool,
    idea_id: Uuid,
    model: &str,
    vector: &[f32],
) -> sqlx::Result<Option<(Uuid, f64)>> {
    let mut tx = pool.begin().await?;
    sqlx::query("SET LOCAL hnsw.iterative_scan = relaxed_order")
        .execute(&mut *tx)
        .await?;
    let nearest = sqlx::query_as(
        "WITH candidates AS MATERIALIZED (
             SELECT e.idea_id, e.embedding <=> $2::vector AS distance
             FROM idea_embeddings e
             JOIN ideas i ON i.id = e.idea_id AND i.status = 'published'
             WHERE e.idea_id <> $1 AND e.model = $3
             ORDER BY e.embedding <=> $2::vector
             LIMIT 5
         )
         SELECT idea_id, 1 - distance FROM candidates ORDER BY distance, idea_id LIMIT 1",
    )
    .bind(idea_id)
    .bind(to_pgvector(vector))
    .bind(model)
    .fetch_optional(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(nearest)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::unit_vector as unit;

    #[test]
    fn text_form() {
        assert_eq!(to_pgvector(&[0.5, -1.0, 0.0]), "[0.5,-1,0]");
        assert_eq!(from_pgvector("[0.5,-1,0]"), Some(vec![0.5, -1.0, 0.0]));
        assert_eq!(from_pgvector("0.5"), None);
        assert_eq!(from_pgvector("[a]"), None);
        assert_eq!(content_hash("a").len(), 32);
    }

    async fn idea(pool: &PgPool, status: &str) -> Uuid {
        crate::test_support::idea_with_status(pool, status).await
    }

    #[sqlx::test(migrator = "crate::db::MIGRATOR")]
    async fn stores_and_finds_the_nearest_published_idea(pool: PgPool) {
        let new = idea(&pool, "dedup").await;
        let near = crate::test_support::published_idea(&pool, "proche").await;
        let far = crate::test_support::published_idea(&pool, "lointaine").await;
        let unpublished = idea(&pool, "queued_analysis").await;
        let hash = content_hash("texte");
        upsert(&pool, new, "mistral-embed", &unit(0.0), &hash)
            .await
            .unwrap();
        assert!(
            nearest_published(&pool, new, "mistral-embed", &unit(0.0))
                .await
                .unwrap()
                .is_none()
        );
        upsert(&pool, far, "mistral-embed", &unit(1.2), &hash)
            .await
            .unwrap();
        upsert(&pool, near, "mistral-embed", &unit(0.1), &hash)
            .await
            .unwrap();
        upsert(&pool, unpublished, "mistral-embed", &unit(0.0), &hash)
            .await
            .unwrap();
        let (id, similarity) = nearest_published(&pool, new, "mistral-embed", &unit(0.0))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(id, near);
        assert!((similarity - 0.1f64.cos()).abs() < 1e-5, "{similarity}");
        // Another model is not comparable.
        assert!(
            nearest_published(&pool, new, "other-model", &unit(0.0))
                .await
                .unwrap()
                .is_none()
        );

        let stored = current(&pool, new, "mistral-embed", &hash)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(stored, unit(0.0));
        assert_eq!(
            current(&pool, new, "mistral-embed", &content_hash("autre"))
                .await
                .unwrap(),
            None
        );
        // Replacing keeps one row.
        upsert(
            &pool,
            new,
            "mistral-embed",
            &unit(0.3),
            &content_hash("autre"),
        )
        .await
        .unwrap();
        let (count,): (i64,) = sqlx::query_as("SELECT count(*) FROM idea_embeddings")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 4);
    }
}
