//! Knowledge Base Note-taking application library
//!
//! This library provides functionality for creating, storing, searching, and managing notes
//! with tags and content in Markdown format.

mod backup_scheduler;
mod errors;
mod helper;
mod storage;
mod types; // Add the new module

// Re-export key components
pub use backup_scheduler::*;
pub use errors::*;
pub use storage::*;
pub use types::*;
