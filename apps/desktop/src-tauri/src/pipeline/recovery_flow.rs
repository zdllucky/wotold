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

/// Чем закончилось восстановление целиком. Возвращается, а не только пишется
/// в лог: иначе решение «идти ли в finalize» проверяемо лишь глазами по
/// логу. Прод не ветвится по вердикту (все решения приняты внутри) — он его
/// логирует; смысл возврата в том, что тест видит выбор, а не последствия.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RecoveryVerdict {
    /// Реконструкция строк из диска не удалась — ни STT, ни finalize.
    ReconstructFailed,
    /// STT не отработал ни разу: finalize пропущен, файлы на диске целы.
    NothingTranscribed(RecoveryOutcome),
    /// Finalize вызван.
    Finalized(RecoveryOutcome),
}

/// [TD-33] Полный клей восстановления: реконструкция строк → STT недостающих
/// чанков → решение о finalize. Все три шага инжектируются, потому что в
/// проде за ними стоят `SqlitePool`, `CallStore`, файловая система и
/// `AppHandle` — а сломаться может именно порядок и условия, а не они сами.
///
/// Инвариант, ради которого функция и существует: **finalize не вызывается**
/// ни после провала реконструкции, ни когда не расшифровался ни один чанк.
/// Merge в этот момент собрал бы пустой транскрипт и перетёр им то, что лежит
/// на диске, — то есть восстановление уничтожило бы запись.
pub(crate) async fn run_recovery_flow<RecFut, E1, Run, RunFut, T, E2, Fin, FinFut>(
    call_id: &str,
    reconstruct: RecFut,
    run_one: Run,
    finalize: Fin,
) -> RecoveryVerdict
where
    RecFut: Future<Output = Result<Vec<RecoveryChunk>, E1>>,
    E1: std::fmt::Display,
    Run: FnMut(&RecoveryChunk) -> RunFut,
    RunFut: Future<Output = Result<T, E2>>,
    E2: std::fmt::Display,
    Fin: FnOnce() -> FinFut,
    FinFut: Future<Output = ()>,
{
    let to_run = match reconstruct.await {
        Ok(v) => v,
        Err(e) => {
            log::warn!("recovery[{call_id}]: reconstruct failed: {e}");
            return RecoveryVerdict::ReconstructFailed;
        }
    };
    log::info!(
        "recovery[{call_id}]: {} chunk(s) to (re)transcribe",
        to_run.len()
    );

    let outcome = run_recovery_chunks(&to_run, run_one).await;
    if !outcome.should_finalize() {
        log::warn!(
            "recovery[{call_id}]: ни один из {} chunk'ов не расшифрован — \
             finalize пропущен, файлы на диске не тронуты",
            outcome.attempted
        );
        return RecoveryVerdict::NothingTranscribed(outcome);
    }

    finalize().await;
    RecoveryVerdict::Finalized(outcome)
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

    // ============================================================
    // [TD-33] run_recovery_flow — реконструкция → STT → finalize
    // ============================================================

    /// Счётчик вызовов finalize. Именно его отсутствие/наличие и есть
    /// предмет проверки: finalize запускает merge, который перетирает диск.
    fn flow_finalize(calls: &RefCell<usize>) -> impl FnOnce() -> std::future::Ready<()> + '_ {
        move || {
            *calls.borrow_mut() += 1;
            std::future::ready(())
        }
    }

    #[tokio::test]
    async fn reconstruct_failure_never_reaches_finalize() {
        // Реконструкция не смогла разложить чанки по диску. Пойти дальше
        // означало бы смержить пустоту поверх записи.
        let finalized = RefCell::new(0usize);
        let ran = RefCell::new(0usize);

        let verdict = run_recovery_flow(
            "call-1",
            async { Err::<Vec<RecoveryChunk>, _>("chunks_dir unreadable") },
            |_| {
                *ran.borrow_mut() += 1;
                async { Ok::<(), String>(()) }
            },
            flow_finalize(&finalized),
        )
        .await;

        assert_eq!(verdict, RecoveryVerdict::ReconstructFailed);
        assert_eq!(*ran.borrow(), 0, "STT не запускается без списка чанков");
        assert_eq!(*finalized.borrow(), 0, "finalize НЕ вызывается");
    }

    #[tokio::test]
    async fn happy_path_finalizes_once_after_all_chunks() {
        let finalized = RefCell::new(0usize);
        let seen = RefCell::new(Vec::new());

        let verdict = run_recovery_flow(
            "call-1",
            async { Ok::<_, String>(vec![chunk(0), chunk(1)]) },
            |rc| {
                seen.borrow_mut().push(rc.idx);
                async { Ok::<(), String>(()) }
            },
            flow_finalize(&finalized),
        )
        .await;

        assert_eq!(*seen.borrow(), vec![0, 1]);
        assert_eq!(
            verdict,
            RecoveryVerdict::Finalized(RecoveryOutcome {
                attempted: 2,
                failed: 0
            })
        );
        assert_eq!(*finalized.borrow(), 1, "ровно один finalize");
    }

    #[tokio::test]
    async fn partial_failure_still_finalizes() {
        // Часть чанков расшифрована — relaxed-гейт в run_local_inner соберёт
        // то, что есть. Обрезанный транскрипт лучше залипшего звонка.
        let finalized = RefCell::new(0usize);

        let verdict = run_recovery_flow(
            "call-1",
            async { Ok::<_, String>(vec![chunk(0), chunk(1), chunk(2)]) },
            |rc| {
                let fail = rc.idx == 2;
                async move {
                    if fail {
                        Err("sherpa-onnx panic".to_string())
                    } else {
                        Ok(())
                    }
                }
            },
            flow_finalize(&finalized),
        )
        .await;

        assert_eq!(
            verdict,
            RecoveryVerdict::Finalized(RecoveryOutcome {
                attempted: 3,
                failed: 1
            })
        );
        assert_eq!(*finalized.borrow(), 1);
    }

    #[tokio::test]
    async fn total_stt_failure_never_reaches_finalize() {
        let finalized = RefCell::new(0usize);

        let verdict = run_recovery_flow(
            "call-1",
            async { Ok::<_, String>(vec![chunk(0), chunk(1)]) },
            |_| async { Err::<(), _>("модель не скачана") },
            flow_finalize(&finalized),
        )
        .await;

        assert_eq!(
            verdict,
            RecoveryVerdict::NothingTranscribed(RecoveryOutcome {
                attempted: 2,
                failed: 2
            })
        );
        assert_eq!(*finalized.borrow(), 0, "merge собрал бы пустоту");
    }

    #[tokio::test]
    async fn empty_reconstruct_finalizes_without_stt() {
        // Все чанки уже done — именно этот путь чинил halt-before-merge:
        // расшифровывать нечего, но собирать есть что.
        let finalized = RefCell::new(0usize);
        let ran = RefCell::new(0usize);

        let verdict = run_recovery_flow(
            "call-1",
            async { Ok::<_, String>(Vec::new()) },
            |_| {
                *ran.borrow_mut() += 1;
                async { Ok::<(), String>(()) }
            },
            flow_finalize(&finalized),
        )
        .await;

        assert_eq!(
            verdict,
            RecoveryVerdict::Finalized(RecoveryOutcome {
                attempted: 0,
                failed: 0
            })
        );
        assert_eq!(*ran.borrow(), 0);
        assert_eq!(*finalized.borrow(), 1, "finalize нужен и без STT");
    }
}
