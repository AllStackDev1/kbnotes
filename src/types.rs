//! Core data structures for the kbnotes application.
//!
//! This module contains the primary types used throughout the application,
//! including Note and Config structures.
use std::path::PathBuf;

use chrono::{DateTime, Utc};
use clap::Subcommand;

use crate::{Note, KbError};

/// A specialized Result type for kbnotes operations.
pub type Result<T> = std::result::Result<T, KbError>;

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

/// Available subcommands for the kbnotes application
#[derive(Subcommand)]
pub enum Commands {
    /// Create a new note
    Create {
        /// Title of the note
        #[clap(short = 'T', long)]
        title: String,

        /// Content of the note, can be markdown formatted
        #[clap(short, long)]
        content: Option<String>,

        /// Open content in editor before saving
        #[clap(short, long)]
        edit: bool,

        /// Tags to associate with the note (comma-separated)
        #[clap(short = 't', long)]
        tags: Option<String>,

        /// Path to a file containing the note's content
        #[clap(short, long)]
        file: Option<PathBuf>,
    },

    /// View a note by ID
    View {
        /// ID of the note to view
        id: String,

        /// Format output as raw JSON
        #[clap(short, long)]
        json: bool,

        /// Open in the default editor
        #[clap(short, long)]
        edit: bool,
    },

    /// List notes with optional filtering
    List {
        /// Filter notes by tag
        #[clap(short, long)]
        tag: Option<String>,

        /// Limit the number of notes returned
        #[clap(short = 'n', long, default_value_t = 10)]
        limit: usize,

        /// Format output as JSON
        #[clap(short, long)]
        json: bool,

        /// Only show note IDs and titles
        #[clap(short, long)]
        brief: bool,
    },

    /// Search notes by title or content
    Search {
        /// Search query text
        query: String,

        /// Limit the number of search results
        #[clap(short = 'n', long, default_value_t = 10)]
        limit: usize,

        /// Format output as JSON
        #[clap(short, long)]
        json: bool,
    },

    /// Edit an existing note
    Edit {
        /// ID of the note to edit
        id: String,

        /// New title for the note
        #[clap(short = 'T', long)]
        title: Option<String>,

        /// New content for the note
        #[clap(short, long)]
        content: Option<String>,

        /// Open content in editor before saving
        #[clap(short, long)]
        edit: bool,

        /// Tags to associate with the note (comma-separated)
        #[clap(short = 't', long)]
        tags: Option<String>,

        /// Path to a file containing the new note content
        #[clap(short, long)]
        file: Option<PathBuf>,
    },

    /// Delete a note by ID
    Delete {
        /// ID of the note to delete
        id: String,

        /// Skip confirmation prompt
        #[clap(short, long)]
        force: bool,
    },

    /// Tag operations (add, remove, list)
    Tag {
        /// ID of the note to modify
        id: String,

        /// Tags to add (comma-separated)
        #[clap(short, long)]
        add: Option<String>,

        /// Tags to remove (comma-separated)
        #[clap(short, long)]
        remove: Option<String>,

        /// List all tags for the note
        #[clap(short, long)]
        list: bool,
    },

    /// Create a backup of all notes
    Backup {
        /// Path for the backup file (default uses config setting)
        #[clap(short, long)]
        output: Option<PathBuf>,
    },

    /// Restore notes from a backup
    Restore {
        /// Path to the backup file
        backup_file: PathBuf,

        /// Skip confirmation prompt
        #[clap(short, long)]
        force: bool,
    },

    /// Configuration management
    Config {
        /// Show current configuration
        #[clap(short = 'S', long)]
        show: bool,

        /// Update a configuration setting
        #[clap(short, long)]
        set: Option<String>,

        /// Reset configuration to defaults
        #[clap(short, long)]
        reset: bool,
    },

    /// Import notes from external sources
    Import {
        /// Path to the file or directory to import from
        source: PathBuf,

        /// Format of the import source
        #[clap(short, long, value_parser = ["markdown", "json", "text"], default_value = "markdown")]
        format: String,

        /// Default tags to apply to imported notes (comma-separated)
        #[clap(short, long)]
        tags: Option<String>,
    },

    /// Export notes to various formats
    Export {
        /// Path where exported files will be saved
        #[clap(short, long)]
        output: PathBuf,

        /// Format to export to
        #[clap(short, long, value_parser = ["markdown", "json", "html", "pdf"], default_value = "markdown")]
        format: String,

        /// Filter notes by tag for export
        #[clap(short, long)]
        tag: Option<String>,

        /// Export as a single file instead of multiple files
        #[clap(short = 's', long)]
        single_file: bool,
    },
}
