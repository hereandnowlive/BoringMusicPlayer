//! One error type for the whole backend.
//!
//! Rust has no exceptions. A function that can fail returns
//! `Result<T, E>`: either `Ok(value)` or `Err(error)`. The `?` operator
//! means "if this is an `Err`, return it from the current function right
//! now, otherwise give me the value inside the `Ok`".
//!
//! Each library we use has its own error type (`rusqlite::Error`,
//! `std::io::Error`, ...). `AppError` wraps them all so every function in
//! this project can share the same return type, `AppResult<T>`.
//!
//! Learning links:
//! - Error handling chapter: <https://doc.rust-lang.org/book/ch09-00-error-handling.html>
//! - The `?` operator: <https://doc.rust-lang.org/book/ch09-02-recoverable-errors-with-result.html#a-shortcut-for-propagating-errors-the--operator>
//! - `thiserror` (the derive macro used below): <https://docs.rs/thiserror>

use serde::{Serialize, Serializer};

/// Everything that can go wrong in the backend.
///
/// `#[error("...")]` is the message shown when the error is printed.
/// `#[from]` generates the conversion that lets `?` turn, for example, a
/// `rusqlite::Error` into an `AppError::Db` automatically.
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("Database error: {0}")]
    Db(#[from] rusqlite::Error),

    #[error("File error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Image error: {0}")]
    Image(#[from] image::ImageError),

    /// Reading or writing tags failed. lofty has several error types, so we
    /// store just the message.
    #[error("Tag error: {0}")]
    Tag(String),

    /// Decoding or playing audio failed.
    #[error("Audio error: {0}")]
    Audio(String),

    /// Anything else, with a message meant to be shown to the user as is.
    #[error("{0}")]
    Other(String),
}

/// Shorthand so functions can write `AppResult<Track>` instead of
/// `Result<Track, AppError>`.
pub type AppResult<T> = Result<T, AppError>;

/// Tauri sends command results to the UI as JSON, so the error type has to be
/// serialisable. We send only the human-readable message; on the TypeScript
/// side a failed `invoke()` rejects with that string.
impl Serialize for AppError {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}
