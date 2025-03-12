use std::path::PathBuf;

use which::which;
use serde::{Deserialize, Serialize};

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

impl Config {
    // This method provides smart fallbacks when no editor is configured
    pub fn get_editor_command(&self) -> String {
        // First try the configured editor
        if let Some(editor) = &self.editor_command {
            return editor.clone();
        }

        // Then try environment variable
        if let Ok(editor) = std::env::var("EDITOR") {
            return editor;
        }

        // Fall back to platform defaults
        if cfg!(windows) {
            "notepad".to_string()
        } else if cfg!(target_os = "macos") {
            "open -t".to_string()
        } else {
            // Try common Linux editors
            for editor in &["nano", "vim", "vi", "emacs"] {
                if which(editor).is_ok() {
                    return editor.to_string();
                }
            }
            "nano".to_string()
        }
    }
}
