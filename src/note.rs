//! Core data structures for the kbnotes application.
//!
//! This module contains the primary types used throughout the application,
//! including Note and Config structures.
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
