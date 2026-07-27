//! [TD-33] Клей восстановления chunked-записи, отделённый от Tauri-обвязки.
//!
//! Мотивация прямая: `recover_chunked_call` / `spawn_recover_chunked` не имели
//! НИ ОДНОГО теста, при том что все три прод-бага M13 (chunk-0 path mismatch,
//! дропнутый финальный chunk, halt-before-merge) сидели именно в клее, а
//! листья вокруг были покрыты параноидально. Инженерное правило 1 ровно про
//! это: покрытие листьев за покрытие клея не считается.
//!
//! Здесь оркестрация без `AppHandle`, `SqlitePool` и файловой системы: раннер
//! чанка инжектируется замыканием — тот же приём, что `rotate_fn`/`enqueue_fn`
//! в `chunk_orchestrator`. Значит happy-path и fail-path проверяемы юнитом.

use std::future::Future;

use super::chunk_recovery::RecoveryChunk;

/// Что реально произошло при прогоне. Возвращается, а не логируется, чтобы
/// вызывающий (и тест) мог принять решение.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct RecoveryOutcome {
    /// Сколько чанков пытались прогнать.
    pub attempted: usize,
    /// Сколько упало. Частичный провал — легальный исход: relaxed-гейт в
    /// `run_local_inner` соберёт то, что успело.
    pub failed: usize,
}

impl RecoveryOutcome {
    /// Есть ли смысл идти в finalize. Ноль успешных чанков при непустом
    /// списке означает, что STT не отработал ни разу — merge собрал бы пустой
    /// транскрипт и перетёр тем, что было на диске.
    pub fn should_finalize(&self) -> bool {
        self.attempted == 0 || self.failed < self.attempted
    }
}

/// Прогнать STT по недостающим чанкам. Ошибка отдельного чанка НЕ прерывает
/// цикл: остальные могут отработать, а частичный результат лучше пустого.
pub(crate) async fn run_recovery_chunks<F, Fut, T, E>(
    chunks: &[RecoveryChunk],
    mut run_one: F,
) -> RecoveryOutcome
where
    F: FnMut(&RecoveryChunk) -> Fut,
    // Значение успеха игнорируем намеренно: восстановлению важен сам факт,
    // а не что вернул раннер — так замыкание можно отдать `run_chunk` как есть.
    Fut: Future<Output = Result<T, E>>,
    E: std::fmt::Display,
{
    let mut outcome = RecoveryOutcome {
        attempted: chunks.len(),
        failed: 0,
    };
    for rc in chunks {
        if let Err(e) = run_one(rc).await {
            outcome.failed += 1;
            log::warn!("recovery: chunk {} failed: {e}", rc.idx);
        }
    }
    outcome
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    fn chunk(idx: u32) -> RecoveryChunk {
        RecoveryChunk {
            idx,
            start_ms: u64::from(idx) * 600_000,
            end_ms: u64::from(idx + 1) * 600_000,
        }
    }

    #[tokio::test]
    async fn happy_path_runs_every_chunk_in_order() {
        let seen = RefCell::new(Vec::new());
        let chunks = [chunk(0), chunk(1), chunk(2)];
        let out = run_recovery_chunks(&chunks, |rc| {
            seen.borrow_mut().push(rc.idx);
            async { Ok::<(), String>(()) }
        })
        .await;

        assert_eq!(*seen.borrow(), vec![0, 1, 2], "порядок чанков важен");
        assert_eq!(
            out,
            RecoveryOutcome {
                attempted: 3,
                failed: 0
            }
        );
        assert!(out.should_finalize());
    }

    #[tokio::test]
    async fn one_failure_does_not_stop_the_rest() {
        // Регрессия из семейства M13: ранний выход по ошибке одного чанка
        // оставлял остальные нерасшифрованными, а звонок — обрезанным.
        // Частичный результат лучше пустого, relaxed-гейт его соберёт.
        let seen = RefCell::new(Vec::new());
        let chunks = [chunk(0), chunk(1), chunk(2)];
        let out = run_recovery_chunks(&chunks, |rc| {
            seen.borrow_mut().push(rc.idx);
            let fail = rc.idx == 1;
            async move {
                if fail {
                    Err("sherpa-onnx panic".to_string())
                } else {
                    Ok(())
                }
            }
        })
        .await;

        assert_eq!(*seen.borrow(), vec![0, 1, 2], "цикл не прерывается");
        assert_eq!(
            out,
            RecoveryOutcome {
                attempted: 3,
                failed: 1
            }
        );
        assert!(
            out.should_finalize(),
            "что-то расшифровано — finalize нужен"
        );
    }

    #[tokio::test]
    async fn total_failure_blocks_finalize() {
        // Ноль успешных при непустом списке: merge собрал бы пустой
        // транскрипт и перетёр тем, что лежит на диске.
        let chunks = [chunk(0), chunk(1)];
        let out = run_recovery_chunks(&chunks, |_| async { Err::<(), _>("нет модели") }).await;

        assert_eq!(
            out,
            RecoveryOutcome {
                attempted: 2,
                failed: 2
            }
        );
        assert!(
            !out.should_finalize(),
            "нечего мержить — не идём в finalize"
        );
    }

    #[tokio::test]
    async fn nothing_to_run_still_finalizes() {
        // Все чанки уже done — реконструкция вернула пустой список. Это НЕ
        // ошибка: assembly + merge + recap всё равно нужны, ровно этот путь
        // и чинил halt-before-merge.
        let out = run_recovery_chunks(&[], |_| async { Ok::<(), String>(()) }).await;

        assert_eq!(
            out,
            RecoveryOutcome {
                attempted: 0,
                failed: 0
            }
        );
        assert!(
            out.should_finalize(),
            "нечего расшифровывать ≠ нечего собирать"
        );
    }
}
