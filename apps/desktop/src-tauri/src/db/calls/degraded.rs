//! [TD-37] Персистентные degraded-флаги звонка.
//!
//! Инженерное правило 3: путь «warn-and-continue», влияющий на результат
//! звонка, обязан оставлять след, доступный UI. До этого все такие состояния
//! жили только в логе — пользователь не отличал «в звонке правда один голос»
//! от «системная дорожка не разделилась и всё ушло в speaker:0».
//!
//! Хранение — JSON-массив кодов в одной колонке `calls.degraded_flags`.
//! Читают их всегда целиком и всегда для одного звонка, а писать приходится из
//! середины пайплайна, где лишняя связанная таблица — ещё одна точка отказа на
//! пути, который и так уже деградировал.

use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use crate::AppError;

/// Что именно пошло не идеально. Коды стабильны — они уходят в БД и в UI,
/// расшифровка живёт в локалях фронтенда.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DegradedFlag {
    /// Расшифрованы не все чанки: транскрипт собран из того, что удалось.
    PartialTranscript,
    /// Системная дорожка не разделена на голоса — все реплики собеседников
    /// в одном спикере.
    SystemTrackNotDiarized,
    /// Микрофонная дорожка не разделена на голоса (при включённой настройке).
    MicTrackNotDiarized,
    /// Кластеризация голосов не отработала — спикеры остались анонимными.
    SpeakerClusteringFailed,
    /// Язык звонка определён, но пере-расшифровка под него не удалась:
    /// часть текста осталась распознанной не тем языком.
    LanguageRepinFailed,
    /// [TD-45] В микрофонной дорожке был провал устройства; дыра заполнена
    /// тишиной, чтобы дорожка не уехала относительно системной.
    MicTrackGapPadded,
    /// [TD-45] То же на системной дорожке.
    SystemTrackGapPadded,
    /// [T5/R15] Запись остановлена приложением после тишины, тихий хвост
    /// отрезан. Тот же принцип, что у `*_gap_padded`: файл, который
    /// пользователь считает записью разговора, изменён не им — значит это
    /// обязано быть видимым, а не только в логе.
    AutoStoppedOnSilence,
}

impl DegradedFlag {
    /// Код для БД и фронтенда.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::PartialTranscript => "partial_transcript",
            Self::SystemTrackNotDiarized => "system_track_not_diarized",
            Self::MicTrackNotDiarized => "mic_track_not_diarized",
            Self::SpeakerClusteringFailed => "speaker_clustering_failed",
            Self::LanguageRepinFailed => "language_repin_failed",
            Self::MicTrackGapPadded => "mic_track_gap_padded",
            Self::SystemTrackGapPadded => "system_track_gap_padded",
            Self::AutoStoppedOnSilence => "auto_stopped_on_silence",
        }
    }
}

/// Флаги звонка. Пустой вектор = оговорок нет.
pub async fn list_degraded_flags(
    pool: &SqlitePool,
    call_id: &str,
) -> Result<Vec<String>, AppError> {
    let raw: Option<Option<String>> =
        sqlx::query_scalar("SELECT degraded_flags FROM calls WHERE id = ?1")
            .bind(call_id)
            .fetch_optional(pool)
            .await?;
    Ok(parse_flags(raw.flatten().as_deref()))
}

/// Добавить флаг. Идемпотентно: повторный вызов не плодит дубли — пайплайн
/// может пройти по одной и той же деградации дважды (retry чанка, reprocess).
///
/// Ошибка записи НЕ пробрасывается вызывающему как фатальная: это диагностика
/// на пути, который уже деградировал, и уронить из-за неё звонок было бы
/// хуже самой деградации. Возвращаем `Result` для тестов и логирования.
pub async fn add_degraded_flag(
    pool: &SqlitePool,
    call_id: &str,
    flag: DegradedFlag,
) -> Result<(), AppError> {
    let mut flags = list_degraded_flags(pool, call_id).await?;
    let code = flag.as_str().to_string();
    if flags.iter().any(|f| f == &code) {
        return Ok(());
    }
    flags.push(code);
    let json = serde_json::to_string(&flags)
        .map_err(|e| AppError::Other(format!("degraded_flags serialize: {e}")))?;
    sqlx::query("UPDATE calls SET degraded_flags = ?1, updated_at = ?2 WHERE id = ?3")
        .bind(json)
        .bind(chrono::Utc::now().to_rfc3339())
        .bind(call_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Сбросить флаги — при reprocess звонок обрабатывается заново, и оговорки
/// прошлого прогона к новому результату отношения не имеют.
pub async fn clear_degraded_flags(pool: &SqlitePool, call_id: &str) -> Result<(), AppError> {
    sqlx::query("UPDATE calls SET degraded_flags = NULL WHERE id = ?1")
        .bind(call_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Разбор колонки. Битое значение — не повод падать: это диагностика, и
/// «оговорок не показали» лучше, чем «звонок не открылся».
fn parse_flags(raw: Option<&str>) -> Vec<String> {
    raw.and_then(|s| serde_json::from_str::<Vec<String>>(s).ok())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::calls::insert_recording;
    use crate::db::test_support::fresh_db;

    #[tokio::test]
    async fn flags_start_empty_and_accumulate() {
        let db = fresh_db().await;
        let call = insert_recording(&db.pool, "local").await.unwrap();
        assert!(list_degraded_flags(&db.pool, &call.id)
            .await
            .unwrap()
            .is_empty());

        add_degraded_flag(&db.pool, &call.id, DegradedFlag::PartialTranscript)
            .await
            .unwrap();
        add_degraded_flag(&db.pool, &call.id, DegradedFlag::SystemTrackNotDiarized)
            .await
            .unwrap();

        let flags = list_degraded_flags(&db.pool, &call.id).await.unwrap();
        assert_eq!(
            flags,
            vec!["partial_transcript", "system_track_not_diarized"],
            "порядок — в котором деградации случились"
        );
    }

    #[tokio::test]
    async fn adding_same_flag_twice_does_not_duplicate() {
        // Пайплайн проходит по одной деградации дважды при retry чанка и
        // reprocess — список не должен расти от повторов.
        let db = fresh_db().await;
        let call = insert_recording(&db.pool, "local").await.unwrap();
        for _ in 0..3 {
            add_degraded_flag(&db.pool, &call.id, DegradedFlag::PartialTranscript)
                .await
                .unwrap();
        }
        assert_eq!(
            list_degraded_flags(&db.pool, &call.id).await.unwrap().len(),
            1
        );
    }

    #[tokio::test]
    async fn clear_wipes_flags_for_reprocess() {
        let db = fresh_db().await;
        let call = insert_recording(&db.pool, "local").await.unwrap();
        add_degraded_flag(&db.pool, &call.id, DegradedFlag::SpeakerClusteringFailed)
            .await
            .unwrap();
        clear_degraded_flags(&db.pool, &call.id).await.unwrap();
        assert!(list_degraded_flags(&db.pool, &call.id)
            .await
            .unwrap()
            .is_empty());
    }

    #[tokio::test]
    async fn unknown_call_reads_as_no_flags_not_error() {
        let db = fresh_db().await;
        assert!(list_degraded_flags(&db.pool, "ghost")
            .await
            .unwrap()
            .is_empty());
    }

    #[test]
    fn broken_column_degrades_to_empty_instead_of_failing() {
        // Битый JSON в диагностическом поле не должен мешать открыть звонок.
        assert!(parse_flags(Some("не json")).is_empty());
        assert!(parse_flags(Some("{}")).is_empty());
        assert!(parse_flags(None).is_empty());
        assert_eq!(
            parse_flags(Some(r#"["partial_transcript"]"#)),
            vec!["partial_transcript"]
        );
    }
}
