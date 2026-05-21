//! [Phase 4 R4] Tauri commands, разбитые по доменам.
//!
//! `lib.rs::invoke_handler![]` ссылается на `commands::xxx` — поэтому каждый
//! sub-module делает `pub use` на свои command'ы, а `mod.rs` собирает их в
//! плоский namespace через `pub use`.

pub mod admin;
#[cfg(target_os = "macos")]
pub mod call_detection;
pub mod calls;
pub mod contacts;
pub mod pipeline;
pub mod recording;
pub mod secrets;
pub mod settings;
pub mod speakers;
pub mod voice_model;
pub mod widget;

// Re-exports чтобы `commands::list_calls` и т.д. продолжали резолвиться
// из `lib.rs::invoke_handler!`.
pub use admin::*;
#[cfg(target_os = "macos")]
pub use call_detection::*;
pub use calls::*;
pub use contacts::*;
pub use pipeline::*;
pub use recording::*;
pub use secrets::*;
pub use settings::*;
pub use speakers::*;
pub use voice_model::*;
pub use widget::*;
