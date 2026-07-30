//! [T1] Silence watch — решение «запись идёт, но в ней тишина».
//!
//! # Зачем
//!
//! Забытая запись живёт часами: `duration_sec` считается по wall-clock, а
//! chunked-пайплайн каждые 10 минут гонит тишину через whisper и добирает
//! галлюцинации в транскрипт и рекап. R14 паспорта разрешает предложить стоп
//! (15 мин тишины) и остановить самим (настраиваемый порог) с подрезкой
//! тихого хвоста.
//!
//! # Что здесь
//!
//! [`SilenceWatch`] здесь — чистое решающее ядро: ни времени, ни I/O, ни
//! каналов.
//! Часы приходят таймстемпами RMS-сэмплов (`ts_ms` от начала записи), так
//! что тесты гоняют часы вручную и не зависят от реального времени
//! (инженерное правило 6). Обёртка с каналами — [`task::run`], формой
//! [`crate::pipeline::chunk_orchestrator::run`].
//!
//! # Сигнал
//!
//! Только RMS `max(mic, system)` ниже [`SilenceWatchConfig::floor_rms`] —
//! R14 запрещает домысливать «идёт ли собрание» по процессам и frontmost-app
//! (это территория R3, и она ненадёжна, пока микрофон занят нами).
//!
//! # Гистерезис
//!
//! Тишину сбрасывает только **непрерывный** звук длиной
//! `voice_hysteresis_ms`: одиночный щелчок клавиатуры или короткий системный
//! блип не должны обнулять 25-минутный тихий хвост. Обратная сторона
//! принята сознательно: если в звонке роняют одиночные слова короче порога и
//! ничего больше, тихий run не сбросится и авто-стоп сработает. У юзера есть
//! подсказка на 15 минуте и `never` в настройках.
//!
//! # Пауза
//!
//! Пауза записи тишиной **не** считается: на паузе рекордеры делают
//! `level.reset()` (`AudioRecorder.swift`), то есть RMS честно нулевой, а
//! юзер при этом активно управляет записью. Тот же guard, что у
//! `chunk_orchestrator` (инженерное правило 2 — twin parity).

/// RMS-порог тишины (0..1). Тот же, что `ChunkOrchestratorConfig::silence_threshold`
/// — дорожки и сэмплы одни и те же, расхождение порогов дало бы два разных
/// понятия «тихо» в одном процессе.
pub const DEFAULT_FLOOR_RMS: f32 = 0.01;

/// Сколько непрерывного звука сбрасывает тихий run. 800 мс — произнесённая
/// фраза проходит, щелчок и короткий блип нет.
pub const DEFAULT_VOICE_HYSTERESIS_MS: u64 = 800;

/// Порог подсказки «может, остановить?». Фиксированный (R14) — настройкой
/// регулируется только сам факт подсказки, не её момент.
pub const SUGGEST_AFTER_MS: u64 = 15 * 60 * 1_000;

/// Сколько тишины оставляем после последнего звука при подрезке. Страховка
/// от обрезанного последнего слова: RMS считается по 100-мс буферам, затухание
/// фразы может уехать за порог раньше, чем она реально закончилась.
pub const DEFAULT_TAIL_PAD_MS: u64 = 5_000;

/// Тюнинг наблюдателя. `Copy` — конфиг мелкий и читается на каждом сэмпле.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SilenceWatchConfig {
    /// RMS ниже этого значения — тишина.
    pub floor_rms: f32,
    /// Непрерывный звук такой длины сбрасывает тихий run.
    pub voice_hysteresis_ms: u64,
    /// Порог подсказки. `None` — настройка `recording.silence_prompt` выключена.
    pub suggest_after_ms: Option<u64>,
    /// Порог авто-стопа. `None` — настройка `recording.silence_auto_stop=never`.
    pub auto_stop_after_ms: Option<u64>,
    /// Хвост тишины, остающийся после подрезки.
    pub tail_pad_ms: u64,
}

impl Default for SilenceWatchConfig {
    fn default() -> Self {
        Self {
            floor_rms: DEFAULT_FLOOR_RMS,
            voice_hysteresis_ms: DEFAULT_VOICE_HYSTERESIS_MS,
            suggest_after_ms: Some(SUGGEST_AFTER_MS),
            auto_stop_after_ms: Some(30 * 60 * 1_000),
            tail_pad_ms: DEFAULT_TAIL_PAD_MS,
        }
    }
}

/// Решение по очередному сэмплу.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SilenceEvent {
    /// Ничего делать не надо.
    None,
    /// Предложить остановить запись. Выдаётся один раз на тихий run.
    SuggestStop {
        /// Сколько уже длится тишина.
        silent_for_ms: u64,
        /// Сколько осталось до авто-стопа, если юзер промолчит. `None` —
        /// авто-стоп выключен. Именно «сколько осталось», а не абсолютный
        /// момент: у фронта нет точки отсчёта записи, и он всё равно вычел бы
        /// одно из другого — только с риском разъехаться на паузах.
        auto_stop_in_ms: Option<u64>,
    },
    /// Остановить запись самим и подрезать WAV до `trim_at_ms`.
    /// Выдаётся один раз на тихий run.
    AutoStop {
        /// Точка реза от начала записи = начало тишины + `tail_pad_ms`,
        /// но не дальше уже записанного.
        trim_at_ms: u64,
        /// Сколько тишины набралось к моменту решения. По определению ≈ порог
        /// авто-стопа, но событие обязано быть самодостаточным: уведомление
        /// пишет «остановлено после N минут тишины», и выводить N из настройки
        /// значило бы соврать, если её поменяли во время записи.
        silent_for_ms: u64,
    },
}

/// Решающее ядро. НЕ thread-safe и не должно быть: живёт на одной задаче,
/// как `SilenceDetector` в оркестраторе.
#[derive(Debug)]
pub struct SilenceWatch {
    cfg: SilenceWatchConfig,
    /// Начало текущего тихого run'а. `None` — сейчас звук либо run сброшен.
    silence_started_ms: Option<u64>,
    /// Начало текущего непрерывного звука — база гистерезиса.
    voice_since_ms: Option<u64>,
    /// Подсказка по текущему run'у уже отдана.
    suggested: bool,
    /// Авто-стоп по текущему run'у уже отдан. Идемпотентность нужна не для
    /// красоты: между решением и фактической остановкой сайдкара проходят
    /// сотни миллисекунд, за которые прилетит ещё несколько тихих сэмплов.
    stopped: bool,
    paused: bool,
}

impl SilenceWatch {
    pub fn new(cfg: SilenceWatchConfig) -> Self {
        Self {
            cfg,
            silence_started_ms: None,
            voice_since_ms: None,
            suggested: false,
            stopped: false,
            paused: false,
        }
    }

    /// Есть ли вообще за чем следить. `false` — обе настройки выключены, и
    /// caller может не поднимать задачу вовсе.
    pub fn is_armed(cfg: &SilenceWatchConfig) -> bool {
        cfg.suggest_after_ms.is_some() || cfg.auto_stop_after_ms.is_some()
    }

    /// Очередной RMS-сэмпл. `ts_ms` — смещение от начала записи (та же
    /// шкала, что у оркестратора: wall-clock от `session.started_at`,
    /// включая паузы).
    pub fn on_sample(&mut self, ts_ms: u64, rms: f32) -> SilenceEvent {
        if self.paused {
            return SilenceEvent::None;
        }

        if rms >= self.cfg.floor_rms {
            let since = *self.voice_since_ms.get_or_insert(ts_ms);
            if ts_ms.saturating_sub(since) >= self.cfg.voice_hysteresis_ms {
                self.reset_run();
            }
            return SilenceEvent::None;
        }

        self.voice_since_ms = None;
        let started = *self.silence_started_ms.get_or_insert(ts_ms);
        if self.stopped {
            return SilenceEvent::None;
        }
        let silent_for_ms = ts_ms.saturating_sub(started);

        // Авто-стоп проверяется первым: поток RMS дырявый по устройству
        // (`try_send` дропает сэмпл при полном буфере), поэтому один сэмпл
        // может перешагнуть оба порога сразу. Стоп в этом случае важнее
        // подсказки, которую всё равно некому будет нажать.
        if let Some(after) = self.cfg.auto_stop_after_ms {
            if silent_for_ms >= after {
                self.stopped = true;
                let trim_at_ms = started.saturating_add(self.cfg.tail_pad_ms).min(ts_ms);
                return SilenceEvent::AutoStop {
                    trim_at_ms,
                    silent_for_ms,
                };
            }
        }

        if let Some(after) = self.cfg.suggest_after_ms {
            if !self.suggested && silent_for_ms >= after {
                self.suggested = true;
                return SilenceEvent::SuggestStop {
                    silent_for_ms,
                    auto_stop_in_ms: self
                        .cfg
                        .auto_stop_after_ms
                        .map(|a| a.saturating_sub(silent_for_ms)),
                };
            }
        }

        SilenceEvent::None
    }

    /// Запись встала на паузу — сэмплы игнорируем, тихий run сбрасываем.
    pub fn on_pause(&mut self) {
        self.paused = true;
        self.reset_run();
    }

    /// Запись возобновлена — считаем тишину с нуля.
    pub fn on_resume(&mut self) {
        self.paused = false;
        self.reset_run();
    }

    /// Юзер нажал «Продолжить» в подсказке. Решение владельца: это сбрасывает
    /// счётчик тишины целиком, то есть авто-стоп откладывается на полный
    /// интервал заново.
    pub fn snooze(&mut self) {
        self.reset_run();
    }

    fn reset_run(&mut self) {
        self.silence_started_ms = None;
        self.voice_since_ms = None;
        self.suggested = false;
        self.stopped = false;
    }
}

pub mod task;

pub use task::{run, SilenceControl, SilenceEventFut, SilenceWatchHandles};

#[cfg(test)]
mod tests {
    use super::*;

    /// Порог 1с/2с и гистерезис 200мс — читаемые числа вместо минут.
    fn cfg() -> SilenceWatchConfig {
        SilenceWatchConfig {
            floor_rms: 0.01,
            voice_hysteresis_ms: 200,
            suggest_after_ms: Some(1_000),
            auto_stop_after_ms: Some(2_000),
            tail_pad_ms: 100,
        }
    }

    const LOUD: f32 = 0.5;
    const QUIET: f32 = 0.001;

    /// Прогнать тихие сэмплы каждые 100мс в `[from, to]`, вернуть все
    /// не-`None` события с их таймстемпами.
    fn silence_run(w: &mut SilenceWatch, from: u64, to: u64) -> Vec<(u64, SilenceEvent)> {
        let mut out = Vec::new();
        let mut ts = from;
        while ts <= to {
            let ev = w.on_sample(ts, QUIET);
            if ev != SilenceEvent::None {
                out.push((ts, ev));
            }
            ts += 100;
        }
        out
    }

    #[test]
    fn suggests_once_per_silence_run() {
        let mut w = SilenceWatch::new(SilenceWatchConfig {
            auto_stop_after_ms: None,
            ..cfg()
        });
        let events = silence_run(&mut w, 0, 1_900);
        assert_eq!(
            events,
            vec![(
                1_000,
                SilenceEvent::SuggestStop {
                    silent_for_ms: 1_000,
                    auto_stop_in_ms: None,
                }
            )],
            "подсказка обязана прийти ровно один раз на run"
        );
    }

    #[test]
    fn suggest_carries_auto_stop_deadline() {
        let mut w = SilenceWatch::new(cfg());
        // Тишина началась на 500мс → дедлайн авто-стопа 500+2000.
        let events = silence_run(&mut w, 500, 1_600);
        assert_eq!(
            events,
            vec![(
                1_500,
                SilenceEvent::SuggestStop {
                    silent_for_ms: 1_000,
                    auto_stop_in_ms: Some(1_000),
                }
            )]
        );
    }

    #[test]
    fn auto_stops_and_trims_to_silence_start_plus_pad() {
        let mut w = SilenceWatch::new(cfg());
        let events = silence_run(&mut w, 1_000, 3_500);
        assert_eq!(
            events,
            vec![
                (
                    2_000,
                    SilenceEvent::SuggestStop {
                        silent_for_ms: 1_000,
                        auto_stop_in_ms: Some(1_000),
                    }
                ),
                // 1000 (начало тишины) + 100 (pad).
                (
                    3_000,
                    SilenceEvent::AutoStop {
                        trim_at_ms: 1_100,
                        silent_for_ms: 2_000,
                    }
                ),
            ]
        );
    }

    #[test]
    fn auto_stop_is_emitted_once_even_if_samples_keep_coming() {
        let mut w = SilenceWatch::new(cfg());
        let events = silence_run(&mut w, 0, 5_000);
        let stops = events
            .iter()
            .filter(|(_, e)| matches!(e, SilenceEvent::AutoStop { .. }))
            .count();
        assert_eq!(
            stops, 1,
            "между решением и стопом сайдкара сэмплы ещё летят"
        );
    }

    #[test]
    fn never_setting_never_auto_stops() {
        let mut w = SilenceWatch::new(SilenceWatchConfig {
            auto_stop_after_ms: None,
            ..cfg()
        });
        let events = silence_run(&mut w, 0, 60_000);
        assert!(
            events
                .iter()
                .all(|(_, e)| matches!(e, SilenceEvent::SuggestStop { .. })),
            "auto_stop_after_ms=None — только подсказка, {events:?}"
        );
    }

    #[test]
    fn prompt_off_still_auto_stops() {
        let mut w = SilenceWatch::new(SilenceWatchConfig {
            suggest_after_ms: None,
            ..cfg()
        });
        let events = silence_run(&mut w, 0, 2_500);
        assert_eq!(
            events,
            vec![(
                2_000,
                SilenceEvent::AutoStop {
                    trim_at_ms: 100,
                    silent_for_ms: 2_000,
                }
            )]
        );
    }

    #[test]
    fn single_blip_shorter_than_hysteresis_does_not_reset_run() {
        let mut w = SilenceWatch::new(cfg());
        // Тишина 0..500, один громкий сэмпл (100мс < 200мс гистерезиса), дальше тишина.
        assert_eq!(silence_run(&mut w, 0, 500), vec![]);
        assert_eq!(w.on_sample(600, LOUD), SilenceEvent::None);
        // Подсказка приходит на 1000 от НАЧАЛА тишины — щелчок run не сбросил.
        let events = silence_run(&mut w, 700, 1_100);
        assert_eq!(
            events,
            vec![(
                1_000,
                SilenceEvent::SuggestStop {
                    silent_for_ms: 1_000,
                    auto_stop_in_ms: Some(1_000),
                }
            )]
        );
    }

    #[test]
    fn sustained_voice_resets_run() {
        let mut w = SilenceWatch::new(cfg());
        assert_eq!(silence_run(&mut w, 0, 900), vec![]);
        // 200мс непрерывного звука = порог гистерезиса → run сброшен.
        assert_eq!(w.on_sample(1_000, LOUD), SilenceEvent::None);
        assert_eq!(w.on_sample(1_100, LOUD), SilenceEvent::None);
        assert_eq!(w.on_sample(1_200, LOUD), SilenceEvent::None);
        // Тишина считается заново с 1300 → подсказка на 2300, не на 1000.
        let events = silence_run(&mut w, 1_300, 2_400);
        assert_eq!(
            events,
            vec![(
                2_300,
                SilenceEvent::SuggestStop {
                    silent_for_ms: 1_000,
                    auto_stop_in_ms: Some(1_000),
                }
            )]
        );
    }

    #[test]
    fn pause_resets_run_and_ignores_samples() {
        let mut w = SilenceWatch::new(cfg());
        assert_eq!(silence_run(&mut w, 0, 900), vec![]);
        w.on_pause();
        // На паузе рекордеры дают честный ноль — это не тишина звонка.
        assert_eq!(silence_run(&mut w, 1_000, 10_000), vec![]);
        w.on_resume();
        let events = silence_run(&mut w, 10_100, 11_200);
        assert_eq!(
            events,
            vec![(
                11_100,
                SilenceEvent::SuggestStop {
                    silent_for_ms: 1_000,
                    auto_stop_in_ms: Some(1_000),
                }
            )],
            "после resume тишина считается с нуля"
        );
    }

    #[test]
    fn snooze_postpones_auto_stop_by_full_interval() {
        let mut w = SilenceWatch::new(cfg());
        let events = silence_run(&mut w, 0, 1_000);
        assert_eq!(events.len(), 1, "дошли до подсказки");
        w.snooze();
        // Полный интервал авто-стопа заново: тишина с 1100 → стоп на 3100,
        // а не на исходных 2000.
        let events = silence_run(&mut w, 1_100, 2_500);
        assert!(
            !events
                .iter()
                .any(|(_, e)| matches!(e, SilenceEvent::AutoStop { .. })),
            "snooze обязан отложить авто-стоп, {events:?}"
        );
        let events = silence_run(&mut w, 2_600, 3_200);
        assert_eq!(
            events,
            vec![(
                3_100,
                SilenceEvent::AutoStop {
                    trim_at_ms: 1_200,
                    silent_for_ms: 2_000,
                }
            )]
        );
    }

    #[test]
    fn trim_point_never_exceeds_recorded_length() {
        // Патологический конфиг: pad больше порога стопа.
        let mut w = SilenceWatch::new(SilenceWatchConfig {
            suggest_after_ms: None,
            auto_stop_after_ms: Some(300),
            tail_pad_ms: 10_000,
            ..cfg()
        });
        let events = silence_run(&mut w, 0, 500);
        assert_eq!(
            events,
            vec![(
                300,
                SilenceEvent::AutoStop {
                    trim_at_ms: 300,
                    silent_for_ms: 300,
                }
            )],
            "рез не может уйти за уже записанное"
        );
    }

    #[test]
    fn is_armed_false_when_both_settings_off() {
        let off = SilenceWatchConfig {
            suggest_after_ms: None,
            auto_stop_after_ms: None,
            ..cfg()
        };
        assert!(!SilenceWatch::is_armed(&off));
        assert!(SilenceWatch::is_armed(&cfg()));
    }

    #[test]
    fn floor_is_inclusive_upper_bound_for_silence() {
        let mut w = SilenceWatch::new(cfg());
        // Ровно на пороге — это звук, не тишина (>= floor).
        assert_eq!(w.on_sample(0, 0.01), SilenceEvent::None);
        let events = silence_run(&mut w, 100, 1_200);
        assert_eq!(
            events,
            vec![(
                1_100,
                SilenceEvent::SuggestStop {
                    silent_for_ms: 1_000,
                    auto_stop_in_ms: Some(1_000),
                }
            )]
        );
    }
}
