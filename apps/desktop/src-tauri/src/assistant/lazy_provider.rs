//! [TD-23] Ленивый локальный LLM-провайдер для ассистента.
//!
//! Прод-обёртка `ask` строила провайдер безусловно и ДО входа в
//! `ask_core_with`, который только потом зовёт роутер. При невыбранном
//! пресете это ломало ровно то, ради чего роутер M16.4 и делался: мета-вопрос
//! («сколько звонков», «когда был последний»), отказ и пустая ветка отвечаются
//! БЕЗ модели, но пользователь вместо ответа получал «модель не установлена».
//!
//! Здесь провайдер строится при первом обращении к `generate`. Пути, которые
//! до модели не доходят, её и не требуют.

use std::path::PathBuf;

use async_trait::async_trait;
use sqlx::SqlitePool;
use tauri::AppHandle;
use tokio::sync::OnceCell;

use crate::providers::llm::{LlmError, LlmProvider, LlmRequest};

pub struct LazyLocalProvider {
    pool: SqlitePool,
    app: AppHandle,
    app_data_dir: PathBuf,
    queue_label: String,
    inner: OnceCell<crate::local_engine::llm::LocalLlamaProvider>,
}

impl LazyLocalProvider {
    pub fn new(
        pool: SqlitePool,
        app: AppHandle,
        app_data_dir: PathBuf,
        queue_label: String,
    ) -> Self {
        Self {
            pool,
            app,
            app_data_dir,
            queue_label,
            inner: OnceCell::new(),
        }
    }

    /// Построить провайдер один раз. Ошибка не кэшируется намеренно: причина
    /// («пресет не выбран», «модель не скачана») устранима из настроек, и
    /// следующий вопрос должен работать без перезапуска приложения.
    async fn provider(
        &self,
    ) -> Result<&crate::local_engine::llm::LocalLlamaProvider, crate::AppError> {
        self.inner
            .get_or_try_init(|| async {
                let s = crate::pipeline::PipelineSettings::load(&self.pool).await?;
                let (provider, _preset) = crate::pipeline::build_local_llm_provider(
                    &self.pool,
                    &self.app_data_dir,
                    &self.app,
                    &s,
                )
                .await?;
                // cache_prompt: стабильный префикс [system][fragments]
                // переживает follow-up-ходы на resident-сервере (PRD §6.4).
                Ok(provider
                    .with_call(self.queue_label.clone())
                    .with_cache_prompt(true))
            })
            .await
    }
}

#[async_trait]
impl LlmProvider for LazyLocalProvider {
    async fn generate(&self, request: LlmRequest) -> Result<serde_json::Value, LlmError> {
        let provider = self
            .provider()
            .await
            .map_err(|e| LlmError::Provider(e.to_string()))?;
        provider.generate(request).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assistant::{ask_core_with, AskArgs};
    use crate::db::test_support::fresh_db;
    use crate::events::EventBus;

    /// Провайдер, который взрывается при любом обращении. Стоит вместо
    /// «модель не установлена»: если путь дошёл до модели — тест это увидит.
    struct ExplodingProvider;

    #[async_trait]
    impl LlmProvider for ExplodingProvider {
        async fn generate(&self, _r: LlmRequest) -> Result<serde_json::Value, LlmError> {
            Err(LlmError::Provider(
                "модель не должна была понадобиться".into(),
            ))
        }
    }

    #[tokio::test]
    async fn meta_question_is_answered_without_touching_the_model() {
        // Регрессия TD-23: прод-обёртка строила провайдер ДО роутера, поэтому
        // при невыбранном пресете мета-вопрос падал «модель не установлена» —
        // ровно то, ради чего роутер M16.4 и делался.
        //
        // Здесь проверяется само свойство: путь мета-вопроса не обращается к
        // модели вообще. Ленивость обёртки — следствие; без неё построение
        // падало бы раньше, чем этот путь начинался.
        let db = fresh_db().await;
        let bus = EventBus::new(None);
        let out = ask_core_with(
            &ExplodingProvider,
            &db.pool,
            &bus,
            AskArgs {
                chat_id: None,
                call_id: None,
                question: "сколько звонков записано".into(),
            },
            None,
        )
        .await
        .expect("мета-вопрос обязан отвечаться без модели");
        assert!(
            !out.message.text.is_empty(),
            "роутер обязан дать прямой ответ"
        );
    }

    #[tokio::test]
    async fn content_question_does_reach_the_model() {
        // Вторая половина: если бы «без модели» отвечалось вообще всё, тест
        // выше зеленел бы и на сломанном роутере.
        let db = fresh_db().await;
        let bus = EventBus::new(None);
        let out = ask_core_with(
            &ExplodingProvider,
            &db.pool,
            &bus,
            AskArgs {
                chat_id: None,
                call_id: None,
                question: "что решили по бюджету на следующий квартал".into(),
            },
            None,
        )
        .await;
        // Пустой архив честно отвечает «не нашлось» ещё до модели — это
        // легальная ветка. Важно, что путь НЕ выдаёт прямой ответ роутера.
        if let Ok(o) = out {
            let kind = o.message.answer.as_ref().map(|a| a.kind);
            assert_eq!(
                kind,
                Some(crate::assistant::types::AssistantAnswerKind::Empty),
                "контентный вопрос на пустом архиве — пустая ветка, а не прямой ответ"
            );
        }
    }
}
