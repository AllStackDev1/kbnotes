use std::collections::HashMap;
use std::fs::{self, File, read_dir};
use std::io::BufReader;
use std::path::Path;
use std::sync::{Arc, Mutex};

use notify::{RecommendedWatcher, RecursiveMode, Watcher};

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
      // Ensure notes directory exists
      if !config.notes_dir.exists() {
          fs::create_dir_all(&config.notes_dir).map_err(|_| {
              KbError::DirectoryError { 
                  path: config.notes_dir.clone() 
              }
          })?;
      }
      
      // Ensure backup directory exists
      if !config.backup_dir.exists() {
          fs::create_dir_all(&config.backup_dir).map_err(|_| {
              KbError::DirectoryError { 
                  path: config.backup_dir.clone() 
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
      storage.load_notes()?;
      
      // Mark as initialized
      storage.initialized = true;
      
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
      // Get an exclusive lock on the cache
      let mut cache = self.notes_cache.lock()
          .map_err(|_| KbError::ApplicationError { 
              message: "Failed to acquire lock on notes cache".to_string() 
          })?;
      
      // Clear the existing cache
      cache.clear();
      
      // Process each file in the notes directory
      self.process_directory(&self.config.notes_dir, &mut cache)?;
      
      // Log success (or use your preferred logging method)
      println!("Loaded {} notes into cache", cache.len());
      
      Ok(())
  }
  
  /// Recursively processes a directory, loading all JSON notes.
  ///
  /// # Arguments
  ///
  /// * `dir_path` - The directory to process
  /// * `cache` - Mutable reference to the notes cache
  ///
  /// # Returns
  ///
  /// A Result indicating success or an error
  fn process_directory(&self, dir_path: &Path, cache: &mut HashMap<String, Note>) -> Result<()> {
      // Read the directory entries
      let entries = read_dir(dir_path)
          .map_err(|e| KbError::Io(e))?;
      
      // Process each entry
      for entry in entries {
          let entry = entry.map_err(|e| KbError::Io(e))?;
          let path = entry.path();
          
          if path.is_dir() {
              // Recursively process subdirectories
              self.process_directory(&path, cache)?;
          } else if let Some(extension) = path.extension() {
              // Check if this is a JSON file
              if extension == "json" {
                  // Load and parse the note
                  match self.load_note_from_file(&path) {
                      Ok(note) => {
                          // Add to cache, using note ID as the key
                          cache.insert(note.id.clone(), note);
                      },
                      Err(e) => {
                          // Log error but continue processing other files
                          eprintln!("Error loading note from {}: {}", path.display(), e);
                      }
                  }
              }
          }
      }
      
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
      // Open the file
      let file = File::open(file_path)
          .map_err(|e| KbError::Io(e))?;
      
      // Create a buffered reader
      let reader = BufReader::new(file);
      
      // Deserialize the JSON into a Note
      let note = serde_json::from_reader(reader)
          .map_err(|e| {
              // Provide more context in the error
              KbError::InvalidFormat { 
                  message: format!("Failed to parse note file {}: {}", file_path.display(), e) 
              }
          })?;
      
      Ok(note)
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
      // Create a new watcher with default configuration
      let mut watcher = notify::recommended_watcher(
          move |res: notify::Result<notify::Event>| {
              // We'll handle events later when implementing the callback
              if let Ok(event) = res {
                  // File system event occurred
                  // We'll process it in a future implementation
              }
          }
      ).map_err(|e| KbError::ApplicationError { 
          message: format!("Failed to create file watcher: {}", e) 
      })?;
      
      // Watch the notes directory recursively
      watcher.watch(&self.config.notes_dir, RecursiveMode::Recursive)
          .map_err(|e| KbError::ApplicationError { 
              message: format!("Failed to watch notes directory: {}", e) 
          })?;
      
      // Store the watcher
      self.watcher = Some(watcher);
      
      Ok(())
  }
}
