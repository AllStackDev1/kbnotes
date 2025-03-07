//! Knowledge Base Note-taking application library
//! 
//! This library provides functionality for creating, storing, searching, and managing notes
//! with tags and content in Markdown format.

mod types;
mod errors;
mod storage;
mod backup_scheduler; // Add the new module

// Re-export key components
pub use types::{Note, Config};
pub use errors::{Result, KbError};
pub use storage::NoteStorage;
pub use backup_scheduler::{BackupScheduler, BackupSchedulerStatus};