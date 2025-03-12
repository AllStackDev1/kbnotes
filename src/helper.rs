use std::{collections::HashMap, fs, path::Path, sync::{Arc, Mutex}};

use log::{debug, error, trace};
use notify::EventKind;

use crate::{KbError, Result, Note};

/// Handles file system events by updating the notes cache
pub async fn handle_fs_event(
    event: notify::Event,
    notes_cache: &Arc<Mutex<HashMap<String, Note>>>,
    // notes_dir: &PathBuf,
) {
    match event.kind {
        EventKind::Create(_) | EventKind::Modify(_) => {
            for path in event.paths {
                if path.extension().is_some_and(|ext| ext == "json") {
                    if let Some(_file_name) = path.file_name() {
                        if let Some(file_stem) = path.file_stem() {
                            let note_id = file_stem.to_string_lossy().to_string();

                            // Load the note from file
                            match load_note_from_file(&path) {
                                Ok(note) => {
                                    // Update cache
                                    if let Ok(mut cache) = notes_cache.lock() {
                                        cache.insert(note_id.clone(), note.clone());
                                        debug!("Updated cache for note: {}", note_id);
                                    }
                                }
                                Err(e) => {
                                    error!(
                                        "Failed to load note from changed file {}: {}",
                                        path.display(),
                                        e
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
        EventKind::Remove(_) => {
            for path in event.paths {
                if path.extension().is_some_and(|ext| ext == "json") {
                    if let Some(file_stem) = path.file_stem() {
                        let note_id = file_stem.to_string_lossy().to_string();

                        // Remove from cache
                        if let Ok(mut cache) = notes_cache.lock() {
                            if cache.remove(&note_id).is_some() {
                                debug!("Removed note {} from cache due to file deletion", note_id);
                            }
                        }
                    }
                }
            }
        }
        _ => {
            // Ignore other events
        }
    }
}

/// Helper method to load a single note from file
pub fn load_note_from_file(path: &Path) -> Result<Note> {
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

// Helper method for parsing tags
pub fn parse_tags(tags: Option<String>) -> Vec<String> {
    tags.map(|t| {
        t.split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect()
    })
    .unwrap_or_default()
}
