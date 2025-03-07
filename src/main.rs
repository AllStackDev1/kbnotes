use std::path::PathBuf;
use std::process;
use std::sync::Arc;

use clap::{Arg, Command};
use env_logger::Env;
use log::{debug, error, info, warn};
use tokio::sync::Mutex;

use kbnotes::{Config, KbError, NoteStorage, Result};

#[tokio::main]
async fn main() {
    // Initialize logging first for better error reporting during startup
    env_logger::Builder::from_env(Env::default().default_filter_or("info"))
        .format_timestamp_millis()
        .init();

    info!("KBNotes application starting...");

    // Parse command-line arguments using clap
    let matches = Command::new("KBNotes")
        .version("1.0.0")
        .author("Your Name <your.email@example.com>")
        .about("Knowledge Base and Note-taking Application")
        .arg(
            Arg::new("config")
                .short('c')
                .long("config")
                .value_name("FILE")
                .help("Sets a custom config file path")
                .value_parser(clap::value_parser!(String)),
        )
        .arg(
            Arg::new("notes-dir")
                .long("notes-dir")
                .value_name("DIRECTORY")
                .help("Sets the notes directory")
                .value_parser(clap::value_parser!(String)),
        )
        .arg(
            Arg::new("backup-dir")
                .long("backup-dir")
                .value_name("DIRECTORY")
                .help("Sets the backup directory")
                .value_parser(clap::value_parser!(String)),
        )
        .get_matches();

    // Initialize the storage system
    match initialize_storage(&matches).await {
        Ok(storage) => {
            info!("NoteStorage initialized successfully");

            // Here you would start your application's main logic
            // For example, launch a CLI interface, API server, etc.

            // For demonstration, let's just print some stats
            /* let note_count = storage
            .lock() */

            // Get backup status
            let backup_status = storage.lock().await.get_backup_status().await;
            info!(
                "Backup scheduler status: {}",
                if backup_status.is_running {
                    "running"
                } else {
                    "stopped"
                }
            );

            // Run the application until terminated
            run_application(storage).await;
        }
        Err(e) => {
            error!("Failed to initialize storage: {}", e);
            process::exit(1);
        }
    }
}

/// Initialize the storage system with configuration
async fn initialize_storage(matches: &clap::ArgMatches) -> Result<Arc<Mutex<NoteStorage>>> {
    // Step 1: Load configuration
    let config = load_configuration(matches)?;
    info!("Configuration loaded successfully");

    // Step 2: Create the storage instance
    let storage = NoteStorage::new(config.clone());

    // Step 3: Create an Arc<Mutex<>> wrapper for the storage
    let storage_arc = Arc::new(Mutex::new(storage));

    // Step 4: Initialize storage (load notes and start backup scheduler)
    storage_arc
        .lock()
        .await
        .initialize(Arc::clone(&storage_arc))
        .await?;

    // Return the initialized storage instance
    Ok(storage_arc)
}

/// Load configuration from file and/or command-line arguments
fn load_configuration(matches: &clap::ArgMatches) -> Result<Config> {
    // Default configuration
    let mut config = load_default_config()?;
    // Override with config file if specified
    if let Some(config_path) = matches.get_one::<String>("config") {
        match load_config_from_file(config_path) {
            Ok(file_config) => {
                info!("Loaded configuration from file: {}", config_path);
                config = file_config;
            }
            Err(e) => {
                warn!("Failed to load configuration from {}: {}", config_path, e);
                warn!("Falling back to default configuration");
            }
        }
    }
    // Override with command-line arguments
    if let Some(notes_dir) = matches.get_one::<String>("notes-dir") {
        info!("Using notes directory from command line: {}", notes_dir);
        config.notes_dir = PathBuf::from(notes_dir);
    }

    if let Some(backup_dir) = matches.get_one::<String>("backup-dir") {
        info!("Using backup directory from command line: {}", backup_dir);
        config.backup_dir = PathBuf::from(backup_dir);
    }

    // Validate the configuration
    validate_configuration(&config)?;

    Ok(config)
}

/// Load the default configuration
fn load_default_config() -> Result<Config> {
    // Get home directory for default paths
    let home_dir = dirs::home_dir().ok_or_else(|| KbError::ApplicationError {
        message: "Could not determine home directory".to_string(),
    })?;

    let notes_dir = home_dir.join(".kbnotes").join("notes");
    let backup_dir = home_dir.join(".kbnotes").join("backups");

    Ok(Config {
        notes_dir,
        backup_dir,
        backup_frequency: 24, // Daily backups
        max_backups: 10,      // Keep 10 backups
        encrypt_notes: false, // No encryption by default
        editor_command: None, // No custom editor
        auto_save: true,      // Auto-save enabled
        auto_backup: true,    // Auto-backup enabled
    })
}

/// Load configuration from a file
fn load_config_from_file(config_path: &str) -> Result<Config> {
    use std::fs;

    let config_file = fs::read_to_string(config_path).map_err(KbError::Io)?;

    // Try to parse as JSON first
    if config_path.ends_with(".json") {
        return serde_json::from_str(&config_file).map_err(KbError::Serialization);
    }

    // // Try to parse as TOML if not JSON
    // if config_path.ends_with(".toml") {
    //     return toml::from_str(&config_file).map_err(|e| KbError::ApplicationError {
    //         message: format!("Failed to parse TOML config: {}", e),
    //     });
    // }

    // // Try YAML as a last resort
    // if config_path.ends_with(".yaml") || config_path.ends_with(".yml") {
    //     return serde_yaml::from_str(&config_file).map_err(|e| KbError::ApplicationError {
    //         message: format!("Failed to parse YAML config: {}", e)
    //     });
    // }

    Err(KbError::ApplicationError {
        message: format!("Unsupported config file format: {}", config_path),
    })
}

/// Validate the configuration for required values and permissions
fn validate_configuration(config: &Config) -> Result<()> {
    use std::fs;

    // Check if notes directory exists or can be created
    if !config.notes_dir.exists() {
        info!(
            "Notes directory does not exist, will be created: {}",
            config.notes_dir.display()
        );
        // We'll check if we can create it during initialization
    } else {
        // Check if we have write access to the notes directory
        let test_file_path = config.notes_dir.join(".write_test");
        match fs::write(&test_file_path, b"write test") {
            Ok(_) => {
                // Clean up the test file
                let _ = fs::remove_file(&test_file_path);
            }
            Err(e) => {
                return Err(KbError::ApplicationError {
                    message: format!(
                        "Cannot write to notes directory '{}': {}",
                        config.notes_dir.display(),
                        e
                    ),
                });
            }
        }
    }

    // Check if backup directory exists or can be created
    if !config.backup_dir.exists() {
        info!(
            "Backup directory does not exist, will be created: {}",
            config.backup_dir.display()
        );
        // We'll check if we can create it during initialization
    }

    // Validate backup frequency (must be positive)
    if config.backup_frequency == 0 {
        return Err(KbError::ApplicationError {
            message: "Backup frequency cannot be zero".to_string(),
        });
    }

    Ok(())
}

/// Gracefully shuts down the application
async fn shutdown_application(storage: Arc<Mutex<NoteStorage>>) -> Result<()> {
    info!("Application shutting down...");

    // Try to acquire the lock with a timeout
    let storage_lock_result =
        tokio::time::timeout(std::time::Duration::from_secs(5), storage.lock()).await;

    // Handle the result of the timeout operation
    let mut storage_lock = match storage_lock_result {
        Ok(lock) => {
            // Successfully acquired the lock within the timeout
            debug!("Acquired storage lock for shutdown within timeout");
            lock
        }
        Err(_elapsed) => {
            // Timeout occurred, we'll try a non-blocking approach
            warn!("Could not acquire lock on storage for shutdown within timeout - trying non-blocking attempt");

            // Try a non-blocking lock acquisition
            match storage.try_lock() {
                Ok(lock) => {
                    info!("Successfully acquired lock through non-blocking attempt");
                    lock
                }
                Err(_) => {
                    // We still couldn't get the lock, we'll wait indefinitely as a last resort
                    warn!("Non-blocking lock attempt failed - waiting indefinitely for lock (might delay shutdown)");
                    let lock = storage.lock().await;
                    info!("Finally acquired storage lock for shutdown");
                    lock
                }
            }
        }
    };

    // Perform complete storage shutdown
    match storage_lock.shutdown().await {
        Ok(_) => info!("Storage system shut down successfully"),
        Err(e) => {
            error!("Error during storage shutdown: {}", e);
            // We'll continue with application shutdown despite this error
            return Err(e);
        }
    }

    // Release the lock
    drop(storage_lock);

    info!("Application shutdown complete");
    Ok(())
}

/// Enhanced application loop with multiple signal handling and proper timeout behavior
async fn run_application(storage: Arc<Mutex<NoteStorage>>) {
    // Set up ctrl-c handler which works on all platforms
    tokio::spawn(async move {
        match tokio::signal::ctrl_c().await {
            Ok(()) => {
                info!("Received Ctrl+C, initiating shutdown");

                // Execute shutdown with timeout
                const SHUTDOWN_TIMEOUT_SECS: u64 = 30;

                match tokio::time::timeout(
                    tokio::time::Duration::from_secs(SHUTDOWN_TIMEOUT_SECS),
                    shutdown_application(storage),
                )
                .await
                {
                    Ok(result) => {
                        if let Err(e) = result {
                            error!("Errors occurred during shutdown: {}", e);
                        } else {
                            info!("Application shutdown completed successfully");
                        }
                    }
                    Err(_elapsed) => {
                        error!(
                            "Shutdown timed out after {} seconds - forcing exit",
                            SHUTDOWN_TIMEOUT_SECS
                        );
                        // We'll exit with an error code since the shutdown timed out
                        std::process::exit(1);
                    }
                }

                // Signal the main loop to exit
                std::process::exit(0);
            }
            Err(e) => error!("Error setting up Ctrl+C handler: {}", e),
        }
    });

    // Your main application logic here
    info!("Application is running. Press Ctrl+C to exit.");

    // In a real application, you might have a server or event loop here
    // For demonstration, we'll just wait indefinitely
    loop {
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
        // Your application's main logic would go here
    }
}
