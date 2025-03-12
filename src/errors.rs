//! Error types for the kbnotes application.
//!
//! This module defines custom error types that categorize different failures
//! that can occur during note management operations.

use std::{io, path::PathBuf};

use chrono::{DateTime, Utc};
use thiserror::Error;

/// The main error type for the kbnotes application.
#[derive(Error, Debug)]
pub enum KbError {
    /// Errors related to file I/O operations.
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),

    /// Errors related to serialization/deserialization operations.
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    /// Errors related to zip operations.
    #[error("Zip error: {0}")]
    ZipError(#[from] zip::result::ZipError),

    /// Note was not found when performing an operation.
    #[error("Note not found: {id}")]
    NoteNotFound { id: String },

    /// Note with the same ID already exists.
    #[error("Note already exists: {id}")]
    NoteAlreadyExists { id: String },

    /// Invalid note format or content.
    #[error("Invalid note format: {message}")]
    InvalidFormat { message: String },

    /// Errors related to backup operations.
    #[error("Backup failed: {message}")]
    BackupFailed { message: String },

    /// Errors related to configuration.
    #[error("Configuration error: {message}")]
    ConfigError { message: String },

    /// Directory creation or access failed.
    #[error("Failed to create or access directory: {path}")]
    DirectoryError { path: PathBuf },

    /// Error when attempting to restore from backup.
    #[error("Restore failed: {message}")]
    RestoreFailed { message: String },

    /// Generic application error with a custom message.
    #[error("{message}")]
    ApplicationError { message: String },

    /// for mutex lock acquisition issues
    #[error("{message}")]
    LockAcquisitionFailed { message: String },

    #[error("Concurrent modification detected for note {id}: Expected timestamp {expected_timestamp}, found {actual_timestamp}")]
    ConcurrentModification {
        id: String,
        expected_timestamp: DateTime<Utc>,
        actual_timestamp: DateTime<Utc>,
    },

    /// file not found
    #[error("File not found: {file_path}")]
    FileNotFound { file_path: String },

    #[error("{message}")]
    EditorError { message: String },
}
