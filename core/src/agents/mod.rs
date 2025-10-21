//! AI subsystem glue code.
//!
//! `config` owns persistence of provider metadata and secrets, `providers`
//! defines the baked-in seeds, and `orchestrator` executes chat completions
//! against the selected runtime.

pub mod config;
pub mod orchestrator;
pub mod providers;

pub use config::{AiProviderInfo, AiRuntimeSelection, AiSettingsSnapshot};
pub use orchestrator::{AiChatInput, AiChatMessage, AiChatResponse, AiOrchestrator};
