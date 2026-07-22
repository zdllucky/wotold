//! [M15] Ассистент — локальный RAG-чат по звонкам.
//!
//! PRD: docs/M15_ASSISTANT_PRD.md. Модули подключаются по фазам:
//! - M15.1: `types` (контракт S2, зеркало packages/contracts/src/assistant.ts)
//! - M15.3: `indexer` · M15.4: `classifier` · M15.5: `retrieval`
//! - M15.6: `budget` · M15.7: `answer` · Ph2: `embedder`

pub mod types;
