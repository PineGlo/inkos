use thiserror::Error;
#[derive(Debug, Error)]
pub enum InkOsError {
    #[error("Database unavailable")] DbUnavailable,
    #[error("Note not found")] NoteNotFound,
    #[error("Unknown error")] Unknown,
}
impl InkOsError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::DbUnavailable => "DB-1001",
            Self::NoteNotFound => "NTE-1001",
            Self::Unknown => "GEN-1000",
        }
    }
    pub fn explain(&self) -> &'static str {
        match self {
            Self::DbUnavailable => "The application could not access the SQLite database.",
            Self::NoteNotFound => "No note exists for the requested ID.",
            Self::Unknown => "An unspecified error occurred.",
        }
    }
}
