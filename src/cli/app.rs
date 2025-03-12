//! CLI module for the kbnotes application
//!
//! This module handles the command-line interface for interacting with the
//! note storage system.
use std::fs::{read_to_string, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use shell_words::split;
use tempfile::Builder;
use tokio::sync::Mutex;

use crate::{parse_tags, Commands, Config, KbError, Note, NoteStorage, Result};

/// CLI Application handler - processes CLI commands and interfaces with NoteStorage
pub struct App {
    /// The note storage backend
    note_storage: Arc<Mutex<NoteStorage>>,

    /// Application configuration
    config: Config,

    /// Whether to display verbose output
    verbose: bool,
}

impl App {
    /// Create a new CLI application with the given storage backend and config
    pub fn new(note_storage: Arc<Mutex<NoteStorage>>, config: Config, verbose: bool) -> Self {
        Self {
            note_storage,
            config,
            verbose,
        }
    }

    /// Run the CLI application with the given command
    pub async fn run(&self, command: Commands) -> Result<()> {
        match command {
            Commands::Create {
                title,
                content,
                edit,
                tags,
                file,
            } => {
                /*    self.create_note(title, content, file, tags, no_editor)
                .await */
            }

            Commands::View { id, json, edit } => {
                // Implementation outline:
                // 1. Retrieve note by ID
                // 2. If edit flag, open in editor then save
                // 3. Display note in requested format
            }

            Commands::List {
                tag,
                limit,
                json,
                brief,
            } => {
                // Implementation outline:
                // 1. Get notes, filtered by tag if provided
                // 2. Apply limit
                // 3. Display in requested format
            }

            Commands::Search { query, limit, json } => {
                // Implementation outline:
                // 1. Perform search with query string
                // 2. Apply limit
                // 3. Display results in requested format
            }

            Commands::Edit {
                id,
                title,
                content,
                edit,
                tags,
                file,
            } => {
                // Implementation outline:
                // 1. Retrieve existing note
                // 2. Apply changes from args
                // 3. If edit flag, open in editor
                // 4. Save updated note
                // 5. Display result
            }

            Commands::Delete { id, force } => {
                // Implementation outline:
                // 1. If not force, confirm deletion
                // 2. Delete note by ID
                // 3. Display confirmation
            }

            Commands::Tag {
                id,
                add,
                remove,
                list,
            } => {
                // Implementation outline:
                // 1. Retrieve note by ID
                // 2. If list, show tags and return
                // 3. If add, parse and add new tags
                // 4. If remove, parse and remove tags
                // 5. Save updated note
                // 6. Display result
            }

            Commands::Backup { output } => {
                // Implementation outline:
                // 1. Determine backup path (from arg or config)
                // 2. Create backup
                // 3. Display result
            }

            Commands::Restore { backup_file, force } => {
                // Implementation outline:
                // 1. If not force, confirm restoration
                // 2. Restore from backup file
                // 3. Display result
            }

            Commands::Config { show, set, reset } => {
                // Implementation outline:
                // 1. If show, display current config
                // 2. If reset, reset to defaults
                // 3. If set, parse key=value and update setting
                // 4. Save config
            }

            Commands::Import {
                source,
                format,
                tags,
            } => {
                // Implementation outline:
                // 1. Validate source exists
                // 2. Parse format and tags
                // 3. Import notes
                // 4. Display result summary
            }

            Commands::Export {
                output,
                format,
                tag,
                single_file,
            } => {
                // Implementation outline:
                // 1. Create output directory if needed
                // 2. Get notes, filtered by tag if provided
                // 3. Export in requested format
                // 4. Display result summary
            }
        }

        Ok(())
    }

    async fn create_note(
        &self,
        title: String,
        content: Option<String>,
        file: Option<PathBuf>,
        tags: Option<String>,
        no_editor: bool,
    ) -> Result<()> {
        // Your implementation from earlier, adapted to CliApp context
        let parsed_tags = parse_tags(tags);

        // Get content based on the provided options
        let note_content = match (content, file) {
            (Some(c), _) => c,
            (_, Some(file_path)) => {
                if !file_path.exists() {
                    return Err(KbError::FileNotFound {
                        file_path: file_path.display().to_string(),
                    });
                }
                read_to_string(file_path)?
            }
            (None, None) => {
                if no_editor {
                    String::new()
                } else {
                    self.open_editor_for_content(&title)?
                }
            }
        };

        // Create and save the note
        let note = Note::new(title, note_content, parsed_tags);

        self.note_storage.lock().await.save_note(&note)?;
        println!("Note created with ID: {}", note.id);
        Ok(())
    }

    fn open_editor_for_content(&self, title: &str) -> Result<String> {
        // Create a temporary file with .md extension
        let temp_file = Builder::new().suffix(".md").tempfile()?;
        let temp_path = temp_file.path().to_path_buf();

        // Get editor from config or environment
        let editor_cmd = self.config.get_editor_command();

        // Write template to the temp file
        self.write_editor_template(&temp_path, title)?;

        // Open editor
        println!("Opening editor to write note content. Save and exit when done...");
        self.launch_editor(&editor_cmd, &temp_path)?;

        // Read and process the content
        let content = read_to_string(&temp_path)?;
        Ok(self.process_editor_content(content))
    }

    fn write_editor_template(&self, path: &Path, title: &str) -> Result<()> {
        let mut file = OpenOptions::new().write(true).open(path)?;

        // Write template with helpful comments
        writeln!(file, "# {}", title)?;
        writeln!(file)?;
        writeln!(file, "<!-- ")?;
        writeln!(
            file,
            "Write your note content below. This note supports Markdown format."
        )?;
        writeln!(
            file,
            "Lines that start with <!-- and end with --> are comments and will be ignored."
        )?;
        writeln!(file, "Save and exit the editor when you're done.")?;
        writeln!(file, "-->")?;
        writeln!(file)?;

        Ok(())
    }

    fn launch_editor(&self, editor_cmd: &str, file_path: &Path) -> Result<()> {
        // Convert file path to string once
        let path_str = file_path.to_string_lossy();

        // Handle shell-like command parsing
        let args = split(editor_cmd).map_err(|e| KbError::EditorError {
            message: format!("Failed to parse editor command: {}", e),
        })?;

        if args.is_empty() {
            return Err(KbError::EditorError {
                message: "Empty editor command".to_string(),
            });
        }

        // First word is the program name, rest are arguments
        let program = &args[0];

        // Create command
        let mut command = Command::new(program);

        // Add any arguments from the original command
        if args.len() > 1 {
            command.args(&args[1..]);
        }

        // Add the file path as the final argument
        command.arg(path_str.as_ref());

        // Execute the command
        let status = command.status()?;

        if !status.success() {
            return Err(KbError::EditorError {
                message: "Editor exited with non-zero status".to_string(),
            });
        }

        Ok(())
    }

    fn process_editor_content(&self, content: String) -> String {
        // Remove HTML comments from content
        content
            .lines()
            .filter(|line| {
                !line.trim_start().starts_with("<!--") && !line.trim_end().ends_with("-->")
            })
            .collect::<Vec<&str>>()
            .join("\n")
    }
}
