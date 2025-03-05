//! Core data structures for the kbnotes application.
//! 
//! This module contains the primary types used throughout the application,
//! including Note and Config structures.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

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

