//! [M14 follow-up] JSON Schemas для schema-constrained local LLM генерации.
//!
//! ## Зачем
//!
//! Generic `json.gbnf` ([`super::gbnf::UNIVERSAL_JSON_OBJECT_GRAMMAR`]) форсит
//! только валидный JSON-объект, НЕ форму. Слабые local-модели (Qwen 1.5-7B)
//! отдавали JSON без `call_type` / без обязательных v2-полей → serde fail →
//! fallback на v1 legacy → пустые summary/задачи.
//!
//! llama.cpp (b9270+) умеет `--json-schema-file` — сам конвертит JSON Schema в
//! GBNF и констрейнит decoding ИМЕННО под форму (required-поля, enum, массивы).
//! Эти схемы передаются через [`crate::providers::llm::LlmRequest::json_schema`].
//!
//! ## Дизайн
//!
//! - Имена полей = snake_case (как serde-выход [`super::summary_v2`]).
//! - `evidence` и `id` ОПЦИОНАЛЬНЫ: слабая модель не обязана заземлять цитату.
//!   Items без evidence сохраняются (см. `summary_validator::strip_unverified_evidence`
//!   Fix A); `id` авто-генерится serde-default'ом.
//! - Массивы могут быть пустыми (`minItems` не задаём) — форма гарантирована,
//!   контент зависит от модели.
//! - Self-contained, без `$ref` (требование llama `--json-schema-file`).
//! - Cloud (Anthropic/Groq) эти схемы не использует — native API contracts.

/// Классификатор типа звонка (Phase A). Форсит присутствие `call_type` —
/// чинит «classifier JSON shape: missing field call_type».
pub(crate) const CLASSIFIER_JSON_SCHEMA: &str = r#"{
  "type": "object",
  "properties": {
    "call_type": {
      "type": "string",
      "enum": ["sales_discovery","sales_demo","product_sync","standup","customer_interview","one_on_one","strategy_brainstorm","status_update","other"]
    },
    "confidence": { "type": "number" },
    "language": { "type": "string", "enum": ["ru","en","kk","mixed"] }
  },
  "required": ["call_type"]
}"#;

/// Title-only регенерация (T-17 local path). Форсит `{ "title": string }` —
/// слабая local-модель иначе даёт мусор/массив → fallback «Без названия».
pub(crate) const TITLE_JSON_SCHEMA: &str = r#"{
  "type": "object",
  "properties": { "title": { "type": "string" } },
  "required": ["title"]
}"#;

/// [recap-rich] Нарратив-минутки — markdown внутри одного JSON-поля. Грамматика
/// форсит валидный `{ "narrative": string }`, парсер извлекает строку.
pub(crate) const NARRATIVE_JSON_SCHEMA: &str = r#"{
  "type": "object",
  "properties": { "narrative": { "type": "string" } },
  "required": ["narrative"]
}"#;

/// Полная форма `CallSummaryV2`. Форсит v2-shape → больше нет v1-fallback'а на
/// слабой local-модели. `evidence`/`id` опциональны; массивы могут быть пусты.
/// Инлайн без `$ref` — llama `--json-schema-file` предупреждает про $refs.
pub(crate) const SUMMARY_V2_JSON_SCHEMA: &str = r#"{
  "type": "object",
  "properties": {
    "schema_version": { "type": "integer", "enum": [2] },
    "title": { "type": "string" },
    "summary": { "type": "string" },
    "key_points": { "type": "array", "maxItems": 20, "items": { "type": "string" } },
    "mom": { "type": "string" },
    "language": { "type": "string", "enum": ["ru","en","kk","mixed"] },
    "call_type": {
      "type": "string",
      "enum": ["sales_discovery","sales_demo","product_sync","standup","customer_interview","one_on_one","strategy_brainstorm","status_update","other"]
    },
    "call_type_confidence": { "type": "number" },
    "participants": {
      "type": "array",
      "items": {
        "type": "object",
        "properties": {
          "speaker_tag": { "type": "string" },
          "display_name": { "type": "string" },
          "role_hint": { "type": "string" }
        },
        "required": ["speaker_tag"]
      }
    },
    "action_items": {
      "type": "array",
      "items": {
        "type": "object",
        "properties": {
          "id": { "type": "string" },
          "text": { "type": "string" },
          "owner_hint": { "type": "string" },
          "owner_confidence": { "type": "number" },
          "due": { "type": "string" },
          "due_confidence": { "type": "number" },
          "category": { "type": "string", "enum": ["commitment","proposal","idea"] },
          "evidence": {
            "type": "object",
            "properties": {
              "quote": { "type": "string" },
              "speaker": { "type": "string" },
              "start_ms": { "type": "integer" },
              "end_ms": { "type": "integer" }
            },
            "required": ["quote"]
          }
        },
        "required": ["text"]
      }
    },
    "decisions": {
      "type": "array",
      "items": {
        "type": "object",
        "properties": {
          "id": { "type": "string" },
          "text": { "type": "string" },
          "confidence": { "type": "number" },
          "evidence": {
            "type": "object",
            "properties": {
              "quote": { "type": "string" },
              "speaker": { "type": "string" },
              "start_ms": { "type": "integer" },
              "end_ms": { "type": "integer" }
            },
            "required": ["quote"]
          }
        },
        "required": ["text"]
      }
    },
    "open_questions": {
      "type": "array",
      "items": {
        "type": "object",
        "properties": {
          "id": { "type": "string" },
          "text": { "type": "string" },
          "raised_by": { "type": "string" },
          "evidence": {
            "type": "object",
            "properties": {
              "quote": { "type": "string" },
              "speaker": { "type": "string" },
              "start_ms": { "type": "integer" },
              "end_ms": { "type": "integer" }
            },
            "required": ["quote"]
          }
        },
        "required": ["text"]
      }
    },
    "topics": {
      "type": "array",
      "maxItems": 6,
      "items": {
        "type": "object",
        "properties": {
          "title": { "type": "string" },
          "points": { "type": "array", "maxItems": 6, "items": { "type": "string" } }
        },
        "required": ["title"]
      }
    }
  },
  "required": ["schema_version","title","summary","key_points","language","call_type","call_type_confidence","action_items","decisions","open_questions"]
}"#;

/// [recap-fix] Map-шаг map-reduce (`map_reduce::build_map_prompt`). Раньше
/// map-вызовы шли через generic outer-`{}` grammar (`gbnf::UNIVERSAL_…`) без
/// формы → слабая Light-модель (1.5B) отдавала prose/обрезанный JSON →
/// `extract_json_object` fail → чанк молча дропался → половина звонка терялась.
/// Reduce уже жёстко констрейнится `SUMMARY_V2_JSON_SCHEMA` и на Light работает
/// — значит и map надо схемой. Nullable-поля (speaker/owner_hint/…) заданы как
/// НЕобязательный `string` (не union `["string","null"]`) — проще для
/// llama `--json-schema-file` конвертера + модель просто опускает поле.
pub(crate) const MAP_CHUNK_JSON_SCHEMA: &str = r#"{
  "type": "object",
  "properties": {
    "chunk_idx": { "type": "integer" },
    "facts": { "type": "array", "maxItems": 20, "items": { "type": "string" } },
    "decisions_candidates": {
      "type": "array",
      "items": {
        "type": "object",
        "properties": {
          "text": { "type": "string" },
          "evidence_quote": { "type": "string" },
          "speaker": { "type": "string" }
        },
        "required": ["text"]
      }
    },
    "action_candidates": {
      "type": "array",
      "items": {
        "type": "object",
        "properties": {
          "text": { "type": "string" },
          "owner_hint": { "type": "string" },
          "due": { "type": "string" },
          "category": { "type": "string", "enum": ["commitment","proposal","idea"] },
          "evidence_quote": { "type": "string" },
          "speaker": { "type": "string" }
        },
        "required": ["text"]
      }
    },
    "open_questions_candidates": {
      "type": "array",
      "items": {
        "type": "object",
        "properties": {
          "text": { "type": "string" },
          "raised_by": { "type": "string" },
          "evidence_quote": { "type": "string" }
        },
        "required": ["text"]
      }
    },
    "topic_tags": { "type": "array", "maxItems": 20, "items": { "type": "string" } },
    "participants_mentioned": { "type": "array", "maxItems": 20, "items": { "type": "string" } }
  },
  "required": ["facts","decisions_candidates","action_candidates","open_questions_candidates","topic_tags","participants_mentioned"]
}"#;

/// [recap-fix] Mid-reduce (level-2, `map_reduce::build_mid_reduce_prompt`) —
/// та же форма что map-чанк, но `group_idx` вместо `chunk_idx`. Констрейним по
/// той же причине что и map.
pub(crate) const MID_REDUCE_JSON_SCHEMA: &str = r#"{
  "type": "object",
  "properties": {
    "group_idx": { "type": "integer" },
    "facts": { "type": "array", "maxItems": 20, "items": { "type": "string" } },
    "decisions_candidates": {
      "type": "array",
      "items": {
        "type": "object",
        "properties": {
          "text": { "type": "string" },
          "evidence_quote": { "type": "string" },
          "speaker": { "type": "string" }
        },
        "required": ["text"]
      }
    },
    "action_candidates": {
      "type": "array",
      "items": {
        "type": "object",
        "properties": {
          "text": { "type": "string" },
          "owner_hint": { "type": "string" },
          "due": { "type": "string" },
          "category": { "type": "string", "enum": ["commitment","proposal","idea"] },
          "evidence_quote": { "type": "string" },
          "speaker": { "type": "string" }
        },
        "required": ["text"]
      }
    },
    "open_questions_candidates": {
      "type": "array",
      "items": {
        "type": "object",
        "properties": {
          "text": { "type": "string" },
          "raised_by": { "type": "string" },
          "evidence_quote": { "type": "string" }
        },
        "required": ["text"]
      }
    },
    "topic_tags": { "type": "array", "maxItems": 20, "items": { "type": "string" } },
    "participants_mentioned": { "type": "array", "maxItems": 20, "items": { "type": "string" } }
  },
  "required": ["facts","decisions_candidates","action_candidates","open_questions_candidates","topic_tags","participants_mentioned"]
}"#;

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn parse(s: &str) -> Value {
        serde_json::from_str(s).expect("schema должна быть валидным JSON")
    }

    #[test]
    fn title_schema_valid_json_requires_title() {
        let v = parse(TITLE_JSON_SCHEMA);
        assert_eq!(v["required"][0], "title");
        assert_eq!(v["properties"]["title"]["type"], "string");
    }

    #[test]
    fn classifier_schema_valid_json_requires_call_type() {
        let v = parse(CLASSIFIER_JSON_SCHEMA);
        let req = v["required"].as_array().unwrap();
        assert!(req.iter().any(|x| x == "call_type"));
        let en = &v["properties"]["call_type"]["enum"];
        assert!(en.as_array().unwrap().iter().any(|x| x == "one_on_one"));
        assert_eq!(en.as_array().unwrap().len(), 9, "9 CallType значений");
    }

    #[test]
    fn summary_v2_schema_valid_json_requires_v2_core() {
        let v = parse(SUMMARY_V2_JSON_SCHEMA);
        let req: Vec<&str> = v["required"]
            .as_array()
            .unwrap()
            .iter()
            .map(|x| x.as_str().unwrap())
            .collect();
        for f in [
            "schema_version",
            "call_type",
            "action_items",
            "decisions",
            "open_questions",
        ] {
            assert!(req.contains(&f), "required должно содержать {f}");
        }
        // evidence НЕ required на item — слабая модель может опустить.
        let ai_req = v["properties"]["action_items"]["items"]["required"]
            .as_array()
            .unwrap();
        assert!(ai_req.iter().any(|x| x == "text"));
        assert!(
            !ai_req.iter().any(|x| x == "evidence"),
            "evidence опционален"
        );
        // schema_version форсится в 2.
        assert_eq!(v["properties"]["schema_version"]["enum"][0], 2);
    }

    #[test]
    fn map_chunk_schema_valid_json_requires_all_arrays() {
        let v = parse(MAP_CHUNK_JSON_SCHEMA);
        let req: Vec<&str> = v["required"]
            .as_array()
            .unwrap()
            .iter()
            .map(|x| x.as_str().unwrap())
            .collect();
        for f in [
            "facts",
            "decisions_candidates",
            "action_candidates",
            "open_questions_candidates",
            "topic_tags",
            "participants_mentioned",
        ] {
            assert!(req.contains(&f), "map schema required должно содержать {f}");
        }
        // category — enum commitment|proposal|idea.
        let cat = &v["properties"]["action_candidates"]["items"]["properties"]["category"]["enum"];
        assert_eq!(cat.as_array().unwrap().len(), 3);
    }

    #[test]
    fn mid_reduce_schema_valid_json_same_shape_group_idx() {
        let v = parse(MID_REDUCE_JSON_SCHEMA);
        assert_eq!(v["properties"]["group_idx"]["type"], "integer");
        let req: Vec<&str> = v["required"]
            .as_array()
            .unwrap()
            .iter()
            .map(|x| x.as_str().unwrap())
            .collect();
        assert!(req.contains(&"facts"));
        assert!(req.contains(&"participants_mentioned"));
    }
}
