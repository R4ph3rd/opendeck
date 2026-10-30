//! Library error type.

use crate::db::DbError;

/// Failure in a library operation.
#[derive(Debug, thiserror::Error)]
pub enum LibraryError {
    /// Opening or migrating the database failed.
    #[error(transparent)]
    Db(#[from] DbError),
    /// SQLite rejected a statement or a constraint failed.
    #[error("sqlite: {0}")]
    Sqlite(#[from] rusqlite::Error),
    /// Filesystem error.
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    /// JSON (settings, options) could not be encoded or decoded.
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    /// The row does not exist.
    #[error("{0} not found")]
    NotFound(&'static str),
    /// The request is not valid (bad range, cycle, wrong kind).
    #[error("invalid request: {0}")]
    Invalid(String),
    /// A stored value could not be decoded.
    #[error("corrupt stored value: {0}")]
    Corrupt(String),
    /// A source file (playlist, XML) could not be parsed.
    #[error("parse error: {0}")]
    Parse(String),
}

/// Result alias for library operations.
pub type Result<T> = std::result::Result<T, LibraryError>;
