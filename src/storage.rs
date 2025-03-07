use std::cmp::{Ordering, Reverse};
use std::collections::{BinaryHeap, HashMap, HashSet};
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use chrono::{DateTime, Utc};
use log::{debug, error, info, trace, warn};
use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use tempfile::NamedTempFile;
use tokio::sync::Mutex as TokioMutex;
use walkdir::WalkDir;
use zip::{write::FileOptions, ZipArchive, ZipWriter};

use crate::backup_scheduler::{BackupScheduler, BackupSchedulerStatus};
use crate::errors::{KbError, Result};
use crate::storage;
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

    /// Backup scheduler for automated backups
    backup_scheduler: Arc<TokioMutex<BackupScheduler>>,
}

/// Summary of a backup restoration operation
#[derive(Debug, Clone)]
pub struct RestoreBackupSummary {
    /// Path to the backup file that was restored
    pub backup_file: PathBuf,
    /// Total number of notes found in the backup
    pub total_notes: usize,
    /// Number of notes successfully restored
    pub notes_restored: usize,
    /// Number of notes skipped (e.g., due to existing notes with overwrite disabled)
    pub notes_skipped: usize,
    /// Details about notes that failed to restore
    pub failed_notes: Vec<(String, String)>, // (note_id, error_message)
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
    pub fn new(config: Config) -> Self {
        // Initialize empty notes cache
        let notes_cache = Arc::new(Mutex::new(HashMap::new()));

        // Initialize scheduler
        let backup_scheduler = BackupScheduler::new(config.clone());

        // Create the storage instance
        Self {
            config,
            notes_cache,
            watcher: None,
            initialized: false,
            backup_scheduler: Arc::new(TokioMutex::new(backup_scheduler)),
        }
    }

    /// Initializes the storage system, loading notes and starting backup scheduler
    pub async fn initialize(&mut self, storage: Arc<TokioMutex<NoteStorage>>) -> Result<()> {
        let mut storage_lock = storage.lock().await;

        info!(
            "Initializing NoteStorage with config: notes_dir={}, backup_dir={}",
            storage_lock.config.notes_dir.display(),
            storage_lock.config.backup_dir.display()
        );

        // Ensure notes directory exists
        if !storage_lock.config.notes_dir.exists() {
            debug!(
                "Notes directory does not exist, creating: {}",
                storage_lock.config.notes_dir.display()
            );
            fs::create_dir_all(&storage_lock.config.notes_dir).map_err(|e| {
                error!("Failed to create notes directory: {}", e);
                KbError::DirectoryError {
                    path: storage_lock.config.notes_dir.clone(),
                }
            })?;
        }

        // Ensure backup directory exists
        if !storage_lock.config.backup_dir.exists() {
            debug!(
                "Backup directory does not exist, creating: {}",
                storage_lock.config.backup_dir.display()
            );
            fs::create_dir_all(&storage_lock.config.backup_dir).map_err(|e| {
                error!("Failed to create backup directory: {}", e);
                KbError::DirectoryError {
                    path: storage_lock.config.backup_dir.clone(),
                }
            })?;
        }

        // Load existing notes into cache
        debug!("Loading notes into storage");
        storage_lock.load_notes()?;
        info!("Loaded notes successfully");

        // Mark as initialized
        storage_lock.initialized = true;

        // Start the backup scheduler
        let mut scheduler = storage_lock.backup_scheduler.lock().await;
        match scheduler.start(storage.clone()).await {
            Ok(_) => info!("Backup scheduler started successfully"),
            Err(e) => error!("Failed to start backup scheduler: {}", e),
        }

        info!("NoteStorage initialization complete");

        Ok(())
    }

    /// Loads all notes from disk into the in-memory cache
    ///
    /// # Returns
    ///
    /// The number of notes loaded in case of success or an error
    pub fn load_notes(&mut self) -> Result<usize> {
        // Ensure notes directory exists
        if !self.config.notes_dir.exists() {
            fs::create_dir_all(&self.config.notes_dir).map_err(|e| KbError::Io(e))?;
            info!(
                "Created notes directory: {}",
                self.config.notes_dir.display()
            );
            return Ok(0); // No notes to load from an empty directory
        }

        // Pre-allocate a HashMap to hold all notes before acquiring the lock
        let mut notes_buffer = HashMap::with_capacity(100); // Initial capacity estimation
        let mut load_errors = Vec::new();

        // Walk the notes directory and load all notes
        for entry in WalkDir::new(&self.config.notes_dir)
            .min_depth(1) // Skip the root directory
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();

            // Only process JSON files
            if path.is_file() && path.extension().is_some_and(|ext| ext == "json") {
                match self.load_note_from_file(path) {
                    Ok(note) => {
                        // Add to our temporary buffer instead of directly to cache
                        notes_buffer.insert(note.id.clone(), note);
                    }
                    Err(e) => {
                        // Collect errors but continue processing
                        let error_msg =
                            format!("Failed to load note from {}: {}", path.display(), e);
                        warn!("{}", error_msg);
                        load_errors.push((path.to_path_buf(), error_msg));
                    }
                }
            }
        }

        let notes_count = notes_buffer.len();

        // Now acquire the lock only once to update the cache with all loaded notes
        if notes_count > 0 {
            // Minimize time holding the lock by using a single batch operation
            match self.notes_cache.lock() {
                Ok(mut cache) => {
                    // Use extend to efficiently add all items at once
                    cache.clear(); // Clear existing cache
                    cache.reserve(notes_count); // Pre-allocate capacity
                    cache.extend(notes_buffer);

                    info!("Loaded {} notes into cache", notes_count);
                }
                Err(_) => {
                    return Err(KbError::LockAcquisitionFailed {
                        message: "Failed to acquire lock on notes cache during load operation"
                            .to_string(),
                    });
                }
            }
        }

        // Handle any load errors
        if !load_errors.is_empty() {
            error!(
                "Encountered {} errors while loading notes",
                load_errors.len()
            );
            // Could return errors as part of a more detailed result if needed
        }

        self.initialized = true;
        Ok(notes_count)
    }

    /// Helper method to load a single note from file
    fn load_note_from_file(&self, path: &Path) -> Result<Note> {
        debug!("Loading note from file: {}", path.display());
        let content = fs::read_to_string(path).map_err(|e| {
            error!("Failed to open note file {}: {}", path.display(), e);
            KbError::Io(e)
        })?;

        let note: Note = serde_json::from_str(&content)?;

        // Validate note
        if note.id.is_empty() {
            let error_mgs = format!("Note from {} has an empty ID", path.display());
            error!("{}", error_mgs);
            return Err(KbError::InvalidFormat { message: error_mgs });
        }

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
                    let error_mgs = format!("Failed to acquire lock for cache update: {}", e);
                    warn!("{}", error_mgs);
                    // KbError::LockAcquisitionFailed {
                    // message: error_mgs
                    // }
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

    /// Restores a single note from its most recent backup
    ///
    /// # Arguments
    ///
    /// * `note_id` - The ID of the note to restore
    ///
    /// # Returns
    ///
    /// The restored note in case of success or an error
    pub fn restore_note_from_backup(&self, note_id: &str) -> Result<Note> {
        // Construct the backup directory path for this note
        let note_backup_dir = self.config.backup_dir.join(note_id);

        if !note_backup_dir.exists() {
            let error = format!("No backup directory found for note {}", note_id);
            error!("{}", error);
            return Err(KbError::BackupFailed { message: error });
        }

        // Find all backup files for this note
        let mut backup_files: Vec<_> = WalkDir::new(&note_backup_dir)
            .max_depth(1)
            .into_iter()
            .filter_map(|entry| entry.ok())
            .filter(|entry| {
                entry.path().is_file()
                    && entry.path().extension().map_or(false, |ext| ext == "json")
            })
            .collect();

        if backup_files.is_empty() {
            let error = format!("No backup files found for note {}", note_id);
            error!("{}", error);
            return Err(KbError::BackupFailed { message: error });
        }

        // Sort backups by modification time (newest first)
        backup_files.sort_by_key(|entry| {
            fs::metadata(entry.path())
                .and_then(|meta| meta.modified())
                .unwrap_or(std::time::SystemTime::UNIX_EPOCH)
        });
        backup_files.reverse(); // Now newest is first

        // Get the most recent backup
        let latest_backup_path = backup_files[0].path();

        // Read and deserialize the backup file
        let backup_content = fs::read_to_string(latest_backup_path).map_err(|e| {
            let error = format!("No backup files found for note {}", note_id);
            error!("{}", error);
            KbError::BackupFailed {
                message: format!(
                    "Failed to read backup file {}: {}",
                    latest_backup_path.display(),
                    e
                ),
            }
        })?;

        let restored_note: Note = serde_json::from_str(&backup_content)?;

        // Save the restored note back to storage
        self.save_note(&restored_note)?;

        // Log the restoration
        let backup_time = fs::metadata(backup_files[0].path())
            .and_then(|meta| meta.modified())
            .map(|time| {
                DateTime::<chrono::Local>::from(time)
                    .format("%Y-%m-%d %H:%M:%S")
                    .to_string()
            })
            .unwrap_or_else(|_| "unknown time".to_string());

        info!(
            "Note {} successfully restored from backup created at {}",
            note_id, backup_time
        );

        Ok(restored_note)
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

    /// Retrieves all notes with a specific tag
    ///
    /// # Arguments
    ///
    /// * `tag` - The tag to search for
    ///
    /// # Returns
    ///
    /// A vector of notes that have the specified tag
    pub fn get_notes_by_tag(&self, tag: &str) -> Result<Vec<Note>> {
        info!("Retrieving notes by tag: {}", tag);

        // Create a normalized version of the tag for comparison
        let search_tag = tag.trim().to_lowercase();

        // Acquire the lock only to clone the required data
        let notes_snapshot = {
            // Scope the lock to this block
            let cache = self
                .notes_cache
                .lock()
                .map_err(|_| KbError::LockAcquisitionFailed {
                    message: "Failed to acquire lock on notes cache".to_string(),
                })?;

            debug!("Searching through {} notes in cache", cache.len());

            // Clone all notes to process outside the lock
            cache.values().cloned().collect::<Vec<Note>>()
        }; // Lock is automatically released here when 'cache' goes out of scope

        // Process the data without holding the lock
        let matching_notes: Vec<Note> = notes_snapshot
            .into_iter()
            .filter(|note| {
                note.tags
                    .iter()
                    .any(|t| t.trim().to_lowercase() == search_tag)
            })
            .collect();

        info!("Found {} notes with tag: {}", matching_notes.len(), tag);
        Ok(matching_notes)
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

    /// Creates a full backup of all notes in a ZIP archive
    ///
    /// # Returns
    ///
    /// The path to the created backup file in case of success or an error
    pub fn create_full_backup(&self) -> Result<PathBuf> {
        // Ensure backup directory exists
        if !self.config.backup_dir.exists() {
            fs::create_dir_all(&self.config.backup_dir).map_err(|e| KbError::BackupFailed {
                message: e.to_string(),
            })?;
        }

        // Generate timestamped filename for the backup
        let timestamp = Utc::now().format("%Y%m%d_%H%M%S");
        let backup_filename = format!("kbnotes_backup_{}.zip", timestamp);
        let backup_path = self.config.backup_dir.join(backup_filename);

        // Create a new ZIP file
        let file = File::create(&backup_path).map_err(|e| KbError::BackupFailed {
            message: e.to_string(),
        })?;

        let mut zip = ZipWriter::new(file);

        // Lock the notes cache for reading
        let notes_cache = self
            .notes_cache
            .lock()
            .map_err(|_| KbError::LockAcquisitionFailed {
                message: "Failed to acquire lock on notes cache".to_string(),
            })?;

        let notes_count = notes_cache.len();

        // Iterate through notes and add each to the ZIP file
        for (id, note) in notes_cache.iter() {
            let options = FileOptions::<zip::write::ExtendedFileOptions>::default()
                .compression_method(zip::CompressionMethod::Deflated)
                .unix_permissions(0o644);

            // Serialize note to JSON - using the existing Serialization error via From trait
            let note_json = serde_json::to_string_pretty(&note)?;

            // Add note to the ZIP with folder structure matching the storage organization
            let folder_name = &id[..2]; // First 2 chars for subdirectory
            let note_path = format!("{}/{}.json", folder_name, id);

            // Start a file in the ZIP archive - using the existing ZipError from #[from] trait
            zip.start_file(note_path, options)?;

            // Write note data to the ZIP file
            zip.write_all(note_json.as_bytes())
                .map_err(|e| KbError::BackupFailed {
                    message: format!("Failed to write note {} content to backup: {}", id, e),
                })?;
        }

        // Finalize the ZIP file
        zip.finish()?;

        // Clean up old backups if exceeding max_backups
        self.cleanup_old_backups()?;

        info!(
            "Full backup created successfully with {} notes at {}",
            notes_count,
            backup_path.display()
        );

        Ok(backup_path)
    }

    /// Removes old backup files if the number of backups exceeds the configured limit
    /// Uses a BinaryHeap for efficient identification of oldest files
    fn cleanup_old_backups(&self) -> Result<()> {
        // If max_backups is 0, keep all backups
        if self.config.max_backups == 0 {
            return Ok(());
        }

        // Custom wrapper to compare backup files by modification time
        #[derive(Debug, Eq)]
        struct BackupFile {
            path: PathBuf,
            modified_time: SystemTime,
        }

        impl PartialEq for BackupFile {
            fn eq(&self, other: &Self) -> bool {
                self.modified_time.eq(&other.modified_time)
            }
        }

        impl PartialOrd for BackupFile {
            fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
                Some(self.cmp(other))
            }
        }

        impl Ord for BackupFile {
            // Compare by modified time (newer files are "greater" than older files)
            fn cmp(&self, other: &Self) -> Ordering {
                self.modified_time.cmp(&other.modified_time)
            }
        }

        // Use a min-heap to keep track of the newest backups
        // By using Reverse, we make this a min-heap where the oldest files are at the top
        let mut newest_backups: BinaryHeap<Reverse<BackupFile>> =
            BinaryHeap::with_capacity((self.config.max_backups + 1) as usize);

        // Find and process all zip backup files in the backup directory
        let mut total_backups = 0;

        for entry in WalkDir::new(&self.config.backup_dir)
            .max_depth(1) // Only look in the immediate backup directory
            .into_iter()
            .filter_map(|entry| entry.ok())
        {
            let path = entry.path();

            // Only consider zip files that match our backup naming pattern
            if path.is_file()
                && path.extension().is_some_and(|ext| ext == "zip")
                && path
                    .file_name()
                    .is_some_and(|name| name.to_string_lossy().starts_with("kbnotes_backup_"))
            {
                // Get file modification time
                if let Ok(metadata) = entry.metadata() {
                    if let Ok(modified_time) = metadata.modified() {
                        total_backups += 1;

                        // Create a BackupFile entry
                        let backup_file = BackupFile {
                            path: path.to_path_buf(),
                            modified_time,
                        };

                        // Add to our min-heap
                        newest_backups.push(Reverse(backup_file));

                        // If we have more than max_backups, remove the oldest one (the top of min-heap)
                        if newest_backups.len() > self.config.max_backups as usize {
                            if let Some(Reverse(oldest)) = newest_backups.pop() {
                                match fs::remove_file(&oldest.path) {
                                    Ok(_) => {
                                        debug!("Removed old backup: {}", oldest.path.display());
                                    }
                                    Err(e) => {
                                        warn!(
                                            "Failed to remove old backup {}: {}",
                                            oldest.path.display(),
                                            e
                                        );
                                        // Continue processing even if we couldn't delete this file
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        let kept = newest_backups.len();
        let removed = total_backups - kept;

        if removed > 0 {
            debug!(
                "Cleanup complete: kept {} backups, removed {} old backups",
                kept, removed
            );
        }

        Ok(())
    }

    /// Get the current backup scheduler status
    pub async fn get_backup_status(&self) -> BackupSchedulerStatus {
        let scheduler = self.backup_scheduler.lock().await;
        scheduler.get_status()
    }

    /// Manually trigger a backup
    pub async fn create_backup_now(&self) -> Result<()> {
        let scheduler = self.backup_scheduler.lock().await;
        scheduler.create_backup_now().await
    }

    /// Stop the backup scheduler
    pub async fn stop_backup_scheduler(&self) -> Result<()> {
        let mut scheduler = self.backup_scheduler.lock().await;
        scheduler.stop().await
    }

    /// Restores all notes from a full backup ZIP archive
    ///
    /// # Arguments
    ///
    /// * `backup_path` - Path to the backup ZIP file to restore from
    /// * `overwrite_existing` - Whether to overwrite existing notes or preserve them
    ///
    /// # Returns
    ///
    /// A summary of the restoration process in case of success or an error
    pub fn restore_full_backup(
        &self,
        backup_path: &Path,
        overwrite_existing: bool,
    ) -> Result<RestoreBackupSummary> {
        // Ensure the backup file exists and is a ZIP file
        if !backup_path.exists() || !backup_path.is_file() {
            return Err(KbError::BackupFailed {
                message: format!("Backup file not found: {}", backup_path.display()),
            });
        }

        if backup_path.extension().map_or(true, |ext| ext != "zip") {
            return Err(KbError::ApplicationError {
                message: format!("Not a valid ZIP file: {}", backup_path.display()),
            });
        }

        // Open the ZIP archive
        let backup_file = File::open(backup_path).map_err(|e| KbError::BackupFailed {
            message: format!("Failed to open backup file: {}", e),
        })?;

        let mut archive = ZipArchive::new(backup_file)?;

        // Track restoration results
        let mut note_ids = HashSet::new();
        let mut notes_restored = 0;
        let mut notes_skipped = 0;
        let mut failed_notes = Vec::new();

        // Get current notes from cache
        let current_notes = {
            let cache = self
                .notes_cache
                .lock()
                .map_err(|_| KbError::LockAcquisitionFailed {
                    message: "Failed to acquire lock on notes cache".to_string(),
                })?;

            cache.keys().cloned().collect::<HashSet<String>>()
        };

        // First pass: Collect all note IDs from the ZIP
        for i in 0..archive.len() {
            let file = archive.by_index(i).map_err(|e| KbError::BackupFailed {
                message: format!("Failed to read ZIP entry: {}", e),
            })?;

            let file_name = file.name().to_string();

            // Expected format: "xx/xxxxxxxxxxxx.json"
            if file_name.ends_with(".json") {
                let path_parts: Vec<&str> = file_name.split('/').collect();
                if path_parts.len() == 2 {
                    if let Some(note_id) = path_parts[1].strip_suffix(".json") {
                        note_ids.insert(note_id.to_string());
                    }
                }
            }
        }

        // Second pass: Restore each note
        for note_id in &note_ids {
            let folder_name = &note_id[..2];
            let file_path = format!("{}/{}.json", folder_name, note_id);

            // Skip existing notes if not overwriting
            if !overwrite_existing && current_notes.contains(note_id) {
                notes_skipped += 1;
                continue;
            }

            // Try to extract and restore the note
            match self.restore_note_from_zip(&mut archive, &file_path, &note_id) {
                Ok(_) => {
                    notes_restored += 1;
                }
                Err(e) => {
                    warn!("Failed to restore note {}: {}", note_id, e);
                    failed_notes.push((note_id.clone(), e.to_string()));
                }
            }
        }

        // Build and return the restoration summary
        let summary = RestoreBackupSummary {
            backup_file: backup_path.to_path_buf(),
            total_notes: note_ids.len(),
            notes_restored,
            notes_skipped,
            failed_notes: failed_notes.clone(),
        };

        info!(
            "Backup restoration complete: restored {}, skipped {}, failed {} notes from {}",
            notes_restored,
            notes_skipped,
            failed_notes.len(),
            backup_path.display()
        );

        Ok(summary)
    }

    /// Helper method to restore a single note from the ZIP archive
    fn restore_note_from_zip(
        &self,
        archive: &mut ZipArchive<File>,
        file_path: &str,
        note_id: &str,
    ) -> Result<()> {
        use std::io::Read;

        // Read the note JSON from the ZIP
        let mut note_file = archive
            .by_name(file_path)
            .map_err(|e| KbError::BackupFailed {
                message: format!("Failed to find note {} in backup: {}", note_id, e),
            })?;

        let mut note_content = String::new();
        note_file
            .read_to_string(&mut note_content)
            .map_err(|e| KbError::BackupFailed {
                message: format!("Failed to read note {} content: {}", note_id, e),
            })?;

        // Deserialize the note
        let note: Note = serde_json::from_str(&note_content)?;

        // Verify note ID matches the expected ID
        if note.id != note_id {
            return Err(KbError::ApplicationError {
                message: format!("Note ID mismatch: expected {}, found {}", note_id, note.id),
            });
        }

        // Save the note to storage
        self.save_note(&note)?;

        Ok(())
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

// Implement Clone for NoteStorage to use in closures
impl Clone for NoteStorage {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            notes_cache: Arc::clone(&self.notes_cache),
            watcher: None,
            initialized: self.initialized,
            backup_scheduler: Arc::clone(&self.backup_scheduler),
        }
    }
}
