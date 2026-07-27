use std::path::Path;

use sqlx::{
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions},
    SqlitePool,
};

use crate::AppError;

mod action_items;
// [M15.2] Ассистент: чаты/сообщения/пассажи + FTS5. pub(crate) — вызовы из
// assistant::{indexer,retrieval} и commands::assistant (M15.3+).
pub(crate) mod assistant;
pub(crate) mod assistant_embeddings;
pub(crate) mod assistant_search;
mod calls;
// [M13.1.3b] Chunked pipelined transcription — call_chunks table helpers.
// pub(crate) чтобы pipeline::chunk_runner мог вызывать insert/mark/list.
pub(crate) mod chunks;
mod contacts;
// [M14 T-02] Decisions / open_questions tables (migration 0015).
// pub(crate) чтобы pipeline::recap мог replace на persist.
pub(crate) mod decisions;
pub(crate) mod open_questions;
mod settings;
// [M14 T-14] Local-only summary generation log.
pub(crate) mod telemetry;
// [Phase 3 R9] pub(crate) чтобы pipeline::voice_backfill мог вызывать
// evict_old_voice_samples. Раньше backfill жил внутри db::set_call_speaker_cluster,
// поэтому хватало private. Теперь side-effect снаружи db/ — нужен crate-wide path.
pub(crate) mod voice_samples;

pub use action_items::{list_action_items, replace_action_items, ActionItem, ActionItemInput};
pub use calls::{
    auto_bind_high_confidence_speakers, auto_bind_owner_speaker, confirm_call_speaker,
    delete_call_and_samples, ensure_call_speakers_present, fail_recording,
    fail_recording_with_reason, finish_recording, get_call, insert_recording,
    insert_speaker_suggestions, list_call_speakers, list_calls, list_interrupted_failed_calls,
    list_orphan_recording_ids, mark_call_ready, pause_call, prune_call_speakers_not_in,
    resume_call, set_call_meta, set_call_progress, set_call_speaker_cluster,
    set_call_speaker_suggestion, set_call_title, set_recap_failed_reason, set_recap_failure,
    set_summary_metadata, sweep_stale_calls, unbind_call_speaker, update_call_duration, Call,
    CallSpeakerView, SummaryMetadata,
};
pub use contacts::{
    create_contact, delete_contact, ensure_owner_contact, list_contacts, rename_owner_contact,
    update_contact, Contact, ContactInput, OwnerContact,
};
pub use settings::{get_setting, set_setting};
pub use voice_samples::{delete_voice_sample, list_voice_samples, VoiceSampleView};
// [B3.8] evict_old_voice_samples + MAX_SAMPLES_PER_CONTACT — internal API,
// доступ через crate::db::voice_samples::* (вызовы из db::calls::confirm_call_speaker
// и pipeline::voice_backfill::maybe_backfill_voice_sample — Phase 3 R9).

const DB_FILE: &str = "app.db";

pub async fn init(app_data_dir: &Path) -> Result<SqlitePool, AppError> {
    let path = app_data_dir.join(DB_FILE);

    // [B16 audit P0] integrity_check перед открытием — если БД corrupt (partial
    // WAL write, force-quit во время migrations), переименовываем в *.corrupt-{ts}
    // и стартуем с пустой. Юзер увидит модал в UI через event 'db:reset' что
    // была пересборка БД (пока без модала — TODO в #refactor).
    if path.exists() {
        if let Err(e) = quick_integrity_check(&path).await {
            let ts = chrono::Utc::now().format("%Y%m%d-%H%M%S");
            let corrupt_path = app_data_dir.join(format!("{DB_FILE}.corrupt-{ts}"));
            log::error!(
                "SQLite integrity check failed: {e}. Renaming {} → {}",
                path.display(),
                corrupt_path.display()
            );
            if let Err(rename_err) = quarantine_corrupt_db(&path, &corrupt_path) {
                // [TD-15] Раньше ошибка глоталась через `.ok()` — код молча шёл
                // открывать заведомо битый файл, и миграции падали с невнятной
                // ошибкой. Удалять при неудаче НЕ будем: карантин существует
                // ради сохранности данных для восстановления.
                log::error!(
                    "не удалось изолировать повреждённую БД {} → {}: {rename_err}. \
                     Приложение продолжит на повреждённом файле; перенесите его \
                     вручную, чтобы стартовать с чистой базой.",
                    path.display(),
                    corrupt_path.display()
                );
            }
        }
    }

    let options = SqliteConnectOptions::new()
        .filename(&path)
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        // [B16 audit P1] busy_timeout 5s — concurrent writes (sweep + pipeline)
        // могут upcкать SQLITE_BUSY; даём wait вместо мгновенного fail.
        .busy_timeout(std::time::Duration::from_secs(5))
        .foreign_keys(true);

    let pool = SqlitePoolOptions::new()
        .max_connections(8)
        .connect_with(options)
        .await?;

    sqlx::migrate!("./migrations").run(&pool).await?;

    Ok(pool)
}

/// Открывает БД на одиночный pragma integrity_check; если не вернёт 'ok' —
/// файл повреждён. Используем минимальный pool (1 connection) + закрываем
/// сразу после check чтобы не держать handle.
/// [TD-15] Изолировать повреждённую БД вместе с её WAL-сайдкарами.
///
/// Раньше переименовывался только `app.db`, а `app.db-wal` / `app.db-shm`
/// оставались на месте. SQLite при создании новой пустой базы попытался бы
/// восстановить **старый WAL против нового файла** — фреймы чужих страниц
/// влились бы в свежую БД. Поэтому переносим все три; отсутствующий сайдкар
/// не ошибка (WAL мог быть уже вычекпойнчен).
fn quarantine_corrupt_db(db_path: &Path, corrupt_path: &Path) -> std::io::Result<()> {
    std::fs::rename(db_path, corrupt_path)?;

    for suffix in ["-wal", "-shm"] {
        let from = sidecar_path(db_path, suffix);
        let to = sidecar_path(corrupt_path, suffix);
        match std::fs::rename(&from, &to) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => {
                // Основной файл уже изолирован; осиротевший сайдкар опаснее
                // молчания — он и есть источник «влившегося чужого WAL».
                log::error!(
                    "повреждённая БД: не удалось перенести {}: {e}",
                    from.display()
                );
            }
        }
    }
    Ok(())
}

/// `app.db` + `-wal` → `app.db-wal`. Суффикс клеится к имени файла целиком,
/// а не через `with_extension` (тот срезал бы `.db`).
fn sidecar_path(base: &Path, suffix: &str) -> std::path::PathBuf {
    let mut os = base.as_os_str().to_os_string();
    os.push(suffix);
    std::path::PathBuf::from(os)
}

async fn quick_integrity_check(path: &Path) -> Result<(), AppError> {
    let options = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(false)
        .journal_mode(SqliteJournalMode::Wal)
        .busy_timeout(std::time::Duration::from_secs(3));
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await?;
    let row: (String,) = sqlx::query_as("PRAGMA integrity_check")
        .fetch_one(&pool)
        .await?;
    pool.close().await;
    if row.0 != "ok" {
        return Err(AppError::Other(format!(
            "integrity_check returned: {}",
            row.0
        )));
    }
    Ok(())
}

#[cfg(test)]
pub mod test_support {
    use super::*;
    use tempfile::TempDir;

    pub struct TestDb {
        pub pool: SqlitePool,
        _dir: TempDir,
    }

    pub async fn fresh_db() -> TestDb {
        let dir = tempfile::tempdir().expect("create temp dir");
        let pool = init(dir.path()).await.expect("init db");
        TestDb { pool, _dir: dir }
    }

    /// [TD-43] Контакт без consent/samples — минимальная строка для тестов
    /// привязки спикеров. Жил в `calls/speakers.rs::tests`; поднят сюда,
    /// когда suggestion-функции переехали в `calls/suggestions.rs` и оба
    /// тест-модуля стали нуждаться в одном хелпере.
    pub async fn insert_contact_row(pool: &SqlitePool, name: &str) -> String {
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();
        sqlx::query(
            "INSERT INTO contacts (id, display_name, is_owner, attributes, created_at, updated_at)
             VALUES (?1, ?2, 0, '{}', ?3, ?3)",
        )
        .bind(&id)
        .bind(name)
        .bind(&now)
        .execute(pool)
        .await
        .unwrap();
        id
    }

    /// [TD-43] Строка `call_speakers` без привязки (`confirmed = 0`).
    pub async fn insert_speaker_row(
        pool: &SqlitePool,
        call_id: &str,
        speaker_tag: &str,
        suggestion_contact_id: Option<&str>,
    ) -> String {
        let id = uuid::Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO call_speakers (id, call_id, speaker_tag, suggestion_contact_id, suggestion_score, suggestion_source, confirmed)
             VALUES (?1, ?2, ?3, ?4, 0.85, 'embedding', 0)",
        )
        .bind(&id)
        .bind(call_id)
        .bind(speaker_tag)
        .bind(suggestion_contact_id)
        .execute(pool)
        .await
        .unwrap();
        id
    }
}

#[cfg(test)]
mod tests {
    use super::test_support::fresh_db;
    use super::{quarantine_corrupt_db, sidecar_path};

    // ============================================================
    // [TD-15] quarantine_corrupt_db — WAL-сайдкары
    // ============================================================

    #[test]
    fn quarantine_moves_db_and_both_sidecars() {
        // Регрессия: переносился только app.db, а -wal/-shm оставались.
        // SQLite восстановил бы СТАРЫЙ WAL против новой пустой базы —
        // фреймы чужих страниц влились бы в свежую БД.
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("app.db");
        let wal = dir.path().join("app.db-wal");
        let shm = dir.path().join("app.db-shm");
        std::fs::write(&db, b"corrupt").unwrap();
        std::fs::write(&wal, b"stale-wal").unwrap();
        std::fs::write(&shm, b"stale-shm").unwrap();

        let corrupt = dir.path().join("app.db.corrupt-20260724");
        quarantine_corrupt_db(&db, &corrupt).unwrap();

        assert!(!db.exists(), "app.db обязан уехать");
        assert!(
            !wal.exists(),
            "-wal не должен остаться: вольётся в новую базу"
        );
        assert!(!shm.exists(), "-shm не должен остаться");

        assert!(corrupt.exists());
        assert!(dir.path().join("app.db.corrupt-20260724-wal").exists());
        assert!(dir.path().join("app.db.corrupt-20260724-shm").exists());
    }

    #[test]
    fn quarantine_ok_when_sidecars_absent() {
        // WAL мог быть вычекпойнчен — отсутствие сайдкаров не ошибка.
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("app.db");
        std::fs::write(&db, b"corrupt").unwrap();

        let corrupt = dir.path().join("app.db.corrupt-x");
        quarantine_corrupt_db(&db, &corrupt).expect("отсутствие сайдкаров — не ошибка");
        assert!(corrupt.exists());
        assert!(!db.exists());
    }

    #[test]
    fn sidecar_path_appends_not_replaces_extension() {
        // with_extension срезал бы `.db` — путь стал бы `app-wal`.
        let p = sidecar_path(std::path::Path::new("/x/app.db"), "-wal");
        assert_eq!(p, std::path::PathBuf::from("/x/app.db-wal"));
    }

    use sqlx::Row;

    #[tokio::test]
    async fn init_runs_migrations_and_creates_settings_table() {
        let db = fresh_db().await;
        let row = sqlx::query(
            "SELECT count(*) AS n FROM sqlite_master WHERE type='table' AND name='settings'",
        )
        .fetch_one(&db.pool)
        .await
        .unwrap();
        let n: i64 = row.try_get("n").unwrap();
        assert_eq!(n, 1, "settings table must exist after migrations");
    }

    #[tokio::test]
    async fn init_enables_foreign_keys() {
        let db = fresh_db().await;
        let row = sqlx::query("PRAGMA foreign_keys")
            .fetch_one(&db.pool)
            .await
            .unwrap();
        let on: i64 = row.try_get(0).unwrap();
        assert_eq!(on, 1);
    }
}
