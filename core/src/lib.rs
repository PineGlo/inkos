//! Core library entry point that wires together the major InkOS subsystems.
//!
//! Each module is intentionally kept lightweight so that the boundaries
//! between responsibilities remain obvious when exploring the codebase:
//! - [`agents`] handles AI provider configuration and the runtime orchestrator.
//! - [`api`] exposes the IPC surface that the Tauri UI invokes.
//! - [`db`] initialises the SQLite database and applies migrations.
//! - [`errors`] keeps the central error catalogue with human friendly metadata.
//! - [`logging`] writes structured diagnostics to the event log table.
//! - [`workers`] implements synchronous background jobs such as the daily digest.

pub mod agents;
pub mod api;
pub mod db;
pub mod errors;
pub mod logging;
pub mod model_manager;
pub mod summarizer;
pub mod workers;
