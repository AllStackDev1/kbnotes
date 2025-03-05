use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use chrono::Utc;
use log::{debug, error, info, trace, warn};
use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use tempfile::NamedTempFile;
use walkdir::WalkDir;

use crate::error::{KbError, Result};
use crate::types::{Config, Note};

/// Manages the storage, retrieval, and synchronization of notes.
pub struct NoteStorage {
    /// Application configuration
    config: Config,

    /// In-memory cache of notes, indexed by note ID
    notes_cache: Arc<Mutex<HashMap<String, Note>>>,

    /// File system watcher to detect changes to note files
    watcher: Option<RecommendedWatcher>,

    /// Flag indicating if the storage system is ready
    initialized: bool,
}

impl NoteStorage {
    /// Creates a new NoteStorage instance with the provided configuration.
    ///
    /// This constructor:
    /// 1. Ensures the notes and backup directories exist
    /// 2. Initializes an empty in-memory cache
    /// 3. Sets up the file system watcher (but doesn't start it yet)
    /// 4. Loads existing notes from the filesystem
    ///
    /// # Arguments
    ///
    /// * `config` - The configuration for the storage system
    ///
    /// # Returns
    ///
    /// A Result containing the new NoteStorage instance or an error
    pub fn new(config: Config) -> Result<Self> {
        info!(
            "Initializing NoteStorage with config: notes_dir={}, backup_dir={}",
            config.notes_dir.display(),
            config.backup_dir.display()
        );

        // Ensure notes directory exists
        if !config.notes_dir.exists() {
            debug!(
                "Notes directory does not exist, creating: {}",
                config.notes_dir.display()
            );
            fs::create_dir_all(&config.notes_dir).map_err(|e| {
                error!("Failed to create notes directory: {}", e);
                KbError::DirectoryError {
                    path: config.notes_dir.clone(),
                }
            })?;
        }

        // Ensure backup directory exists
        if !config.backup_dir.exists() {
            debug!(
                "Backup directory does not exist, creating: {}",
                config.backup_dir.display()
            );
            fs::create_dir_all(&config.backup_dir).map_err(|e| {
                error!("Failed to create backup directory: {}", e);
                KbError::DirectoryError {
                    path: config.backup_dir.clone(),
                }
            })?;
        }

        // Initialize empty notes cache
        let notes_cache = Arc::new(Mutex::new(HashMap::new()));

        // Create the storage instance
        let mut storage = NoteStorage {
            config,
            notes_cache,
            watcher: None,
            initialized: false,
        };

        // Load existing notes
        debug!("Loading notes into storage");
        storage.load_notes()?;

        // Mark as initialized
        storage.initialized = true;
        info!("NoteStorage initialization complete");

        Ok(storage)
    }

    /// Loads all existing notes from the file system into the in-memory cache.
    ///
    /// This method:
    /// 1. Scans the notes directory for JSON files
    /// 2. Deserializes each file into a Note object
    /// 3. Populates the in-memory cache with the notes
    ///
    /// # Returns
    ///
    /// A Result indicating success or an error
    pub fn load_notes(&mut self) -> Result<()> {
        info!("Loading notes from: {}", self.config.notes_dir.display());

        // Get an exclusive lock on the cache
        let mut cache = self.notes_cache.lock().map_err(|e| {
            error!("Failed to acquire lock on notes cache: {}", e);
            KbError::ApplicationError {
                message: "Failed to acquire lock on notes cache".to_string(),
            }
        })?;

        cache.clear();
        debug!("Cache cleared, starting to load notes");

        // Use walkdir instead of manual recursion
        let mut loaded_count = 0;
        let mut error_count = 0;

        // Use walkdir instead of manual recursion
        for entry in WalkDir::new(&self.config.notes_dir)
            .follow_links(true)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();

            // Only process files (not directories)
            if path.is_file() {
                if let Some(extension) = path.extension() {
                    if extension == "json" {
                        match self.load_note_from_file(path) {
                            Ok(note) => {
                                cache.insert(note.id.clone(), note);
                                loaded_count += 1;
                            }
                            Err(e) => {
                                eprintln!("Error loading note from {}: {}", path.display(), e);
                                error_count += 1;
                            }
                        }
                    }
                }
            }
        }

        info!(
            "Loaded {} notes into cache ({} errors)",
            loaded_count, error_count
        );

        Ok(())
    }

    /// Loads a single note from a file.
    ///
    /// # Arguments
    ///
    /// * `file_path` - The path to the note file
    ///
    /// # Returns
    ///
    /// A Result containing the Note or an error
    fn load_note_from_file(&self, file_path: &Path) -> Result<Note> {
        debug!("Loading note from file: {}", file_path.display());

        // Open the file
        let file = File::open(file_path).map_err(|e| {
            error!("Failed to open note file {}: {}", file_path.display(), e);
            KbError::Io(e)
        })?;

        // Create a buffered reader
        let reader = BufReader::new(file);

        // Deserialize the JSON into a Note
        let note: Note = serde_json::from_reader(reader).map_err(|e| {
            error!("Failed to parse note file {}: {}", file_path.display(), e);
            KbError::InvalidFormat {
                message: format!("Failed to parse note file {}: {}", file_path.display(), e),
            }
        })?;

        trace!("Successfully loaded note: {}", note.id);
        Ok(note)
    }

    /// Saves a note to storage using atomic operations to prevent data corruption
    pub fn save_note(&self, note: &Note) -> Result<()> {
        info!("Saving note: {}", note.id);

        // Generate the file path based on the note id
        let file_path = self.get_note_path(&note.id);
        debug!("File path for note: {}", file_path.display());

        // Ensure the parent directory exists
        if let Some(parent) = file_path.parent() {
            if !parent.exists() {
                debug!("Creating parent directory: {}", parent.display());
                fs::create_dir_all(parent).map_err(|e| {
                    error!("Failed to create directory {}: {}", parent.display(), e);
                    KbError::Io(e)
                })?;
            }
        }

        // Create a temporary file in the same directory (for atomic operation)
        let dir = file_path.parent().unwrap_or_else(|| Path::new("."));
        debug!("Creating temporary file in directory: {}", dir.display());
        let mut temp_file = NamedTempFile::new_in(dir).map_err(|e| {
            error!("Failed to create temporary file: {}", e);
            KbError::Io(e)
        })?;

        // Serialize the note to JSON
        trace!("Serializing note to JSON");
        let json = serde_json::to_string_pretty(note).map_err(|e| {
            error!("Failed to serialize note: {}", e);
            KbError::Serialization(e)
        })?;

        // Write to the temporary file
        trace!("Writing to temporary file");
        temp_file.write_all(json.as_bytes()).map_err(|e| {
            error!("Failed to write to temporary file: {}", e);
            KbError::Io(e)
        })?;

        temp_file.flush().map_err(|e| {
            error!("Failed to flush temporary file: {}", e);
            KbError::Io(e)
        })?;

        // Atomically move the temporary file to the target location
        debug!("Performing atomic move of temporary file to final location");
        temp_file.persist(&file_path).map_err(|e| {
            error!(
                "Failed to persist file {}: {}",
                file_path.display(),
                e.error
            );
            KbError::Io(e.error)
        })?;

        // If we're initialized, update the cache as well
        if self.initialized {
            debug!("Updating note in cache");
            match self.notes_cache.lock() {
                Ok(mut cache) => {
                    cache.insert(note.id.clone(), note.clone());
                    trace!("Cache updated successfully");
                }
                Err(e) => {
                    warn!("Failed to acquire lock for cache update: {}", e);
                    // Continue since the file is saved already
                }
            }
        }

        // Create a backup if auto_backup is enabled
        if self.config.auto_backup {
            debug!("Creating backup of note (auto_backup enabled)");
            match self.backup_note(note) {
                Ok(_) => trace!("Backup created successfully"),
                Err(e) => warn!("Failed to create backup: {}", e),
            }
        }

        info!("Note saved successfully: {}", note.id);
        Ok(())
    }

    /// Helper method to get the file path for a note
    fn get_note_path(&self, note_id: &str) -> PathBuf {
        // Create path with structure: notes_dir/first_2_chars_of_id/note_id.json
        let id_prefix = if note_id.len() >= 2 {
            &note_id[0..2]
        } else {
            note_id
        };

        self.config
            .notes_dir
            .join(id_prefix)
            .join(format!("{}.json", note_id))
    }

    /// Creates a backup of the note in the backup directory
    fn backup_note(&self, note: &Note) -> Result<()> {
        debug!("Creating backup for note: {}", note.id);
        // Create a timestamped backup path
        let timestamp = Utc::now().timestamp();

        let backup_path = self
            .config
            .backup_dir
            .join(format!("{}_{}.json", note.id, timestamp));

        debug!("Backup path: {}", backup_path.display());

        // Ensure backup directory exists
        if !self.config.backup_dir.exists() {
            debug!(
                "Creating backup directory: {}",
                self.config.backup_dir.display()
            );
            fs::create_dir_all(&self.config.backup_dir).map_err(|e| {
                error!("Failed to create backup directory: {}", e);
                KbError::Io(e)
            })?;
        }

        // Write the note to the backup file
        trace!("Serializing note for backup");
        let json = serde_json::to_string_pretty(note).map_err(|e| {
            error!("Failed to serialize note for backup: {}", e);
            KbError::Serialization(e)
        })?;

        trace!("Writing backup file");
        fs::write(&backup_path, json).map_err(|e| {
            error!(
                "Failed to write backup file {}: {}",
                backup_path.display(),
                e
            );
            KbError::Io(e)
        })?;

        info!("Backup created successfully at: {}", backup_path.display());
        Ok(())
    }

    /// Retrieves a note by its ID from the storage
    /// Returns Some(Note) if found, or None if not found
    pub fn get_note(&self, note_id: &str) -> Option<Note> {
        debug!("Retrieving note by ID: {}", note_id);

        // First, try to get from cache
        match self.notes_cache.lock() {
            Ok(cache) => {
                // If found in cache, clone and return it
                if let Some(note) = cache.get(note_id) {
                    trace!("Note found in cache: {}", note_id);
                    return Some(note.clone());
                }
            }
            Err(e) => {
                error!("Failed to acquire lock on cache: {}", e);
                // Fall through to file system check
            }
        }

        // Not found in cache or couldn't access cache, try to load from disk
        debug!("Note not found in cache, checking file system: {}", note_id);
        let file_path = self.get_note_path(note_id);

        if file_path.exists() {
            debug!("Note file exists at: {}", file_path.display());
            match self.load_note_from_file(&file_path) {
                Ok(note) => {
                    // Update cache with the found note
                    if let Ok(mut cache) = self.notes_cache.lock() {
                        trace!("Updating cache with note loaded from disk");
                        cache.insert(note_id.to_string(), note.clone());
                    } else {
                        warn!("Failed to acquire lock to update cache");
                    }
                    return Some(note);
                }
                Err(e) => {
                    error!("Error loading note from file: {}", e);
                    return None;
                }
            }
        }

        // Not found
        debug!("Note not found: {}", note_id);
        None
    }

    pub fn get_notes_by_tag(&self, tag: &str) -> Vec<Note> {
        info!("Retrieving notes by tag: {}", tag);

        // Use match for explicit error handling on mutex lock
        match self.notes_cache.lock() {
            Ok(cache) => {
                debug!("Searching through {} notes in cache", cache.len());
                // Use iterator with filter to collect matching notes
                let matching_notes: Vec<Note> = cache
                    .values()
                    .filter(|note| note.tags.iter().any(|t| t == tag))
                    .cloned()
                    .collect();

                info!("Found {} notes with tag: {}", matching_notes.len(), tag);
                matching_notes
            }
            Err(err) => {
                error!("Failed to acquire lock on notes cache: {}", err);
                // Return empty vector in case of lock failure
                Vec::new()
            }
        }
    }

    /// Searches notes by title and content using fuzzy matching
    /// Returns a Vec of Notes sorted by relevance score
    pub fn search_notes(&self, query: &str) -> Vec<Note> {
        use fuzzy_matcher::skim::SkimMatcherV2;
        use fuzzy_matcher::FuzzyMatcher;

        info!("Searching notes with query: '{}'", query);

        // Create a fuzzy matcher with default options
        let matcher = SkimMatcherV2::default();

        // Structure to hold note and its relevance score
        struct ScoredNote {
            note: Note,
            score: i64,
        }

        match self.notes_cache.lock() {
            Ok(cache) => {
                debug!("Searching through {} notes in cache", cache.len());
                let mut matched_notes: Vec<ScoredNote> = Vec::new();

                // Iterate through all notes in the cache
                for note in cache.values() {
                    trace!("Checking note: {}", note.id);

                    // Try to match against title first (higher priority)
                    let title_score = matcher.fuzzy_match(&note.title, query).unwrap_or(0);

                    // Try to match against content
                    let content_score = matcher.fuzzy_match(&note.content, query).unwrap_or(0);

                    // Calculate final score - title matches are weighted more heavily
                    let final_score = title_score * 2 + content_score;

                    // If we have any match at all, include this note
                    if final_score > 0 {
                        trace!("Note matched with score {}: {}", final_score, note.id);
                        matched_notes.push(ScoredNote {
                            note: note.clone(),
                            score: final_score,
                        });
                    }
                }

                debug!(
                    "Found {} matching notes before sorting",
                    matched_notes.len()
                );

                // Sort matched notes by score (highest first)
                matched_notes.sort_by(|a, b| {
                    // Reverse ordering to get highest scores first
                    b.score.cmp(&a.score)
                });

                // Extract just the notes in sorted order
                let result: Vec<Note> = matched_notes
                    .into_iter()
                    .map(|scored| scored.note)
                    .collect();

                info!("Returning {} sorted search results", result.len());
                result
            }
            Err(err) => {
                error!(
                    "Failed to acquire lock on notes cache during search: {}",
                    err
                );
                Vec::new()
            }
        }
    }

    /// Initializes the file system watcher.
    ///
    /// This method sets up the watcher to monitor the notes directory
    /// for changes to note files.
    ///
    /// # Returns
    ///
    /// A Result indicating success or an error
    fn init_watcher(&mut self) -> Result<()> {
        info!(
            "Initializing file system watcher for: {}",
            self.config.notes_dir.display()
        );

        let mut watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
            match res {
                Ok(event) => {
                    debug!("File system event detected: {:?}", event);
                    // File system event occurred
                    // We'll process it in a future implementation
                }
                Err(e) => {
                    error!("Error in file watcher: {}", e);
                }
            }
        })
        .map_err(|e| {
            error!("Failed to create file watcher: {}", e);
            KbError::ApplicationError {
                message: format!("Failed to create file watcher: {}", e),
            }
        })?;

        debug!("Setting up recursive watching on notes directory");
        watcher
            .watch(&self.config.notes_dir, RecursiveMode::Recursive)
            .map_err(|e| {
                error!("Failed to watch notes directory: {}", e);
                KbError::ApplicationError {
                    message: format!("Failed to watch notes directory: {}", e),
                }
            })?;

        // Store the watcher
        self.watcher = Some(watcher);
        info!("File system watcher successfully initialized");

        Ok(())
    }
}
