//! [TD-22] Прямые ответы роутера: карточка контакта, счётчики, последний
//! звонок, список звонков.
//!
//! Выделены из `router.rs` — тот упёрся в лимит когезии 800 строк, а фикс
//! требовал и правок в решении о маршруте, и тестов на них (правило 8).
//! Граница естественная: здесь построение ответа, там — решение, какой
//! ответ строить.

use sqlx::SqlitePool;

use crate::assistant::types::AssistantSource;
use crate::AppError;

use super::router::RoutedAnswer;

/// [B26.5a] Карточка(и) контакта по имени из вопроса (до 3 совпадений).
pub(crate) async fn who_is(
    pool: &SqlitePool,
    ws: &[String],
) -> Result<Option<RoutedAnswer>, AppError> {
    use crate::assistant::contacts_ctx;

    const WHO_STRIP: &[&str] = &["кто", "такой", "такая", "такое", "это", "вообще"];
    let name_part = ws
        .iter()
        .filter(|w| !WHO_STRIP.contains(&w.as_str()))
        .cloned()
        .collect::<Vec<_>>()
        .join(" ");
    if name_part.is_empty() {
        return Ok(None);
    }
    let contacts = contacts_ctx::list_contact_briefs(pool).await?;
    let matched = contacts_ctx::match_contacts(&contacts, &name_part);
    if matched.is_empty() {
        return Ok(None);
    }
    let mut lines = Vec::new();
    let mut sources = Vec::new();
    for c in matched.iter().take(3) {
        let stats = contacts_ctx::contact_call_stats(pool, &c.id).await?;
        lines.push(contacts_ctx::contact_card_text(c, &stats));
        if let Some((id, title, _)) = &stats.last_call {
            sources.push(AssistantSource {
                call_id: id.clone(),
                call_title: title.clone(),
                start_ms: None,
            });
        }
    }
    Ok(Some(RoutedAnswer::Direct {
        text: lines.join("\n"),
        sources,
    }))
}

/// «Записано N звонков (M в поиске), суммарно X ч Y мин» — из index_stats.
pub(crate) async fn stats_answer(pool: &SqlitePool) -> Result<RoutedAnswer, AppError> {
    let stats = crate::db::assistant::index_stats(pool).await?;
    let total_min = stats.total_duration_sec / 60;
    let dur = if total_min >= 60 {
        format!("{} ч {} мин", total_min / 60, total_min % 60)
    } else {
        format!("{total_min} мин")
    };
    let text = format!(
        "Записано {} звонков, {} из них в поиске ассистента. Суммарная длительность — {dur}.",
        stats.total_calls, stats.indexed_calls
    );
    Ok(RoutedAnswer::Direct {
        text,
        sources: vec![],
    })
}

/// Последний ready-звонок: (id, титул, «ДД.ММ.ГГГГ»).
pub(crate) async fn last_ready_call(
    pool: &SqlitePool,
) -> Result<Option<(String, String, String)>, AppError> {
    let row: Option<(String, Option<String>, String)> = sqlx::query_as(
        "SELECT id, title, started_at FROM calls WHERE status = 'ready'
         ORDER BY started_at DESC LIMIT 1",
    )
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|(id, title, started_at)| {
        let date = fmt_date(&started_at).unwrap_or_else(|| started_at.clone());
        let title = title
            .filter(|t| !t.trim().is_empty())
            .unwrap_or_else(|| "Без названия".to_string());
        (id, title, date)
    }))
}

/// «2026-07-21T…» → «21.07.2026».
pub(crate) fn fmt_date(started_at: &str) -> Option<String> {
    let d = started_at.get(..10)?;
    let mut it = d.split('-');
    let (y, m, day) = (it.next()?, it.next()?, it.next()?);
    if y.len() != 4 || m.len() != 2 || day.len() != 2 {
        return None;
    }
    Some(format!("{day}.{m}.{y}"))
}

/// Список звонков (опц. за период), свежие сверху, максимум 10.
/// [B26.1] Период — полуинтервал `period_range` (обе границы опциональны,
/// открытый край — сентинел, лексикографика RFC3339 это позволяет).
pub(crate) async fn list_calls(pool: &SqlitePool, ws: &[String]) -> Result<RoutedAnswer, AppError> {
    let range = crate::assistant::period::period_range(ws, chrono::Local::now());
    let rows: Vec<(String, Option<String>, String)> = match &range {
        Some((since, until)) => {
            sqlx::query_as(
                "SELECT id, title, started_at FROM calls
                 WHERE status = 'ready' AND started_at >= ?1 AND started_at < ?2
                 ORDER BY started_at DESC LIMIT 10",
            )
            .bind(since.as_deref().unwrap_or("0000-01-01T00:00:00Z"))
            .bind(until.as_deref().unwrap_or("9999-12-31T00:00:00Z"))
            .fetch_all(pool)
            .await?
        }
        None => {
            sqlx::query_as(
                "SELECT id, title, started_at FROM calls
                 WHERE status = 'ready' ORDER BY started_at DESC LIMIT 10",
            )
            .fetch_all(pool)
            .await?
        }
    };
    if rows.is_empty() {
        return Ok(RoutedAnswer::Direct {
            text: if range.is_some() {
                "За этот период записанных звонков нет.".to_string()
            } else {
                "Записанных звонков пока нет.".to_string()
            },
            sources: vec![],
        });
    }
    let mut lines = Vec::new();
    let mut sources = Vec::new();
    for (id, title, started_at) in rows {
        let title = title
            .filter(|t| !t.trim().is_empty())
            .unwrap_or_else(|| "Без названия".into());
        let date = fmt_date(&started_at).unwrap_or(started_at);
        lines.push(format!("— «{title}» · {date}"));
        sources.push(AssistantSource {
            call_id: id,
            call_title: title,
            start_ms: None,
        });
    }
    Ok(RoutedAnswer::Direct {
        text: lines.join("\n"),
        sources,
    })
}
