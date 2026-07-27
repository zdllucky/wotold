//! [TD-22] Темпоральный слой ассистента: разбор относительного периода из
//! вопроса и префильтр звонков по нему.
//!
//! Выделен из `router.rs` — тот перевалил за лимит когезии 800 строк, и
//! добавить в него guard'ы против ложных перехватов стало нельзя (инженерное
//! правило 8). Граница естественная: здесь только время, в роутере — решение
//! о маршруте.

use sqlx::SqlitePool;

use crate::AppError;

use super::router::{has, has_any, words};

/// [B26.1] Относительный период из слов вопроса → полуинтервал
/// `[since, until)` в UTC RFC3339. Границы дней считаются в ЛОКАЛЬНОМ поясе
/// пользователя (`now: DateTime<Local>` инъектится ради тестов), потом
/// конвертируются в UTC — `calls.started_at` хранится в UTC. None-граница =
/// открытый край. None целиком — период в вопросе не распознан.
pub(crate) fn period_range(
    ws: &[String],
    now: chrono::DateTime<chrono::Local>,
) -> Option<(Option<String>, Option<String>)> {
    use chrono::{Datelike, Duration, Local, TimeZone, Utc};

    let local_ymd = |y: i32, m: u32, d: u32| -> Option<chrono::DateTime<Local>> {
        Local.with_ymd_and_hms(y, m, d, 0, 0, 0).earliest()
    };
    let to_utc = |d: chrono::DateTime<Local>| {
        d.with_timezone(&Utc)
            .to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
    };
    let today0 = local_ymd(now.year(), now.month(), now.day())?;
    // Первое число месяца со сдвигом на delta месяцев (любой знак).
    let month_first = |y: i32, m: u32, delta: i32| -> Option<chrono::DateTime<Local>> {
        let total = (y * 12 + (m as i32 - 1)) + delta;
        local_ymd(total.div_euclid(12), (total.rem_euclid(12) + 1) as u32, 1)
    };

    // Маркер «прошлый/прошлой/прошлом…» — переключает на календарный период.
    let last = has_any(
        ws,
        &["прошлой", "прошлую", "прошлом", "прошлый", "прошлого"],
    );
    let ago = has(ws, "назад");

    if has(ws, "сегодня") {
        return Some((Some(to_utc(today0)), None));
    }
    if has(ws, "вчера") {
        return Some((
            Some(to_utc(today0 - Duration::days(1))),
            Some(to_utc(today0)),
        ));
    }
    if has(ws, "позавчера") {
        return Some((
            Some(to_utc(today0 - Duration::days(2))),
            Some(to_utc(today0 - Duration::days(1))),
        ));
    }
    if has_any(ws, &["неделю", "неделе", "недели", "неделя"]) {
        let monday = today0 - Duration::days(i64::from(now.weekday().num_days_from_monday()));
        if last || ago {
            return Some((
                Some(to_utc(monday - Duration::days(7))),
                Some(to_utc(monday)),
            ));
        }
        // «за неделю» — последние 7 дней (нижняя граница, как раньше).
        return Some((Some(to_utc(today0 - Duration::days(7))), None));
    }
    if has_any(ws, &["месяц", "месяца", "месяце"]) {
        let first_this = month_first(now.year(), now.month(), 0)?;
        if last || ago {
            // «в прошлом месяце» / «месяц назад» — календарный прошлый месяц.
            let first_prev = month_first(now.year(), now.month(), -1)?;
            return Some((Some(to_utc(first_prev)), Some(to_utc(first_this))));
        }
        return Some((Some(to_utc(today0 - Duration::days(31))), None));
    }
    if has_any(ws, &["году", "года", "год"]) {
        let jan1_this = local_ymd(now.year(), 1, 1)?;
        if last {
            let jan1_prev = local_ymd(now.year() - 1, 1, 1)?;
            return Some((Some(to_utc(jan1_prev)), Some(to_utc(jan1_this))));
        }
        if has_any(ws, &["этом", "этот", "текущем"]) {
            return Some((Some(to_utc(jan1_this)), None));
        }
        if ago {
            // «год назад» — неоднозначно, берём последние 365 дней.
            return Some((Some(to_utc(today0 - Duration::days(365))), None));
        }
        return None;
    }
    // Месяцы по имени: «в июне / июня». Будущий месяц → прошлый год.
    const MONTHS: &[(&[&str], u32)] = &[
        (&["январе", "января"], 1),
        (&["феврале", "февраля"], 2),
        (&["марте", "марта"], 3),
        (&["апреле", "апреля"], 4),
        (&["мае", "мая"], 5),
        (&["июне", "июня"], 6),
        (&["июле", "июля"], 7),
        (&["августе", "августа"], 8),
        (&["сентябре", "сентября"], 9),
        (&["октябре", "октября"], 10),
        (&["ноябре", "ноября"], 11),
        (&["декабре", "декабря"], 12),
    ];
    for (forms, m) in MONTHS {
        if has_any(ws, forms) {
            let year = if *m > now.month() {
                now.year() - 1
            } else {
                now.year()
            };
            let first = month_first(year, *m, 0)?;
            let next = month_first(year, *m, 1)?;
            return Some((Some(to_utc(first)), Some(to_utc(next))));
        }
    }
    if has_any(ws, &["квартал", "квартала", "квартале"]) {
        if last {
            // Календарный прошлый квартал.
            let q_first_month = ((now.month() - 1) / 3) * 3 + 1;
            let this_q = month_first(now.year(), q_first_month, 0)?;
            let prev_q = month_first(now.year(), q_first_month, -3)?;
            return Some((Some(to_utc(prev_q)), Some(to_utc(this_q))));
        }
        return Some((Some(to_utc(today0 - Duration::days(92))), None));
    }
    None
}

/// [B26.2] Темпоральный префильтр для ОБЫЧНЫХ вопросов: явный период в
/// вопросе → набор `call_id` за период. `Ok(None)` — периода нет.
/// `Ok(Some(пустой))` — период есть, но звонков нет: вызывающий отвечает
/// честным «за этот период не найдено», не гоняя поиск.
pub(crate) async fn period_call_filter(
    pool: &SqlitePool,
    question: &str,
) -> Result<Option<std::collections::HashSet<String>>, AppError> {
    let ws = words(question);
    let Some((since, until)) = period_range(&ws, chrono::Local::now()) else {
        return Ok(None);
    };
    let rows: Vec<(String,)> = sqlx::query_as(
        "SELECT id FROM calls
         WHERE status = 'ready' AND started_at >= ?1 AND started_at < ?2",
    )
    .bind(since.as_deref().unwrap_or("0000-01-01T00:00:00Z"))
    .bind(until.as_deref().unwrap_or("9999-12-31T00:00:00Z"))
    .fetch_all(pool)
    .await?;
    Ok(Some(rows.into_iter().map(|(id,)| id).collect()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::test_support::fresh_db;

    async fn seed_call(pool: &SqlitePool, id: &str, title: &str, started_at: &str) {
        sqlx::query(
            "INSERT INTO calls (id, title, started_at, duration_sec, status, path_label, created_at, updated_at)
             VALUES (?1, ?2, ?3, 600, 'ready', 'managed', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)",
        )
        .bind(id)
        .bind(title)
        .bind(started_at)
        .execute(pool)
        .await
        .unwrap();
    }

    // ── [B26.1] period_range: полуинтервалы в локальном поясе ──

    fn fixed_now(y: i32, m: u32, d: u32) -> chrono::DateTime<chrono::Local> {
        use chrono::TimeZone;
        chrono::Local
            .with_ymd_and_hms(y, m, d, 15, 30, 0)
            .single()
            .unwrap()
    }

    fn local_utc(y: i32, m: u32, d: u32) -> String {
        use chrono::TimeZone;
        chrono::Local
            .with_ymd_and_hms(y, m, d, 0, 0, 0)
            .single()
            .unwrap()
            .with_timezone(&chrono::Utc)
            .to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
    }

    fn range_of(
        q: &str,
        now: chrono::DateTime<chrono::Local>,
    ) -> Option<(Option<String>, Option<String>)> {
        period_range(&words(q), now)
    }

    #[test]
    fn period_range_days_and_weeks() {
        // 2026-07-23 — четверг.
        let now = fixed_now(2026, 7, 23);
        assert_eq!(
            range_of("что обсуждали сегодня", now),
            Some((Some(local_utc(2026, 7, 23)), None))
        );
        assert_eq!(
            range_of("что обсуждали вчера", now),
            Some((Some(local_utc(2026, 7, 22)), Some(local_utc(2026, 7, 23))))
        );
        // «на прошлой неделе»: пн этой = 20.07 → [13.07, 20.07).
        assert_eq!(
            range_of("звонки на прошлой неделе", now),
            Some((Some(local_utc(2026, 7, 13)), Some(local_utc(2026, 7, 20))))
        );
        // «за неделю» — просто последние 7 дней.
        assert_eq!(
            range_of("звонки за неделю", now),
            Some((Some(local_utc(2026, 7, 16)), None))
        );
    }

    #[test]
    fn period_range_months_and_years() {
        let now = fixed_now(2026, 7, 23);
        // «в прошлом месяце» — календарный июнь.
        assert_eq!(
            range_of("что было в прошлом месяце", now),
            Some((Some(local_utc(2026, 6, 1)), Some(local_utc(2026, 7, 1))))
        );
        // «месяц назад» — тот же календарный прошлый.
        assert_eq!(
            range_of("звонки месяц назад", now),
            Some((Some(local_utc(2026, 6, 1)), Some(local_utc(2026, 7, 1))))
        );
        // «в прошлом году» — календарный 2025.
        assert_eq!(
            range_of("что обсуждали в прошлом году", now),
            Some((Some(local_utc(2025, 1, 1)), Some(local_utc(2026, 1, 1))))
        );
        // Именованный месяц: прошедший в этом году.
        assert_eq!(
            range_of("что было в июне", now),
            Some((Some(local_utc(2026, 6, 1)), Some(local_utc(2026, 7, 1))))
        );
        // Текущий месяц.
        assert_eq!(
            range_of("что было в июле", now),
            Some((Some(local_utc(2026, 7, 1)), Some(local_utc(2026, 8, 1))))
        );
        // Будущий месяц → прошлый год.
        assert_eq!(
            range_of("что было в августе", now),
            Some((Some(local_utc(2025, 8, 1)), Some(local_utc(2025, 9, 1))))
        );
    }

    // [B26.2] Префильтр: набор call_id за период из вопроса.
    #[tokio::test]
    async fn period_call_filter_selects_calls_in_range() {
        let db = fresh_db().await;
        let yesterday = (chrono::Local::now() - chrono::Duration::days(1))
            .with_timezone(&chrono::Utc)
            .to_rfc3339();
        seed_call(&db.pool, "c-old", "Старый", "2020-01-01T09:00:00+00:00").await;
        seed_call(&db.pool, "c-yest", "Вчерашний", &yesterday).await;

        let set = period_call_filter(&db.pool, "что обсуждали вчера про бюджет")
            .await
            .unwrap()
            .expect("период распознан");
        assert!(set.contains("c-yest"));
        assert!(!set.contains("c-old"));

        // Без периода — None (фильтр не применяется).
        assert!(period_call_filter(&db.pool, "что решили по командам")
            .await
            .unwrap()
            .is_none());

        // Период есть, звонков нет → Some(пустой) — честный empty у вызывающего.
        let empty = period_call_filter(&db.pool, "что было в прошлом месяце")
            .await
            .unwrap()
            .expect("период распознан");
        assert!(empty.is_empty());
    }

    #[test]
    fn period_range_january_rollover_and_negatives() {
        // 1 января: «в прошлом месяце» = декабрь прошлого года.
        let now = fixed_now(2026, 1, 1);
        assert_eq!(
            range_of("что было в прошлом месяце", now),
            Some((Some(local_utc(2025, 12, 1)), Some(local_utc(2026, 1, 1))))
        );
        // Негативы: словоформы не из списка периодов.
        let now = fixed_now(2026, 7, 23);
        assert_eq!(range_of("месячный отчёт по проекту", now), None);
        assert_eq!(range_of("что решили по командам", now), None);
        assert_eq!(range_of("сколько звонков записано", now), None);
    }
}
