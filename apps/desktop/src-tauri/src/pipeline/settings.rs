//! [Phase 2 R3] Typed pipeline settings, loaded once per run.
//!
//! Раньше `pipeline/mod.rs` читал 6-8 настроек inline через `read_setting()`
//! на каждый run + повторно в `regenerate_recap` — `preferred_language`
//! читался дважды на один pipeline. SETTING_* константы были разбросаны,
//! магические строки (`auto_bind_enabled = '1'`) ломались при опечатках.
//!
//! Этот модуль:
//! - Группирует все pipeline-related ключи в одном месте.
//! - Один `PipelineSettings::load(pool)` → typed struct.
//! - Производные значения (effective recap lang, model override) — методы
//!   на struct, не растащены по callsite'ам.
//! - Edge cases (malformed threshold, empty proxy url, "auto" lang) тестируются
//!   в изоляции — раньше они растворялись в integration через `run_inner`.

use sqlx::SqlitePool;

use crate::{db, AppError};

#[cfg(target_os = "macos")]
use crate::local_engine::engine::{self, EngineKind};

// === Setting keys (private — наружу только typed PipelineSettings) ===

const SETTING_STT_PROVIDER: &str = "stt_provider";
const SETTING_PROVIDER_PATH: &str = "provider_path";
const SETTING_STT_LANG: &str = "stt_lang";
const SETTING_LLM_MODEL: &str = "llm_model";
const SETTING_PROXY_BASE_URL: &str = "proxy_base_url";
/// [B13] BCP47 язык override для LLM-output. 'auto' = язык STT detection.
const SETTING_PREFERRED_LANGUAGE: &str = "preferred_language";
/// [V7] Opt-in auto-bind speakers. '1' = enabled.
const SETTING_AUTO_BIND_ENABLED: &str = "auto_bind_enabled";
const SETTING_AUTO_BIND_THRESHOLD: &str = "auto_bind_threshold";

// === Default proxy URL ===

#[cfg(debug_assertions)]
pub const DEFAULT_PROXY_BASE_URL: &str = "https://wotold-proxy-staging.animereader.workers.dev";
#[cfg(not(debug_assertions))]
pub const DEFAULT_PROXY_BASE_URL: &str = "https://wotold-proxy.animereader.workers.dev";

/// [V7] Допустимый диапазон auto-bind threshold (UI ограничивает 90/95/98).
/// При мусорных значениях из БД clamp'имся внутрь — защита от ручных правок.
const AUTO_BIND_THRESHOLD_MIN: f64 = 0.80;
const AUTO_BIND_THRESHOLD_MAX: f64 = 1.00;
const AUTO_BIND_THRESHOLD_DEFAULT_PCT: f64 = 95.0;

/// [V7] Конфигурация opt-in auto-bind. `None` когда выключено в Settings.
#[derive(Debug, Clone, PartialEq)]
pub struct AutoBindConfig {
    /// Cosine similarity threshold (0.80..1.00). Сравнивается с
    /// `call_speakers.suggestion_score`.
    pub threshold: f64,
}

#[derive(Debug, Clone)]
pub struct PipelineSettings {
    pub stt_provider: String,
    pub provider_path: String,
    pub stt_lang: String,
    pub llm_model: String,
    pub proxy_base_url: String,
    pub preferred_language: String,
    pub auto_bind: Option<AutoBindConfig>,
    /// [M12.6] Активный engine — резюмирует выбор пользователя в Settings →
    /// «Движок распознавания». На macOS читается из `local_engine.active`
    /// (наследие из миграции 0011 + Settings UI M12.5). На не-macOS — всегда
    /// `cloud_managed` (R9 — local движок недоступен).
    #[cfg(target_os = "macos")]
    pub engine: EngineKind,
}

impl PipelineSettings {
    /// Прочитать все pipeline-related настройки одним проходом.
    /// Пустые/отсутствующие значения подставляются дефолтами.
    pub async fn load(pool: &SqlitePool) -> Result<Self, AppError> {
        let stt_provider = read_setting(pool, SETTING_STT_PROVIDER, "auto").await?;
        let provider_path = read_setting(pool, SETTING_PROVIDER_PATH, "managed").await?;
        let stt_lang = read_setting(pool, SETTING_STT_LANG, "auto").await?;
        let llm_model = read_setting(pool, SETTING_LLM_MODEL, "").await?;
        let proxy_base_url = db::get_setting(pool, SETTING_PROXY_BASE_URL)
            .await?
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| DEFAULT_PROXY_BASE_URL.to_string());
        let preferred_language = read_setting(pool, SETTING_PREFERRED_LANGUAGE, "auto").await?;

        // Auto-bind: enabled только при явном "1". Threshold parse'ится с
        // fallback на 95 и clamp в whitelist'нутый диапазон.
        let auto_bind = if read_setting(pool, SETTING_AUTO_BIND_ENABLED, "").await? == "1" {
            let raw = read_setting(
                pool,
                SETTING_AUTO_BIND_THRESHOLD,
                &AUTO_BIND_THRESHOLD_DEFAULT_PCT.to_string(),
            )
            .await?;
            let pct: f64 = raw
                .parse()
                .unwrap_or(AUTO_BIND_THRESHOLD_DEFAULT_PCT)
                .clamp(
                    AUTO_BIND_THRESHOLD_MIN * 100.0,
                    AUTO_BIND_THRESHOLD_MAX * 100.0,
                );
            Some(AutoBindConfig {
                threshold: pct / 100.0,
            })
        } else {
            None
        };

        #[cfg(target_os = "macos")]
        let engine = engine::load_or_default(pool).await?;

        Ok(Self {
            stt_provider,
            provider_path,
            stt_lang,
            llm_model,
            proxy_base_url,
            preferred_language,
            auto_bind,
            #[cfg(target_os = "macos")]
            engine,
        })
    }

    /// LLM-output язык. Если `preferred_language='auto'|''`, fallback на
    /// detected STT lang; иначе override (например 'ru' даже для en transcript).
    pub fn effective_recap_lang(&self, lang_detected: Option<&str>) -> Option<String> {
        if self.preferred_language == "auto" || self.preferred_language.is_empty() {
            lang_detected.map(|s| s.to_string())
        } else {
            Some(self.preferred_language.clone())
        }
    }

    /// LLM model override. Пустая строка → None (прокси сам выберет по
    /// LLM_BACKEND); иначе конкретная модель.
    pub fn model_override(&self) -> Option<&str> {
        if self.llm_model.is_empty() {
            None
        } else {
            Some(self.llm_model.as_str())
        }
    }
}

async fn read_setting(
    pool: &SqlitePool,
    key: &str,
    default_value: &str,
) -> Result<String, AppError> {
    Ok(db::get_setting(pool, key)
        .await?
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| default_value.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::test_support::fresh_db;

    #[tokio::test]
    async fn load_uses_defaults_for_unset_keys() {
        let db = fresh_db().await;
        let s = PipelineSettings::load(&db.pool).await.unwrap();
        assert_eq!(s.stt_provider, "auto");
        assert_eq!(s.provider_path, "managed");
        assert_eq!(s.stt_lang, "auto");
        assert_eq!(s.llm_model, "");
        assert_eq!(s.preferred_language, "auto");
        assert_eq!(s.proxy_base_url, DEFAULT_PROXY_BASE_URL);
        assert!(s.auto_bind.is_none(), "auto_bind off by default (R2)");
    }

    #[tokio::test]
    async fn load_respects_explicit_overrides() {
        let db = fresh_db().await;
        db::set_setting(&db.pool, SETTING_STT_PROVIDER, "soniox")
            .await
            .unwrap();
        db::set_setting(&db.pool, SETTING_PROVIDER_PATH, "byo")
            .await
            .unwrap();
        db::set_setting(&db.pool, SETTING_PREFERRED_LANGUAGE, "ru")
            .await
            .unwrap();
        db::set_setting(&db.pool, SETTING_LLM_MODEL, "claude-3-5-sonnet")
            .await
            .unwrap();
        db::set_setting(&db.pool, SETTING_PROXY_BASE_URL, "https://example.com")
            .await
            .unwrap();

        let s = PipelineSettings::load(&db.pool).await.unwrap();
        assert_eq!(s.stt_provider, "soniox");
        assert_eq!(s.provider_path, "byo");
        assert_eq!(s.preferred_language, "ru");
        assert_eq!(s.model_override(), Some("claude-3-5-sonnet"));
        assert_eq!(s.proxy_base_url, "https://example.com");
    }

    #[tokio::test]
    async fn empty_proxy_url_falls_back_to_default() {
        let db = fresh_db().await;
        db::set_setting(&db.pool, SETTING_PROXY_BASE_URL, "   ")
            .await
            .unwrap();
        let s = PipelineSettings::load(&db.pool).await.unwrap();
        assert_eq!(s.proxy_base_url, DEFAULT_PROXY_BASE_URL);
    }

    #[tokio::test]
    async fn auto_bind_enabled_picks_threshold_from_setting() {
        let db = fresh_db().await;
        db::set_setting(&db.pool, SETTING_AUTO_BIND_ENABLED, "1")
            .await
            .unwrap();
        db::set_setting(&db.pool, SETTING_AUTO_BIND_THRESHOLD, "98")
            .await
            .unwrap();
        let s = PipelineSettings::load(&db.pool).await.unwrap();
        let cfg = s.auto_bind.expect("auto_bind enabled");
        assert!((cfg.threshold - 0.98).abs() < 1e-6);
    }

    #[tokio::test]
    async fn auto_bind_malformed_threshold_falls_back_to_95() {
        let db = fresh_db().await;
        db::set_setting(&db.pool, SETTING_AUTO_BIND_ENABLED, "1")
            .await
            .unwrap();
        db::set_setting(&db.pool, SETTING_AUTO_BIND_THRESHOLD, "not-a-number")
            .await
            .unwrap();
        let s = PipelineSettings::load(&db.pool).await.unwrap();
        let cfg = s.auto_bind.expect("enabled despite bad threshold");
        assert!((cfg.threshold - 0.95).abs() < 1e-6);
    }

    #[tokio::test]
    async fn auto_bind_threshold_clamps_to_safe_range() {
        let db = fresh_db().await;
        db::set_setting(&db.pool, SETTING_AUTO_BIND_ENABLED, "1")
            .await
            .unwrap();
        // Юзер вручную выставил 50 в DB (минуя UI whitelist 90/95/98).
        // Должно зажаться к min=80.
        db::set_setting(&db.pool, SETTING_AUTO_BIND_THRESHOLD, "50")
            .await
            .unwrap();
        let s = PipelineSettings::load(&db.pool).await.unwrap();
        assert!((s.auto_bind.unwrap().threshold - 0.80).abs() < 1e-6);

        db::set_setting(&db.pool, SETTING_AUTO_BIND_THRESHOLD, "150")
            .await
            .unwrap();
        let s2 = PipelineSettings::load(&db.pool).await.unwrap();
        assert!((s2.auto_bind.unwrap().threshold - 1.00).abs() < 1e-6);
    }

    #[tokio::test]
    async fn auto_bind_off_when_setting_empty_or_zero() {
        let db = fresh_db().await;
        for v in ["", "0", "false", "yes"] {
            db::set_setting(&db.pool, SETTING_AUTO_BIND_ENABLED, v)
                .await
                .unwrap();
            let s = PipelineSettings::load(&db.pool).await.unwrap();
            assert!(
                s.auto_bind.is_none(),
                "auto_bind must be None for value '{v}'"
            );
        }
    }

    #[test]
    fn effective_recap_lang_uses_detected_when_auto() {
        let s = settings_with_preferred("auto");
        assert_eq!(s.effective_recap_lang(Some("en")), Some("en".into()));
        assert_eq!(s.effective_recap_lang(None), None);
    }

    #[test]
    fn effective_recap_lang_uses_detected_when_empty() {
        let s = settings_with_preferred("");
        assert_eq!(s.effective_recap_lang(Some("kk")), Some("kk".into()));
    }

    #[test]
    fn effective_recap_lang_overrides_when_explicit() {
        let s = settings_with_preferred("ru");
        // override побеждает даже если STT detect'нул en
        assert_eq!(s.effective_recap_lang(Some("en")), Some("ru".into()));
        assert_eq!(s.effective_recap_lang(None), Some("ru".into()));
    }

    #[test]
    fn model_override_empty_returns_none() {
        let mut s = settings_with_preferred("auto");
        s.llm_model = String::new();
        assert_eq!(s.model_override(), None);
        s.llm_model = "claude-3-5-sonnet".into();
        assert_eq!(s.model_override(), Some("claude-3-5-sonnet"));
    }

    fn settings_with_preferred(lang: &str) -> PipelineSettings {
        PipelineSettings {
            stt_provider: "auto".into(),
            provider_path: "managed".into(),
            stt_lang: "auto".into(),
            llm_model: String::new(),
            proxy_base_url: DEFAULT_PROXY_BASE_URL.into(),
            preferred_language: lang.into(),
            auto_bind: None,
            #[cfg(target_os = "macos")]
            engine: EngineKind::CloudManaged,
        }
    }
}
