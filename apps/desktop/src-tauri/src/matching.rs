//! Voice matching (#25 / M3.2-3.4).
//!
//! Дано: embedding кластера спикера + список `voice_samples` всех контактов
//! (с consent_voice='true' — C2 фильтр). Считаем cosine similarity max-per-contact,
//! возвращаем ранжированный список кандидатов.

use serde::{Deserialize, Serialize};
use sqlx::{Row, SqlitePool};

use crate::{embeddings, AppError};

/// Один candidate из matching. `score` — max cosine similarity по всем
/// семплам этого контакта (M3.2 паспорта).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MatchCandidate {
    pub contact_id: String,
    pub display_name: String,
    pub score: f32,
}

/// Все voice_samples одного контакта (для batch matching).
pub struct ContactSamples {
    pub contact_id: String,
    pub display_name: String,
    pub embeddings: Vec<Vec<f32>>,
}

/// C2 (#40) фильтр: возвращает только контакты с attributes.consent_voice='true'.
/// owner-контакт пропускается (M3.7 — owner auto-bound через mic-track, не matching).
pub async fn list_consenting_samples(pool: &SqlitePool) -> Result<Vec<ContactSamples>, AppError> {
    let rows = sqlx::query(
        "SELECT c.id, c.display_name, c.attributes, vs.embedding
         FROM contacts c
         JOIN voice_samples vs ON vs.contact_id = c.id
         WHERE c.is_owner = 0",
    )
    .fetch_all(pool)
    .await?;

    let mut by_contact: std::collections::HashMap<String, ContactSamples> =
        std::collections::HashMap::new();
    for row in rows {
        let contact_id: String = row.try_get("id")?;
        let display_name: String = row.try_get("display_name")?;
        let attrs_json: Option<String> = row.try_get("attributes")?;
        let consent_voice = attrs_json
            .as_deref()
            .and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok())
            .and_then(|v| v.get("consent_voice").cloned())
            .map(|v| matches!(v, serde_json::Value::String(ref s) if s == "true"))
            .unwrap_or(false);
        if !consent_voice {
            continue;
        }
        let embedding_blob: Vec<u8> = row.try_get("embedding")?;
        let embedding = embeddings::bytes_to_embedding(&embedding_blob)?;
        by_contact
            .entry(contact_id.clone())
            .or_insert(ContactSamples {
                contact_id,
                display_name,
                embeddings: Vec::new(),
            })
            .embeddings
            .push(embedding);
    }
    Ok(by_contact.into_values().collect())
}

/// Top-N кандидатов по cosine similarity. `min_score` — порог отсечения
/// (default 0.5 у вызывающего; за 0.7 уже уверенный match).
pub fn rank_candidates(
    embedding: &[f32],
    contacts: &[ContactSamples],
    min_score: f32,
    top_n: usize,
) -> Vec<MatchCandidate> {
    let mut candidates: Vec<MatchCandidate> = contacts
        .iter()
        .filter_map(|c| {
            let best = c
                .embeddings
                .iter()
                .map(|e| embeddings::cosine_similarity(embedding, e))
                .fold(f32::NEG_INFINITY, f32::max);
            if best.is_finite() && best >= min_score {
                Some(MatchCandidate {
                    contact_id: c.contact_id.clone(),
                    display_name: c.display_name.clone(),
                    score: best,
                })
            } else {
                None
            }
        })
        .collect();
    candidates.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    candidates.truncate(top_n);
    candidates
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::test_support::fresh_db;
    use serde_json::json;

    async fn insert_contact_with_samples(
        pool: &SqlitePool,
        contact_id: &str,
        name: &str,
        consent: bool,
        embeddings_list: &[&[f32]],
    ) {
        let attrs = if consent {
            json!({"consent_voice": "true"}).to_string()
        } else {
            "{}".to_string()
        };
        let now = "2026-05-20T00:00:00Z";
        sqlx::query(
            "INSERT INTO contacts (id, display_name, is_owner, attributes, created_at, updated_at)
             VALUES (?1, ?2, 0, ?3, ?4, ?4)",
        )
        .bind(contact_id)
        .bind(name)
        .bind(&attrs)
        .bind(now)
        .execute(pool)
        .await
        .unwrap();
        for (i, emb) in embeddings_list.iter().enumerate() {
            let blob = embeddings::embedding_to_bytes(emb);
            sqlx::query(
                "INSERT INTO voice_samples (id, contact_id, embedding, quality, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
            )
            .bind(format!("vs-{contact_id}-{i}"))
            .bind(contact_id)
            .bind(blob)
            .bind(0.9_f32)
            .bind(now)
            .execute(pool)
            .await
            .unwrap();
        }
    }

    #[tokio::test]
    async fn list_consenting_samples_filters_by_consent_flag() {
        let db = fresh_db().await;
        insert_contact_with_samples(&db.pool, "c1", "Alice", true, &[&[1.0, 0.0, 0.0]]).await;
        insert_contact_with_samples(&db.pool, "c2", "Bob", false, &[&[0.0, 1.0, 0.0]]).await;

        let result = list_consenting_samples(&db.pool).await.unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].contact_id, "c1");
    }

    #[tokio::test]
    async fn list_consenting_samples_skips_owner() {
        let db = fresh_db().await;
        crate::db::ensure_owner_contact(&db.pool).await.unwrap();
        // У owner consent сейчас нет (не задан в attributes).
        let result = list_consenting_samples(&db.pool).await.unwrap();
        assert!(result.is_empty());
    }

    #[tokio::test]
    async fn list_consenting_samples_groups_multiple_per_contact() {
        let db = fresh_db().await;
        insert_contact_with_samples(
            &db.pool,
            "c1",
            "Alice",
            true,
            &[&[1.0, 0.0], &[0.9, 0.1], &[0.8, 0.2]],
        )
        .await;
        let result = list_consenting_samples(&db.pool).await.unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].embeddings.len(), 3);
    }

    #[test]
    fn rank_candidates_returns_best_per_contact() {
        let contacts = vec![
            ContactSamples {
                contact_id: "c1".into(),
                display_name: "Alice".into(),
                // first sample слабо похож, second — почти идентичен query
                embeddings: vec![vec![1.0, 0.0, 0.0], vec![0.9, 0.1, 0.0]],
            },
            ContactSamples {
                contact_id: "c2".into(),
                display_name: "Bob".into(),
                embeddings: vec![vec![0.0, 1.0, 0.0]],
            },
        ];
        let query = vec![0.9, 0.1, 0.0]; // совпадает с Alice second
        let ranked = rank_candidates(&query, &contacts, 0.5, 5);
        assert!(!ranked.is_empty());
        assert_eq!(ranked[0].contact_id, "c1");
        assert!(ranked[0].score > 0.95);
    }

    #[test]
    fn rank_candidates_drops_below_min_score() {
        let contacts = vec![ContactSamples {
            contact_id: "c1".into(),
            display_name: "Alice".into(),
            embeddings: vec![vec![1.0, 0.0]],
        }];
        let query = vec![0.0, 1.0]; // orthogonal → cosine ≈ 0
        let ranked = rank_candidates(&query, &contacts, 0.5, 5);
        assert!(ranked.is_empty());
    }

    #[test]
    fn rank_candidates_sorts_descending_by_score() {
        let contacts = vec![
            ContactSamples {
                contact_id: "c1".into(),
                display_name: "Alice".into(),
                embeddings: vec![vec![0.8, 0.2]],
            },
            ContactSamples {
                contact_id: "c2".into(),
                display_name: "Bob".into(),
                embeddings: vec![vec![0.9, 0.1]],
            },
        ];
        // Query почти идентичен Bob
        let query = vec![0.95, 0.05];
        let ranked = rank_candidates(&query, &contacts, 0.5, 5);
        assert_eq!(ranked.len(), 2);
        assert_eq!(ranked[0].contact_id, "c2");
        assert_eq!(ranked[1].contact_id, "c1");
    }

    #[test]
    fn rank_candidates_respects_top_n() {
        let contacts: Vec<_> = (0..5)
            .map(|i| ContactSamples {
                contact_id: format!("c{i}"),
                display_name: format!("c{i}"),
                embeddings: vec![vec![1.0, 0.0]],
            })
            .collect();
        let query = vec![1.0, 0.0];
        let ranked = rank_candidates(&query, &contacts, 0.5, 2);
        assert_eq!(ranked.len(), 2);
    }
}
