//! CLI module for the kbnotes application
//!
//! This module handles the command-line interface for interacting with the
//! note storage system.
use std::{
    fs::{read_to_string, OpenOptions},
    io::{stdin, stdout, Write},
    path::{Path, PathBuf},
    process::Command,
    sync::Arc,
};

use log::info;

use shell_words::split;
use tempfile::Builder;
use tokio::sync::Mutex;

use crate::{
    parse_tags, Commands, Config, EditNoteOptions, KbError, ListNotesOptions, Note, NoteStorage,
    Result,
};

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
            } => self.create_note(title, content, file, tags, edit).await?,

            Commands::View { id, json, edit } => {}

            Commands::List(options) => self.list_notes(options).await?,

            Commands::Search {
                query,
                limit,
                format,
                include_content,
            } => {
                self.handle_search(query, limit, format, include_content)
                    .await?;
            }

            Commands::Edit(options) => self.handle_edit(options).await?,

            Commands::Delete { id, force } => self.handle_delete(id, force).await?,

            Commands::Tag {
                id,
                add,
                remove,
                list,
            } => {}

            Commands::Backup { output } => {}

            Commands::Restore { backup_file, force } => {}

            Commands::Config { show, set, reset } => {}

            Commands::Import {
                source,
                format,
                tags,
            } => {}

            Commands::Export {
                output,
                format,
                tag,
                single_file,
            } => {}
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
        info!("Opening editor to write note content. Save and exit when done...");
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

    /// List notes according to provided filters and options
    async fn list_notes(&self, options: ListNotesOptions) -> Result<()> {
        // Step 1: Retrieve notes based on filters
        let notes = self
            .retrieve_filtered_notes(options.tag, options.search)
            .await?;

        // Step 2: Sort notes based on sort criteria
        let mut sorted_notes = self.sort_notes(notes, &options.sort_by, options.descending);

        // Step 3: Apply limit
        if sorted_notes.len() > options.limit {
            sorted_notes.truncate(options.limit);
        }

        // Step 4: Display notes in requested format
        self.display_notes(&sorted_notes, &options.format, options.detailed)?;
        Ok(())
    }

    /// Retrieve notes based on tag and search filters
    async fn retrieve_filtered_notes(
        &self,
        tag: Option<String>,
        search: Option<String>,
    ) -> Result<Vec<Note>> {
        let storage = self.note_storage.lock().await.clone();
        match (tag, search) {
            // Case 1: Filter by both tag and search term
            (Some(tag_value), Some(search_term)) => {
                // First, filter by tag
                let tagged_notes = storage.get_notes_by_tag(&tag_value)?;

                // Then filter the tagged notes by search term
                let filtered_notes: Vec<Note> = tagged_notes
                    .into_iter()
                    .filter(|note| {
                        note.title.contains(&search_term) || note.content.contains(&search_term)
                    })
                    .collect();

                Ok(filtered_notes)
            }

            // Case 2: Filter by tag only
            (Some(tag_value), None) => storage.get_notes_by_tag(&tag_value),

            // Case 3: Filter by search term only
            (None, Some(search_term)) => Ok(storage.search_notes(&search_term)),

            // Case 4: No filters, show all notes
            (None, None) => Ok(Vec::new()),
        }
    }

    /// Sort notes by specified criteria
    fn sort_notes(&self, mut notes: Vec<Note>, sort_by: &str, descending: bool) -> Vec<Note> {
        match sort_by {
            "title" => {
                notes.sort_by(|a, b| {
                    let cmp = a.title.cmp(&b.title);
                    if descending {
                        cmp.reverse()
                    } else {
                        cmp
                    }
                });
            }
            "id" => {
                notes.sort_by(|a, b| {
                    let cmp = a.id.cmp(&b.id);
                    if descending {
                        cmp.reverse()
                    } else {
                        cmp
                    }
                });
            }
            // Default is "date"
            _ => {
                notes.sort_by(|a, b| {
                    let cmp = a.created_at.cmp(&b.created_at);
                    if descending {
                        cmp.reverse()
                    } else {
                        cmp
                    }
                });
            }
        }

        notes
    }

    /// Display notes in the requested format
    fn display_notes(&self, notes: &[Note], format: &str, detailed: bool) -> Result<()> {
        if notes.is_empty() {
            println!("No notes found matching the criteria.");
            return Ok(());
        }

        match format {
            "json" => self.display_notes_json(notes, detailed)?,
            _ => self.display_notes_text(notes, detailed)?,
        }

        // Print count at the end
        println!(
            "\nFound {} note{}",
            notes.len(),
            if notes.len() == 1 { "" } else { "s" }
        );

        Ok(())
    }

    /// Display notes in JSON format
    fn display_notes_json(&self, notes: &[Note], detailed: bool) -> Result<()> {
        // For JSON output, we'll either output the full notes or a simplified version
        if detailed {
            // Full notes with all fields
            println!("{}", serde_json::to_string_pretty(notes)?);
        } else {
            // Simplified notes with just id, title, and tags
            let simplified_notes: Vec<serde_json::Value> = notes
                .iter()
                .map(|note| {
                    serde_json::json!({
                        "id": note.id,
                        "title": note.title,
                        "created_at": note.created_at,
                        "updated_at": note.updated_at.to_rfc3339(),
                        "tags": note.tags,
                    })
                })
                .collect();

            println!("{}", serde_json::to_string_pretty(&simplified_notes)?);
        }

        Ok(())
    }

    /// Display notes in text format
    fn display_notes_text(&self, notes: &[Note], detailed: bool) -> Result<()> {
        // Use terminal width for formatting if available
        let term_width = terminal_size::terminal_size()
            .map(|(w, _)| w.0 as usize)
            .unwrap_or(80);

        for (i, note) in notes.iter().enumerate() {
            // Add separator between notes (except before the first)
            if i > 0 {
                println!("{}", "-".repeat(term_width.min(50)));
            }

            // Format created date
            let created_at = note.created_at.format("%Y-%m-%d %H:%M");

            // Print ID, title, and creation date
            println!("ID: {} | Created: {}", note.id, created_at);
            println!("Title: {}", console::style(&note.title).bold());

            // Print tags if any
            if !note.tags.is_empty() {
                let tags = note
                    .tags
                    .iter()
                    .map(|tag| format!("#{}", tag))
                    .collect::<Vec<_>>()
                    .join(" ");

                println!("Tags: {}", console::style(tags).cyan());
            }

            // Print content preview or full content based on detailed flag
            if detailed {
                println!("\n{}", note.content);
            } else {
                // Get a content preview (first line or first N characters)
                let preview = self.get_content_preview(&note.content, 100);
                if !preview.is_empty() {
                    println!("\n{}", preview);
                }
            }
        }

        Ok(())
    }

    /// Generate a content preview for displaying brief notes
    fn get_content_preview(&self, content: &str, max_len: usize) -> String {
        // Get first non-empty line
        let first_line = content
            .lines()
            .find(|line| !line.trim().is_empty())
            .unwrap_or("");

        if first_line.len() <= max_len {
            first_line.to_string()
        } else {
            format!("{}...", &first_line[..max_len])
        }
    }

    async fn handle_search(
        &self,
        query: String,
        limit: usize,
        format: String,
        include_content: bool,
    ) -> Result<()> {
        // Validate format
        let format = format.to_lowercase();
        if !["text", "json"].contains(&format.as_str()) {
            return Err(KbError::InvalidFormat {
                message: format!("Invalid format: {}. Must be one of: text, json", format),
            });
        }

        // Perform the search
        let mut results = self.note_storage.lock().await.clone().search_notes(&query);

        // Apply limit if specified (0 means no limit)
        if limit > 0 && results.len() > limit {
            results = results.into_iter().take(limit).collect();
        }

        // Display results according to format
        match format.as_str() {
            "json" => self.display_notes_json(&results, include_content)?,
            _ => self.display_notes_text(&results, include_content)?,
        }

        // Report total count
        if !results.is_empty() {
            if limit > 0 && results.len() == limit {
                println!(
                    "\nShowing {} of many matching results. Use --limit to show more.",
                    results.len()
                );
            } else {
                println!("\nFound {} matching notes.", results.len());
            }
        } else {
            println!("No notes found matching query: \"{}\"", query);
        }

        Ok(())
    }

    async fn handle_edit(&self, options: EditNoteOptions) -> Result<()> {
        // Validate input - check for conflicting options
        if options.content.is_some() && options.file.is_some() {
            return Err(KbError::ApplicationError {
                message: "Cannot specify both --content and --file options".to_string(),
            });
        }

        if options.content.is_some() && options.open_editor {
            return Err(KbError::ApplicationError {
                message: "Cannot specify both --content and --edit options".to_string(),
            });
        }

        if options.file.is_some() && options.open_editor {
            return Err(KbError::ApplicationError {
                message: "Cannot specify both --file and --edit options".to_string(),
            });
        }

        // Retrieve the existing note
        let mut note = self
            .note_storage
            .lock()
            .await
            .clone()
            .get_note(&options.id)
            .unwrap();

        // Update title if provided
        if let Some(new_title) = options.title {
            note.title = new_title;
        }

        // Handle content updates
        if let Some(new_content) = options.content {
            // Direct content update
            note.content = new_content;
        } else if let Some(file_path) = options.file {
            // Read content from file
            note.content = self.read_content_from_file(&file_path)?;

            /*     // Store file path in metadata
            note.metadata
                .insert("source_file".to_string(), &file_path.clone()); */
            println!("Content updated from file: {}", file_path);
        } else if options.open_editor {
            // Open the editor with existing content
            note.content = self.open_editor_with_content(&note.title, &note.content)?;
            println!("Content updated from editor");
        }

        // Handle tag updates
        if let Some(tags_to_add) = options.add_tags {
            let new_tags = tags_to_add
                .split(',')
                .map(|t| t.trim().to_string())
                .filter(|t| !t.is_empty())
                .collect::<Vec<String>>();

            // Add only tags that don't already exist
            for tag in new_tags {
                if !note.tags.contains(&tag) {
                    note.tags.push(tag);
                }
            }
        }

        if let Some(tags_to_remove) = options.remove_tags {
            let remove = tags_to_remove
                .split(',')
                .map(|t| t.trim().to_string())
                .filter(|t| !t.is_empty())
                .collect::<Vec<String>>();

            // Remove specified tags
            note.tags.retain(|tag| !remove.contains(tag));
        }

        // Update the note's last modified time
        note.updated_at = chrono::Utc::now();

        // Save the updated note
        self.note_storage.lock().await.update_note(note.clone())?;

        println!("Note {} updated successfully", note.id);

        Ok(())
    }

    // Helper function for reading content from file (reuse from create command)
    fn read_content_from_file(&self, file_path: &str) -> Result<String> {
        let path = Path::new(file_path);

        if !path.exists() {
            return Err(KbError::ApplicationError {
                message: format!("File not found: {}", file_path),
            });
        }

        if !path.is_file() {
            return Err(KbError::ApplicationError {
                message: format!("Not a file: {}", file_path),
            });
        }

        read_to_string(path).map_err(KbError::Io)
    }

    // Helper function to open editor with existing content
    fn open_editor_with_content(&self, title: &str, existing_content: &str) -> Result<String> {
        // Create a temporary file for editing
        // let mut temp_file = tempfile::NamedTempFile::new()
        //     .map_err(KbError::Io);
        let temp_file = Builder::new().suffix(".md").tempfile()?;
        let temp_path = temp_file.path().to_path_buf();

        let mut temp_file = OpenOptions::new().write(true).open(&temp_path)?;

        // Write existing content to the file
        writeln!(temp_file, "# {}", title)?;
        writeln!(temp_file, "<!-- Edit your note below this line -->")?;
        writeln!(temp_file, "\n{}", existing_content)?;

        // Get editor command from config, or use default
        let editor_cmd = self
            .config
            .editor_command
            .clone()
            .unwrap_or_else(|| "nano".to_string());

        // Build and execute the editor command
        let status = std::process::Command::new(&editor_cmd)
            .arg(temp_path.clone())
            .status()
            .map_err(|e| KbError::ApplicationError {
                message: format!("Failed to execute editor command: {}", e),
            })?;

        if !status.success() {
            return Err(KbError::ApplicationError {
                message: "Editor exited with non-zero status".to_string(),
            });
        }

        // Read the updated content from the temp file
        let content = read_to_string(&temp_path).map_err(KbError::Io)?;

        Ok(content)
    }

    async fn handle_delete(&self, id: String, force: bool) -> Result<()> {
        // Step 1: Fetch the note to be deleted (to verify it exists and show details in the prompt)
        let note = match self.note_storage.lock().await.get_note(&id) {
            Some(note) => note,
            _ => {
                return Err(KbError::NoteNotFound { id });
            }
        };

        // Step 2: Show note details and prompt for confirmation (unless force flag is set)
        if !force {
            println!("You are about to delete the following note:");
            println!("ID:     {}", note.id);
            println!("Title:  {}", note.title);
            println!("Tags:   {}", note.tags.join(", "));
            println!("Created: {}", note.created_at.format("%Y-%m-%d %H:%M:%S"));

            // Show content preview (first line or two)
            if !note.content.is_empty() {
                let preview = note.content.lines().take(2).collect::<Vec<_>>().join("\n");

                println!("\nContent preview:");
                println!(
                    "{}{}",
                    preview,
                    if note.content.lines().count() > 2 {
                        "..."
                    } else {
                        ""
                    }
                );
            }

            // Ask for confirmation
            println!("\nThis action cannot be undone!");
            print!("Are you sure you want to delete this note? [y/N]: ");
            stdout().flush().map_err(KbError::Io)?;

            // Read user input
            let mut input = String::new();
            stdin().read_line(&mut input).map_err(KbError::Io)?;

            // Check if user confirmed
            let input = input.trim().to_lowercase();
            if input != "y" && input != "yes" {
                println!("Deletion cancelled.");
                return Ok(());
            }
        }

        // Step 3: Delete the note
        self.note_storage.lock().await.delete_note(&id)?;

        // Step 4: Provide feedback
        println!(
            "Note '{}' ({}) has been permanently deleted.",
            note.title, note.id
        );

        Ok(())
    }

    /// Handle importing notes from external sources
    fn handle_import(
        &self,
        path: String,
        format: String,
        tags: Option<String>,
        title_from_filename: bool,
        recursive: bool,
        pattern: Option<String>,
        verbose: bool,
    ) -> Result<()> {
        // Parse tags from comma-separated string
        let parsed_tags = tags
            .map(|t| {
                t.split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect::<Vec<String>>()
            })
            .unwrap_or_else(Vec::new);

        // Normalize format string
        let format = format.to_lowercase();
        let format = match format.as_str() {
            "md" => "markdown",
            "txt" => "text",
            f => f,
        };

        // Get the path
        let path = PathBuf::from(&path);

        // Import statistics
        let mut total_files = 0;
        let mut imported_notes = 0;
        let mut failed_imports = 0;

        // Process based on whether it's a file or directory
        if path.is_file() {
            if verbose {
                println!("Importing file: {}", path.display());
            }

            // Import a single file
            match self.import_file(&path, &format, &parsed_tags, title_from_filename) {
                Ok(note_id) => {
                    imported_notes += 1;
                    println!("Imported note with ID: {}", note_id);
                }
                Err(e) => {
                    failed_imports += 1;
                    eprintln!("Failed to import {}: {}", path.display(), e);
                }
            }

            total_files = 1;
        } else if path.is_dir() {
            // Compile the pattern if provided
            let pattern_matcher = pattern
                .map(|p| {
                    globset::GlobBuilder::new(&p)
                        .case_insensitive(true)
                        .build()
                        .map_err(|e| KbError::ValidationFailed(format!("Invalid pattern: {}", e)))
                        .and_then(|glob| Ok(globset::GlobSet::new(&[glob])?))
                })
                .transpose()?;

            // Walk the directory
            let mut entries = Vec::new();
            if recursive {
                // Use walkdir for recursive traversal
                for entry in walkdir::WalkDir::new(&path) {
                    match entry {
                        Ok(entry) if entry.file_type().is_file() => {
                            entries.push(entry.path().to_path_buf());
                        }
                        Ok(_) => {} // Skip directories
                        Err(e) => {
                            if verbose {
                                eprintln!("Error accessing path: {}", e);
                            }
                        }
                    }
                }
            } else {
                // Non-recursive, just list direct children
                if let Ok(dir_entries) = std::fs::read_dir(&path) {
                    for entry in dir_entries {
                        if let Ok(entry) = entry {
                            let path = entry.path();
                            if path.is_file() {
                                entries.push(path);
                            }
                        }
                    }
                }
            }

            // Filter by pattern if needed
            let filtered_entries = if let Some(matcher) = &pattern_matcher {
                entries
                    .into_iter()
                    .filter(|p| matcher.is_match(p))
                    .collect::<Vec<_>>()
            } else {
                entries
            };

            total_files = filtered_entries.len();

            if verbose {
                println!("Found {} matching files", total_files);
            }

            // Import each file
            for file_path in filtered_entries {
                if verbose {
                    println!("Importing: {}", file_path.display());
                }

                match self.import_file(&file_path, &format, &parsed_tags, title_from_filename) {
                    Ok(note_id) => {
                        imported_notes += 1;
                        if verbose {
                            println!("Imported as note ID: {}", note_id);
                        }
                    }
                    Err(e) => {
                        failed_imports += 1;
                        eprintln!("Failed to import {}: {}", file_path.display(), e);
                    }
                }
            }
        } else {
            return Err(KbError::ValidationFailed(format!(
                "Path not found: {}",
                path.display()
            )));
        }

        // Show summary
        println!("\nImport summary:");
        println!("  Total files processed: {}", total_files);
        println!("  Successfully imported: {}", imported_notes);
        println!("  Failed imports: {}", failed_imports);

        Ok(())
    }

    /// Import a single file as a note
    fn import_file(
        &self,
        path: &PathBuf,
        format: &str,
        tags: &[String],
        title_from_filename: bool,
    ) -> Result<String> {
        // Read the file content
        let content = std::fs::read_to_string(path).map_err(|e| {
            KbError::ValidationFailed(format!("Failed to read file {}: {}", path.display(), e))
        })?;

        // Determine the title
        let title = if title_from_filename {
            path.file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("Unnamed Note")
                .to_string()
        } else {
            // Try to extract title from content based on format
            match format {
                "markdown" => {
                    // Look for a markdown H1 heading (# Title)
                    let first_line = content.lines().next().unwrap_or("");
                    if first_line.starts_with("# ") {
                        first_line[2..].trim().to_string()
                    } else {
                        path.file_name()
                            .and_then(|s| s.to_str())
                            .unwrap_or("Unnamed Note")
                            .to_string()
                    }
                }
                "json" => {
                    // For JSON files, we'll handle differently in the parse_note_from_json function
                    path.file_name()
                        .and_then(|s| s.to_str())
                        .unwrap_or("Unnamed Note")
                        .to_string()
                }
                _ => {
                    // For other formats, use filename
                    path.file_name()
                        .and_then(|s| s.to_str())
                        .unwrap_or("Unnamed Note")
                        .to_string()
                }
            }
        };

        // Process content based on format
        match format {
            "markdown" => self.import_markdown_note(title, content, tags, path),
            "json" => self.import_json_note(content, tags, path),
            "text" => self.import_text_note(title, content, tags, path),
            _ => Err(KbError::ValidationFailed(format!(
                "Unsupported format: {}",
                format
            ))),
        }
    }

    /// Import a markdown note
    fn import_markdown_note(
        &self,
        title: String,
        content: String,
        tags: &[String],
        source_path: &PathBuf,
    ) -> Result<String> {
        // Create note with the provided content
        let mut note = Note::new(title, content, tags.to_vec());

        // Add metadata
        note.metadata
            .insert("source_file".to_string(), source_path.display().to_string());
        note.metadata
            .insert("import_format".to_string(), "markdown".to_string());
        note.metadata
            .insert("imported_at".to_string(), Utc::now().to_rfc3339());

        // Save the note
        self.runtime
            .block_on(async { self.storage.save_note(&note).await })?;

        Ok(note.id)
    }

    /// Import a JSON formatted note
    fn import_json_note(
        &self,
        content: String,
        extra_tags: &[String],
        source_path: &PathBuf,
    ) -> Result<String> {
        // Parse JSON
        let json: serde_json::Value = serde_json::from_str(&content)
            .map_err(|e| KbError::ValidationFailed(format!("Invalid JSON: {}", e)))?;

        // Extract note fields
        let title = json
            .get("title")
            .and_then(|v| v.as_str())
            .ok_or_else(|| KbError::ValidationFailed("JSON missing 'title' field".to_string()))?
            .to_string();

        let content = json
            .get("content")
            .and_then(|v| v.as_str())
            .ok_or_else(|| KbError::ValidationFailed("JSON missing 'content' field".to_string()))?
            .to_string();

        // Extract tags if present and merge with extra_tags
        let mut tags = extra_tags.to_vec();
        if let Some(json_tags) = json.get("tags").and_then(|v| v.as_array()) {
            for tag_value in json_tags {
                if let Some(tag) = tag_value.as_str() {
                    if !tag.is_empty() && !tags.contains(&tag.to_string()) {
                        tags.push(tag.to_string());
                    }
                }
            }
        }

        // Create the note
        let mut note = Note::new(title, content, tags);

        // Add metadata
        note.metadata
            .insert("source_file".to_string(), source_path.display().to_string());
        note.metadata
            .insert("import_format".to_string(), "json".to_string());
        note.metadata
            .insert("imported_at".to_string(), Utc::now().to_rfc3339());

        // Copy additional fields as metadata
        for (key, value) in json.as_object().unwrap_or(&serde_json::Map::new()) {
            // Skip fields we've already processed
            if !["title", "content", "tags"].contains(&key.as_str()) {
                if let Some(str_value) = value.as_str() {
                    note.metadata.insert(key.clone(), str_value.to_string());
                } else {
                    // For non-string values, convert to string representation
                    note.metadata.insert(key.clone(), value.to_string());
                }
            }
        }

        // Save the note
        self.runtime
            .block_on(async { self.storage.save_note(&note).await })?;

        Ok(note.id)
    }

    /// Import a plain text note
    fn import_text_note(
        &self,
        title: String,
        content: String,
        tags: &[String],
        source_path: &PathBuf,
    ) -> Result<String> {
        // Create note with the provided content
        let mut note = Note::new(title, content, tags.to_vec());

        // Add metadata
        note.metadata
            .insert("source_file".to_string(), source_path.display().to_string());
        note.metadata
            .insert("import_format".to_string(), "text".to_string());
        note.metadata
            .insert("imported_at".to_string(), Utc::now().to_rfc3339());

        // Save the note
        self.runtime
            .block_on(async { self.storage.save_note(&note).await })?;

        Ok(note.id)
    }
}
