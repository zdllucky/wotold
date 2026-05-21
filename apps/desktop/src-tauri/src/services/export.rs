//! [Phase 4 R4] Композиция Markdown-экспорта звонка.
//!
//! Чистая функция без I/O — `commands/calls.rs::export_call_markdown`
//! сам читает recap/transcript с диска и пишет результат, а формирование
//! Markdown-байтов отдано сюда, чтобы тестировать форматирование без
//! tempfile / Tauri State.

use crate::db::Call;
use crate::AppError;

/// Скомпонованный Markdown export: header (title + meta) + recap + transcript.
///
/// - Если ОБА `recap` и `transcript` отсутствуют — `AppError::Other`,
///   потому что нечего экспортировать.
/// - Если один из них есть — рендерится только присутствующая секция.
///
/// Чистая функция: не трогает диск, не зовёт sqlx, не эмитит events.
pub fn compose_call_markdown(
    call: &Call,
    recap: Option<&str>,
    transcript: Option<&str>,
) -> Result<String, AppError> {
    if recap.is_none() && transcript.is_none() {
        return Err(AppError::Other(
            "Ни recap, ни транскрипт ещё не готовы — нечего экспортировать.".to_string(),
        ));
    }

    let title = call.title.as_deref().unwrap_or("Без названия").trim();
    let mut out = String::with_capacity(8192);
    out.push_str(&format!("# {title}\n\n"));
    out.push_str(&format!("- **Дата**: {}\n", call.started_at));
    if let Some(dur) = call.duration_sec {
        out.push_str(&format!("- **Длительность**: {} сек\n", dur));
    }
    if let Some(provider) = &call.provider {
        out.push_str(&format!("- **Провайдер STT**: {provider}\n"));
    }
    if let Some(lang) = &call.lang_detected {
        out.push_str(&format!("- **Язык**: {lang}\n"));
    }
    out.push_str("\n---\n\n");

    if let Some(r) = recap {
        out.push_str("## Саммари\n\n");
        out.push_str(r.trim_end());
        out.push_str("\n\n---\n\n");
    }
    if let Some(t) = transcript {
        out.push_str("## Расшифровка\n\n");
        out.push_str(t.trim_end());
        out.push('\n');
    }

    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_call() -> Call {
        Call {
            id: "c1".into(),
            title: Some("Quarterly review".into()),
            started_at: "2024-01-01T10:00:00+00:00".into(),
            ended_at: Some("2024-01-01T10:05:00+00:00".into()),
            duration_sec: Some(300),
            status: "ready".into(),
            provider: Some("soniox".into()),
            path_label: "managed".into(),
            lang_detected: Some("ru".into()),
            failed_reason: None,
            recap_failed_reason: None,
            pipeline_step: None,
            pipeline_pct: None,
            pipeline_eta_sec: None,
            upload_bytes: None,
            paused_at: None,
            paused_total_ms: 0,
            created_at: "2024-01-01T10:00:00+00:00".into(),
            updated_at: "2024-01-01T10:05:00+00:00".into(),
        }
    }

    #[test]
    fn composes_full_export_with_recap_and_transcript() {
        let call = sample_call();
        let md = compose_call_markdown(&call, Some("# Recap body"), Some("S1: hello"))
            .expect("should compose");
        assert!(md.contains("# Quarterly review"));
        assert!(md.contains("- **Дата**: 2024-01-01T10:00:00+00:00"));
        assert!(md.contains("- **Длительность**: 300 сек"));
        assert!(md.contains("- **Провайдер STT**: soniox"));
        assert!(md.contains("- **Язык**: ru"));
        assert!(md.contains("## Саммари"));
        assert!(md.contains("# Recap body"));
        assert!(md.contains("## Расшифровка"));
        assert!(md.contains("S1: hello"));
    }

    #[test]
    fn renders_only_transcript_section_when_recap_missing() {
        let call = sample_call();
        let md = compose_call_markdown(&call, None, Some("S1: only-transcript"))
            .expect("should compose");
        assert!(!md.contains("## Саммари"));
        assert!(md.contains("## Расшифровка"));
        assert!(md.contains("S1: only-transcript"));
    }

    #[test]
    fn returns_error_when_both_artifacts_missing() {
        let call = sample_call();
        let err = compose_call_markdown(&call, None, None).unwrap_err();
        assert!(err.to_string().contains("нечего экспортировать"));
    }

    #[test]
    fn falls_back_to_placeholder_title_and_omits_optional_meta() {
        let mut call = sample_call();
        call.title = None;
        call.duration_sec = None;
        call.provider = None;
        call.lang_detected = None;

        let md = compose_call_markdown(&call, Some("only recap"), None).expect("ok");
        assert!(md.contains("# Без названия"));
        assert!(!md.contains("Длительность"));
        assert!(!md.contains("Провайдер STT"));
        assert!(!md.contains("Язык"));
        assert!(md.contains("## Саммари"));
        assert!(!md.contains("## Расшифровка"));
    }
}
