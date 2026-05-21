//! [Phase 3 R2] Typed pipeline stages — `Stage` enum заменяет magic numbers
//! 1..5 разбросанные по `run_inner`.
//!
//! Раньше каждый emit_progress callsite таскал свой step number inline:
//!
//! ```ignore
//! emit_progress(pool, app, &ctx.call_id, 1, 0, ...).await;
//! ```
//!
//! При смене порядка stages (исторически шаг 3 эмитился после шага 4) приходилось
//! помнить какие числа отвечают за какие фазы. Теперь:
//!
//! ```ignore
//! emit_progress(pool, app, &ctx.call_id, Stage::Upload.step(), 0, ...).await;
//! ```
//!
//! UI-listener'ы зависят от того что step number стабилен (frontend
//! ProgressRail / PipelineStrip отрисовывают именно эти числа). Поэтому
//! `step()` — это часть контракта, не deal-of-the-day.
//!
//! TIMING contract сохраняется 1-в-1: эмиссия событий идёт в
//! `Upload (1) → Transcribe (2) → MergeArtifacts (4) → RecognizeSpeakers (3) → Recap (5)`
//! — шаг 3 идёт ПОСЛЕ шага 4 потому что speaker recognition работает по уже
//! персистированному merged-транскрипту (transcript.md / raw_stt.json должны
//! быть на диске до cluster pipeline'а, иначе UI словит partial state).

/// Логические этапы pipeline'а. `step()` — стабильное число, которое
/// frontend listener'ы матчат для рендера ProgressRail.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    /// Загрузка аудио на STT-провайдера. В текущей реализации мгновенный
    /// «псевдо-шаг» (pct 0→100 одной транзакцией), потому что provider'ы
    /// не отдают per-byte progress. upload_bytes hint всё равно эмитим
    /// для UI'я.
    Upload,
    /// STT-вызов: provider transcribe (с retry + fallback внутри).
    Transcribe,
    /// Speaker recognition: cluster extraction + matching → suggestion.
    /// Non-fatal: ошибки логируются, pipeline продолжает на recap.
    RecognizeSpeakers,
    /// Merge mic+system → persist `raw_stt.json` + `transcript.md`.
    /// Идёт ДО RecognizeSpeakers (см. file doc-comment).
    MergeArtifacts,
    /// LLM recap.md + action_items. Non-fatal: ошибки → `recap_failed_reason`.
    Recap,
}

impl Stage {
    /// Стабильное step number для UI listener'ов. Не менять без миграции
    /// frontend ProgressRail / PipelineStrip.
    pub fn step(self) -> u8 {
        match self {
            Stage::Upload => 1,
            Stage::Transcribe => 2,
            Stage::RecognizeSpeakers => 3,
            Stage::MergeArtifacts => 4,
            Stage::Recap => 5,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn step_numbers_are_stable_contract() {
        // Эти числа — публичный контракт с frontend'ом. Если этот тест
        // упал — значит фронт тоже надо обновить (CallStateTag/ProgressRail).
        assert_eq!(Stage::Upload.step(), 1);
        assert_eq!(Stage::Transcribe.step(), 2);
        assert_eq!(Stage::RecognizeSpeakers.step(), 3);
        assert_eq!(Stage::MergeArtifacts.step(), 4);
        assert_eq!(Stage::Recap.step(), 5);
    }

    #[test]
    fn step_numbers_are_unique() {
        let stages = [
            Stage::Upload,
            Stage::Transcribe,
            Stage::RecognizeSpeakers,
            Stage::MergeArtifacts,
            Stage::Recap,
        ];
        let mut steps: Vec<u8> = stages.iter().map(|s| s.step()).collect();
        steps.sort();
        steps.dedup();
        assert_eq!(steps.len(), 5, "каждая stage имеет уникальный step");
    }
}
