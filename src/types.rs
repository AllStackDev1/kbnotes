//! Core data structures for the kbnotes application.
//!
//! This module contains the primary types used throughout the application,
//! including Note and Config structures.

use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Represents a single note in our system
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Note {
    /// Unique identifier for the note
    pub id: String,
    /// Note title
    pub title: String,
    /// Note content in Markdown format
    pub content: String,
    /// Tags for organization
    pub tags: Vec<String>,
    /// When the note was created
    pub created_at: DateTime<Utc>,
    /// Last modification time
    pub updated_at: DateTime<Utc>,
}

impl Note {
    /// Creates a new note with the given title and content
    pub fn new(title: String, content: String, tags: Vec<String>) -> Self {
        let now = Utc::now();
        // Generate a unique ID using timestamp and title
        let id = format!(
            "{}-{}",
            now.timestamp_millis(),
            title.to_lowercase().replace(' ', "-")
        );

        Note {
            id,
            title,
            content,
            tags,
            created_at: now,
            updated_at: now,
        }
    }
}

/// Represents the expected state of a note for concurrency control
pub struct NoteVersion {
    /// The ID of the note
    pub id: String,
    /// The expected last update timestamp
    pub updated_at: DateTime<Utc>,
}

/// Summary of a backup restoration operation
#[derive(Debug, Clone)]
pub struct RestoreBackupSummary {
    /// Path to the backup file that was restored
    pub backup_file: PathBuf,
    /// Total number of notes found in the backup
    pub total_notes: usize,
    /// Number of notes successfully restored
    pub notes_restored: usize,
    /// Number of notes skipped (e.g., due to existing notes with overwrite disabled)
    pub notes_skipped: usize,
    /// Details about notes that failed to restore
    pub failed_notes: Vec<(String, String)>, // (note_id, error_message)
}

/// Represents the result of an attempt to resolve a concurrent modification conflict
pub enum ConflictResolution {
    /// The update should use the client's version (force update)
    UseClientVersion,
    /// The update should use the server's version (discard changes)
    UseServerVersion,
    /// The update should use a merged version
    UseMergedVersion(Note),
    /// The conflict was not resolved
    Unresolved,
}

/// Application configuration settings.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Config {
    /// Directory where notes are stored
    pub notes_dir: PathBuf,

    /// Directory for backups
    pub backup_dir: PathBuf,

    /// How often to create backups (in hours)
    pub backup_frequency: u32,

    /// Maximum number of backups to keep
    pub max_backups: u32,

    /// Whether to encrypt notes (for future extension)
    pub encrypt_notes: bool,

    /// Default editor command (for future extension)
    pub editor_command: Option<String>,

    /// Whether to enable auto-saving (for future extension)
    pub auto_save: bool,

    /// Whether to enable auto-saving (for future extension)
    pub auto_backup: bool,
    // /// Auto-save interval in minutes (if auto_save is enabled) (for future extension)
    // pub auto_save_interval: u32,

    // /// Default file format for notes (.md, .txt, etc.) (for future extension)
    // pub default_format: String,
}
