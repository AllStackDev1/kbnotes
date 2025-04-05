//! Core data structures for the kbnotes application.
//!
//! This module contains the primary types used throughout the application,
//! including Note and Config structures.
use std::path::PathBuf;

use chrono::{DateTime, Utc};
use clap::{Args, Subcommand};

use crate::{KbError, Note};

#[derive(Debug, Clone, Args)]
pub struct ListNotesOptions {
    /// Filter notes by tag
    #[clap(short = 't', long = "tag")]
    pub tag: Option<String>,

    /// Search term to filter notes by title or content
    #[clap(short = 's', long = "search")]
    pub search: Option<String>,

    /// Maximum number of notes to display
    #[clap(short = 'n', long = "limit", default_value = "20")]
    pub limit: usize,

    /// Show detailed information including content
    #[clap(short = 'd', long = "detailed")]
    pub detailed: bool,

    /// Output format (text, json)
    #[clap(short = 'f', long = "format", default_value = "text", value_parser = clap::builder::PossibleValuesParser::new(["text", "json"]))]
    pub format: String,

    /// Sort notes by field (default is date)
    #[clap(long = "sort-by", default_value = "date", value_parser = clap::builder::PossibleValuesParser::new(["date", "title", "id"]))]
    pub sort_by: String,

    /// Sort in descending order
    #[clap(long = "desc")]
    pub descending: bool,
}

#[derive(Debug, Clone, Args)]
pub struct EditNoteOptions {
    /// ID of the note to edit
    pub id: String,

    /// New title for the note
    #[clap(short = 't', long = "title")]
    pub title: Option<String>,

    /// New content for the note (cannot be used with --file or --edit)
    #[clap(short = 'c', long = "content")]
    pub content: Option<String>,

    /// File to read content from (cannot be used with --content or --edit)
    #[clap(short = 'f', long = "file")]
    pub file: Option<String>,

    /// Open the editor to edit content (cannot be used with --content or --file)
    #[clap(short = 'e', long = "edit")]
    pub open_editor: bool,

    /// Tags to add (comma separated)
    #[clap(short = 'a', long = "add-tags")]
    pub add_tags: Option<String>,

    /// Tags to remove (comma separated)
    #[clap(short = 'r', long = "remove-tags")]
    pub remove_tags: Option<String>,
}

#[derive(Debug, Clone, Args)]
pub struct ImportOptions {
    /// Path to file or directory to import from
    #[clap(short = 'p', long = "path", required = true)]
    path: String,

    /// Format of the notes (markdown, json, text)
    #[clap(short = 'f', long = "format", default_value = "markdown", value_parser = clap::builder::PossibleValuesParser::new(["markdown", "md", "json", "text", "txt"]))]
    format: String,

    /// Tags to apply to all imported notes (comma separated)
    #[clap(short = 'g', long = "tags")]
    tags: Option<String>,

    /// Use filenames as note titles when importing
    #[clap(long = "title-from-filename")]
    title_from_filename: bool,

    /// Recursive import (for directories)
    #[clap(short = 'r', long = "recursive")]
    recursive: bool,

    /// Pattern to match files (glob syntax, e.g. "*.md")
    #[clap(long = "pattern")]
    pattern: Option<String>,

    /// Show detailed progress during import
    #[clap(short = 'v', long = "verbose")]
    verbose: bool,
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

    /// List all notes, optionally filtering by tag
    #[clap(
        name = "list",
        about = "List all notes, optionally filtering by tag",
        long_about = ""
    )]
    List(ListNotesOptions),

    /// Search for notes
    #[clap(
        name = "search",
        about = "Search for notes containing specific text",
        long_about = "Search for notes containing specific text in either title, content, or both.\n\nExamples:\n  kbnotes search \"project ideas\"\n  kbnotes search \"meeting\" --title-only\n  kbnotes search \"todo\" --limit 5 --format json"
    )]
    Search {
        /// Search query
        query: String,

        /// Maximum number of results to return
        #[clap(short = 'l', long = "limit", default_value = "0")]
        limit: usize,

        /// Output format (text, json)
        #[clap(short = 'f', long = "format", default_value = "text", value_parser = clap::builder::PossibleValuesParser::new(["text", "json"]))]
        format: String,

        /// Include note content in results
        #[clap(short = 'c', long = "include-content")]
        include_content: bool,
    },

    /// Edit an existing note
    #[clap(
        name = "edit",
        about = "Edit an existing note",
        long_about = "Edit a note's title, content, or tags. Content can be provided directly, read from a file, or entered using your default editor.\n\nExamples:\n  kbnotes edit abc123 --title \"Updated Title\"\n  kbnotes edit abc123 --content \"New content\"\n  kbnotes edit abc123 --file updates.md\n  kbnotes edit abc123 --edit\n  kbnotes edit abc123 --add-tags \"important,follow-up\""
    )]
    Edit(EditNoteOptions),

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
    #[clap(
        name = "import",
        about = "Import notes from external files or directories",
        long_about = "Import one or more notes from external files or directories with various format options.\n\nExamples:\n  kbnotes import -p ~/Documents/notes/ -f markdown\n  kbnotes import -p exported_notes.json -f json -g \"imported,archive\"\n  kbnotes import -p meeting_notes.md -f markdown --title-from-filename"
    )]
    Import(ImportOptions),

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
