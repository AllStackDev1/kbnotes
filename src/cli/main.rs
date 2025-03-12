use std::path::PathBuf;

use clap::Parser;

use crate::Commands;

/// Main CLI application arguments and command structure
#[derive(Parser)]
#[clap(
    author = "Your Name <your.email@example.com>",
    version = "1.0.0",
    about = "Knowledge Base and Note-taking Application"
)]
pub struct Cli {
    /// Path to the configuration file
    #[clap(short = 'c', long, value_parser)]
    pub config: Option<PathBuf>,

    /// Path to the notes directory
    #[clap(long, value_parser)]
    pub notes_dir: Option<String>,

    /// Path to the backup directory
    #[clap(long, value_parser)]
    pub backup_dir: Option<String>,

    /// Verbose output mode
    #[clap(short, long)]
    pub verbose: bool,

    /// Subcommands for the kbnotes application
    #[clap(subcommand)]
    pub command: Commands,
}
