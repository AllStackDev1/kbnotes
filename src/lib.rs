//! Knowledge Base Note-taking application library
//!
//! This library provides functionality for creating, storing, searching, and managing notes
//! with tags and content in Markdown format.

mod backup_scheduler;
mod cli;
mod errors;
mod helper;
mod note;
mod storage;
mod types;
mod config;

// Re-export key components
pub use backup_scheduler::*;
pub use config::*;
pub use cli::*;
pub use errors::*;
pub use helper::*;
pub use note::*;
pub use storage::*;
pub use types::*;
