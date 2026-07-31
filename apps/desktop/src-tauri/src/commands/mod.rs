//! [Phase 4 R4] Tauri commands, разбитые по доменам.
//!
//! `lib.rs::invoke_handler![]` ссылается на `commands::xxx` — поэтому каждый
//! sub-module делает `pub use` на свои command'ы, а `mod.rs` собирает их в
//! плоский namespace через `pub use`.

pub mod admin;
pub mod assistant;
#[cfg(target_os = "macos")]
pub mod call_detection;
pub mod calls;
pub mod chunk_retry;
pub mod chunked_setup;
pub mod contacts;
#[cfg(target_os = "macos")]
pub mod local_engine;
pub mod notify;
pub mod orphan_reconcile;
pub mod parked_resume;
pub mod pipeline;
pub mod recording;
pub mod recovery;
pub mod settings;
pub mod share;
#[cfg(target_os = "macos")]
pub mod silence;
pub mod speakers;
pub mod widget;

// Re-exports чтобы `commands::list_calls` и т.д. продолжали резолвиться
// из `lib.rs::invoke_handler!`.
pub use admin::*;
pub use assistant::*;
#[cfg(target_os = "macos")]
pub use call_detection::*;
pub use calls::*;
pub use chunk_retry::*;
pub use contacts::*;
#[cfg(target_os = "macos")]
pub use local_engine::*;
pub use notify::*;
pub use parked_resume::*;
pub use pipeline::*;
pub use recording::*;
pub use recovery::*;
pub use settings::*;
pub use share::*;
#[cfg(target_os = "macos")]
pub use silence::*;
pub use speakers::*;
pub use widget::*;
