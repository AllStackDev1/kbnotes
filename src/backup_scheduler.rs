// src/backup_scheduler.rs - Backup scheduler module
use std::path::PathBuf;
use std::sync::{Arc, Weak};

use chrono::Utc;
use log::{debug, error, info};
use tokio::sync::{mpsc, Mutex};
use tokio::task::JoinHandle;
use tokio::time::{self, Duration};

use crate::{Config, KbError, NoteStorage, Result};

#[derive(Debug, Clone)]
pub struct BackupSchedulerStatus {
    /// Whether the scheduler is running
    pub is_running: bool,
    /// The time the last backup was created
    pub last_backup_time: Option<chrono::DateTime<Utc>>,
    /// The path to the last backup file
    pub last_backup_path: Option<PathBuf>,
}

#[derive(Debug, Clone)]
pub enum BackupCommand {
    /// Create a full backup immediately
    CreateBackupNow,
    /// Stop the backup scheduler
    Stop,
}

pub struct BackupScheduler {
    /// Configuration for the scheduler
    config: Config,

    /// Channel to send commands to the scheduler task
    command_tx: mpsc::Sender<BackupCommand>,

    /// Handle to the scheduler task
    scheduler_task: Option<JoinHandle<()>>,

    /// Current status of the scheduler
    status: BackupSchedulerStatus,

    /// Weak reference to the storage
    storage: Option<Weak<Mutex<NoteStorage>>>,
}

/// Represents the backup scheduler status
impl BackupScheduler {
    /// Create a new backup scheduler with the provided config
    pub fn new(config: Config) -> Self {
        info!("Initializing backup scheduler with config: {:?}", config);
        let (command_tx, _) = mpsc::channel(10);

        Self {
            config,
            command_tx,
            scheduler_task: None,
            status: BackupSchedulerStatus {
                is_running: false,
                last_backup_time: None,
                last_backup_path: None,
            },
            storage: None,
        }
    }

    /// Set the weak reference to NoteStorage
    pub fn set_storage(&mut self, storage: Arc<Mutex<NoteStorage>>) {
        self.storage = Some(Arc::downgrade(&storage));
        info!("Storage reference set in BackupScheduler.");
    }

    /// Star the backup scheduler
    pub async fn start(&mut self) -> Result<()> {
        info!("Starting backup scheduler...");
        if !self.config.auto_backup {
            return Ok(()); // No need to start if auto backup is disabled
        }

        let storage = match &self.storage {
            Some(weak) => match weak.upgrade() {
                Some(strong) => strong, // Successfully retrieved Arc<Mutex<NoteStorage>>
                None => {
                    error!("Failed to retrieve NoteStorage - reference is no longer valid.");
                    return Err(KbError::ApplicationError {
                        message: "NoteStorage reference is no longer valid.".to_string(),
                    });
                }
            },
            None => {
                error!("No storage reference found in BackupScheduler.");
                return Err(KbError::ApplicationError {
                    message: "BackupScheduler does not have a storage reference.".to_string(),
                });
            }
        };

        let (command_tx, mut command_rx) = mpsc::channel(10);
        self.command_tx = command_tx;

        let backup_frequency_secs = self.config.backup_frequency as u64 * 3600;
        let storage_clone = Arc::clone(&storage);

        let task = tokio::spawn(async move {
            let mut interval = time::interval(Duration::from_secs(backup_frequency_secs));
            interval.tick().await; // Initial tick

            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        let storage = Arc::clone(&storage_clone);
                        match storage.lock().await.create_full_backup() {
                            Ok(path) => info!("Scheduled backup completed at {}", path.display()),
                            Err(e) => error!("Scheduled backup failed: {}", e),
                        };
                    }
                    Some(cmd) = command_rx.recv() => match cmd {
                        BackupCommand::CreateBackupNow => {
                            let storage = Arc::clone(&storage_clone);
                            match storage.lock().await.create_full_backup() {
                                Ok(path) => info!("Manual backup completed at {}", path.display()),
                                Err(e) => error!("Manual backup failed: {}", e),
                            };
                        },
                        BackupCommand::Stop => {
                            info!("Backup scheduler stopping...");
                            break;
                        }
                    }
                }
            }
        });

        self.scheduler_task = Some(task);
        self.status.is_running = true;

        Ok(())
    }

    /// Stop the backup scheduler if it's running
    pub async fn stop(&mut self) -> Result<()> {
        if let Some(task) = self.scheduler_task.take() {
            // Send stop command to the scheduler task
            if let Err(e) = self.command_tx.send(BackupCommand::Stop).await {
                error!("Failed to send stop command to backup scheduler: {}", e);
            }

            // Wait for the task to complete
            if let Err(e) = task.await {
                let error_mgs = format!("Failed to stop backup scheduler: {}", e);
                error!("{}", error_mgs);
                return Err(KbError::BackupFailed { message: error_mgs });
            }

            self.status.is_running = false;
            info!("Backup scheduler stopped");
        } else {
            debug!("Backup scheduler is not running");
        }

        Ok(())
    }

    /// Create a backup immediately, regardless of the schedule
    pub async fn create_backup_now(&self) -> Result<()> {
        if !self.status.is_running {
            return Err(KbError::BackupFailed {
                message: "Backup scheduler is not running".to_string(),
            });
        }

        self.command_tx
            .send(BackupCommand::CreateBackupNow)
            .await
            .map_err(|e| KbError::BackupFailed {
                message: format!("Failed to send backup command: {}", e),
            })?;

        Ok(())
    }

    /// Get the current status of the backup scheduler
    pub fn get_status(&self) -> BackupSchedulerStatus {
        self.status.clone()
    }

    /// Update the scheduler's last backup information
    pub fn update_last_backup(&mut self, path: PathBuf) {
        self.status.last_backup_time = Some(Utc::now());
        self.status.last_backup_path = Some(path);
    }
}
