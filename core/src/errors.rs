//! Domain specific error catalogue.
//!
//! Errors are intentionally terse in the Rust layer but always include a
//! stable code and explanation so the UI can surface meaningful guidance to
//! the user.

use thiserror::Error;

/// Canonical error variants emitted by the core service.
#[derive(Debug, Error)]
pub enum InkOsError {
    #[error("Database unavailable")]
    DbUnavailable,
    #[error("Note not found")]
    NoteNotFound,
    #[error("Unknown error")]
    Unknown,
}

impl InkOsError {
    /// Machine-readable error code that maps onto documentation in
    /// `docs/error-codes.md`.
    pub fn code(&self) -> &'static str {
        match self {
            Self::DbUnavailable => "DB-1001",
            Self::NoteNotFound => "NTE-1001",
            Self::Unknown => "GEN-1000",
        }
    }

    /// Human friendly explanation that can be shown in the UI or logs.
    pub fn explain(&self) -> &'static str {
        match self {
            Self::DbUnavailable => "The application could not access the SQLite database.",
            Self::NoteNotFound => "No note exists for the requested ID.",
            Self::Unknown => "An unspecified error occurred.",
        }
    }
}
