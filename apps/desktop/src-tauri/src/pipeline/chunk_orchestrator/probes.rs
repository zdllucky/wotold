//! [TD-48] Пробы для тестов оркестратора: счётчик вызовов с сигналом и
//! барьеры на встречном давлении каналов.
//!
//! Отдельным файлом, потому что тесты оркестратора и без них подошли к лимиту
//! 800 (правило 8), а пробы к самим сценариям отношения не имеют.

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{mpsc, watch};

/// [TD-48] Счётчик вызовов с сигналом. Тест ждёт «оркестратор дошёл до N-го
/// вызова», а не «прошло 50 мс»: на нагруженном раннере второе не гарантирует
/// первого, и именно так эти тесты флачили (правило 6).
#[derive(Clone)]
pub(super) struct CallProbe {
    tx: Arc<watch::Sender<usize>>,
    rx: watch::Receiver<usize>,
}

impl CallProbe {
    pub(super) fn new() -> Self {
        let (tx, rx) = watch::channel(0usize);
        Self {
            tx: Arc::new(tx),
            rx,
        }
    }

    pub(super) fn hit(&self) {
        self.tx.send_modify(|c| *c += 1);
    }

    pub(super) fn count(&self) -> usize {
        *self.rx.borrow()
    }

    /// Дождаться `n`-го вызова. Таймаут — страховка от зависания: без него
    /// сломанный оркестратор вешал бы прогон вместо внятного падения.
    pub(super) async fn wait_for(&self, n: usize) {
        let mut rx = self.rx.clone();
        let waited = tokio::time::timeout(Duration::from_secs(5), async {
            rx.wait_for(|&c| c >= n).await.map(|_| ())
        })
        .await;
        assert!(
            waited.is_ok(),
            "не дождались {n} вызовов за 5 с (было {})",
            self.count()
        );
    }

    /// Убедиться, что за `within` вызовов НЕ прибавилось. Для утверждений
    /// «ротации не случилось» сигнала не существует по определению —
    /// единственный честный способ это ограниченное ожидание.
    pub(super) async fn expect_none_within(&self, within: Duration) {
        let mut rx = self.rx.clone();
        let changed = tokio::time::timeout(within, rx.changed()).await;
        assert!(
            changed.is_err(),
            "ожидали тишину, а вызовов стало {}",
            self.count()
        );
    }
}

/// [TD-48] Отправить пачку RMS и дождаться, что оркестратор её **забрал**.
///
/// Канал ёмкости 1: возврат `send` означает, что предыдущий сэмпл уже принят,
/// поэтому дубль последнего и есть барьер. Без него порядок «RMS доехали →
/// пауза» держался только на `sleep(30)`, и на нагруженном раннере пауза
/// могла обогнать сэмплы — тогда момент начала паузы фиксировался по нулевому
/// таймкоду, накопленная пауза выходила втрое больше, и чанк не резался
/// вообще.
pub(super) async fn send_rms_settled(tx: &mpsc::Sender<(u64, f32)>, samples: &[(u64, f32)]) {
    for s in samples {
        tx.send(*s).await.unwrap();
    }
    if let Some(last) = samples.last() {
        tx.send(*last).await.unwrap();
    }
}

/// [TD-48] Отправить команду паузы и дождаться, что оркестратор её **обработал**.
///
/// Сигнала на это нет, зато есть встречное давление канала: при ёмкости 1
/// третий `send` проходит только после того, как приёмник забрал первый И
/// довертел тело своей ветки `select!` (цикл однопоточный). Команда паузы
/// идемпотентна по контракту — это отдельно проверяет
/// `pause_resume_idempotent_no_crash`, — поэтому повтор безвреден.
pub(super) async fn set_paused_and_settle(tx: &mpsc::Sender<bool>, paused: bool) {
    for _ in 0..3 {
        tx.send(paused).await.unwrap();
    }
}
