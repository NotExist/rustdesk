// Update Manager Module
// Core coordinator for the entire update system

use crate::{
    hbbs_http::downloader,
    update_config::{UpdateConfig, UpdateState},
    update_rollback::{BackupMetadata, RollbackManager},
    update_verifier::{UpdateManifest, UpdateVerifier},
};
use hbb_common::{bail, log, ResultType};
use std::{
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, SystemTime},
};

lazy_static::lazy_static! {
    static ref UPDATE_MANAGER: Mutex<Option<UpdateManager>> = Mutex::new(None);
    static ref UPDATE_STATE: Mutex<UpdateState> = Mutex::new(UpdateState::Idle);
}

/// Main update manager coordinating all update operations
pub struct UpdateManager {
    config: UpdateConfig,
    rollback_manager: RollbackManager,
    current_manifest: Option<UpdateManifest>,
    download_id: Option<String>,
    is_updating: Arc<AtomicBool>,
    last_check_time: Option<SystemTime>,
}

impl UpdateManager {
    /// Initialize the global update manager
    pub fn init() -> ResultType<()> {
        let config = UpdateConfig::load();
        let backup_dir = RollbackManager::default_backup_dir();
        let rollback_manager = RollbackManager::new(backup_dir)?;

        let manager = Self {
            config,
            rollback_manager,
            current_manifest: None,
            download_id: None,
            is_updating: Arc::new(AtomicBool::new(false)),
            last_check_time: None,
        };

        *UPDATE_MANAGER.lock().unwrap() = Some(manager);
        log::info!("Update manager initialized");
        Ok(())
    }

    /// Get the global update manager instance
    fn get() -> Option<std::sync::MutexGuard<'static, Option<UpdateManager>>> {
        Some(UPDATE_MANAGER.lock().unwrap())
    }

    /// Check for updates from the update server
    pub fn check_for_updates() -> ResultType<Option<UpdateManifest>> {
        let mut manager_guard = Self::get().ok_or("Update manager not initialized")?;
        let manager = manager_guard.as_mut().ok_or("Update manager not available")?;

        // Prevent concurrent checks
        if *UPDATE_STATE.lock().unwrap() != UpdateState::Idle {
            log::warn!("Update check already in progress");
            return Ok(None);
        }

        *UPDATE_STATE.lock().unwrap() = UpdateState::Checking;
        log::info!("Checking for updates...");

        // Record check time
        manager.last_check_time = Some(SystemTime::now());

        // Fetch update manifest
        match Self::fetch_manifest(&manager.config) {
            Ok(manifest) => {
                let current_version = crate::VERSION.to_string();
                log::info!(
                    "Current version: {}, Latest version: {}",
                    current_version,
                    manifest.version
                );

                // Check if update is available
                if Self::is_newer_version(&current_version, &manifest.version) {
                    log::info!("New version available: {}", manifest.version);

                    *UPDATE_STATE.lock().unwrap() = UpdateState::Available {
                        version: manifest.version.clone(),
                        release_notes: manifest.release_notes.clone(),
                        download_url: manifest.download_url.clone(),
                        signature: manifest.signature.clone(),
                        file_size: manifest.file_size,
                    };

                    manager.current_manifest = Some(manifest.clone());

                    // Auto-download if configured
                    if manager.config.auto_download {
                        log::info!("Auto-download enabled, starting download");
                        std::mem::drop(manager_guard); // Release lock
                        Self::download_update()?;
                    }

                    Ok(Some(manifest))
                } else {
                    log::info!("Already on the latest version");
                    *UPDATE_STATE.lock().unwrap() = UpdateState::Idle;
                    Ok(None)
                }
            }
            Err(e) => {
                log::error!("Failed to check for updates: {}", e);
                *UPDATE_STATE.lock().unwrap() = UpdateState::Failed {
                    error: e.to_string(),
                    retry_count: 0,
                };
                Err(e)
            }
        }
    }

    /// Download the update package
    pub fn download_update() -> ResultType<()> {
        let mut manager_guard = Self::get().ok_or("Update manager not initialized")?;
        let manager = manager_guard.as_mut().ok_or("Update manager not available")?;

        let manifest = manager
            .current_manifest.clone()
            .ok_or("No update manifest available")?;

        log::info!("Starting download of version {}", manifest.version);

        // Prepare download directory
        let download_dir = Self::get_download_dir()?;
        std::fs::create_dir_all(&download_dir)?;

        let filename = format!("RustDesk-{}.dmg", manifest.version);
        let download_path = download_dir.join(&filename);

        // Start download using existing downloader
        let download_id = downloader::download_file(
            manifest.download_url.clone(),
            Some(download_path.clone()),
            Some(Duration::from_secs(3600)), // Auto-delete after 1 hour if not completed
        )?;

        manager.download_id = Some(download_id.clone());

        *UPDATE_STATE.lock().unwrap() = UpdateState::Downloading {
            version: manifest.version.clone(),
            downloaded: 0,
            total: manifest.file_size,
            speed_bps: 0,
        };

        // Start progress monitoring
        Self::monitor_download_progress(download_id, manifest);

        Ok(())
    }

    /// Monitor download progress in background thread
    fn monitor_download_progress(download_id: String, manifest: UpdateManifest) {
        std::thread::spawn(move || {
            loop {
                std::thread::sleep(Duration::from_millis(500));

                match downloader::get_download_data(&download_id) {
                    Ok(data) => {
                        if let Some(error) = data.error {
                            log::error!("Download failed: {}", error);
                            *UPDATE_STATE.lock().unwrap() = UpdateState::Failed {
                                error,
                                retry_count: 0,
                            };
                            break;
                        }

                        // Update progress
                        if let Some(total) = data.total_size {
                            *UPDATE_STATE.lock().unwrap() = UpdateState::Downloading {
                                version: manifest.version.clone(),
                                downloaded: data.downloaded_size,
                                total,
                                speed_bps: 0, // TODO: Calculate actual speed
                            };

                            // Check if download completed
                            if data.downloaded_size >= total {
                                if let Some(path) = data.path {
                                    log::info!("Download completed: {:?}", path);
                                    *UPDATE_STATE.lock().unwrap() = UpdateState::Downloaded {
                                        version: manifest.version.clone(),
                                        file_path: path.to_string_lossy().to_string(),
                                    };

                                    // Start verification
                                    if let Err(e) = Self::verify_update(&path, &manifest) {
                                        log::error!("Verification failed: {}", e);
                                        *UPDATE_STATE.lock().unwrap() = UpdateState::Failed {
                                            error: e.to_string(),
                                            retry_count: 0,
                                        };
                                    }
                                    break;
                                }
                            }
                        }
                    }
                    Err(e) => {
                        log::error!("Failed to get download data: {}", e);
                        break;
                    }
                }
            }
        });
    }

    /// Verify downloaded update package
    fn verify_update(file_path: &Path, manifest: &UpdateManifest) -> ResultType<()> {
        log::info!("Verifying update package");

        *UPDATE_STATE.lock().unwrap() = UpdateState::Verifying {
            version: manifest.version.clone(),
        };

        // Verify signature and integrity
        UpdateVerifier::verify_update(file_path, &manifest.signature, manifest.file_size)?;

        log::info!("Update package verified successfully");

        *UPDATE_STATE.lock().unwrap() = UpdateState::ReadyToInstall {
            version: manifest.version.clone(),
            file_path: file_path.to_string_lossy().to_string(),
        };

        // Check if we should install automatically
        let manager_guard = Self::get().ok_or("Update manager not initialized")?;
        if let Some(manager) = manager_guard.as_ref() {
            if manager.config.install_timing == crate::update_config::UpdateInstallTiming::WhenIdle
            {
                // Check if system is idle (no active connections)
                if crate::updater::has_no_active_conns() {
                    log::info!("System is idle, proceeding with installation");
                    std::mem::drop(manager_guard); // Release lock
                    Self::install_update()?;
                } else {
                    log::info!("Active connections detected, deferring installation");
                    Self::send_notification("update_ready_deferred", &manifest.version);
                }
            }
        }

        Ok(())
    }

    /// Install the verified update
    pub fn install_update() -> ResultType<()> {
        let mut manager_guard = Self::get().ok_or("Update manager not initialized")?;
        let manager = manager_guard.as_mut().ok_or("Update manager not available")?;

        // Prevent concurrent installations
        if manager
            .is_updating
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            bail!("Installation already in progress");
        }

        // Verify no active connections
        if !crate::updater::has_no_active_conns() {
            manager.is_updating.store(false, Ordering::SeqCst);
            bail!("Cannot install update while connections are active");
        }

        let state = UPDATE_STATE.lock().unwrap().clone();
        let (version, file_path) = match state {
            UpdateState::ReadyToInstall { version, file_path } => (version, file_path),
            _ => {
                manager.is_updating.store(false, Ordering::SeqCst);
                bail!("No update ready to install");
            }
        };

        log::info!("Starting installation of version {}", version);

        *UPDATE_STATE.lock().unwrap() = UpdateState::Installing {
            version: version.clone(),
            progress: 0,
        };

        // Create backup before installation
        if manager.config.keep_backup {
            log::info!("Creating backup before update");
            let current_app = Self::get_current_app_path()?;
            let current_version = crate::VERSION.to_string();

            match manager
                .rollback_manager
                .create_backup(&current_app, &current_version)
            {
                Ok(_) => {
                    log::info!("Backup created successfully");
                    // Cleanup old backups
                    let _ = manager
                        .rollback_manager
                        .cleanup_old_backups(manager.config.max_backups);
                }
                Err(e) => {
                    log::error!("Failed to create backup: {}", e);
                    manager.is_updating.store(false, Ordering::SeqCst);
                    *UPDATE_STATE.lock().unwrap() = UpdateState::Failed {
                        error: format!("Backup failed: {}", e),
                        retry_count: 0,
                    };
                    return Err(e);
                }
            }
        }

        // Extract and install
        std::mem::drop(manager_guard); // Release lock before platform-specific operations
        let result = Self::perform_installation(&PathBuf::from(&file_path), &version);

        let manager_guard = Self::get().ok_or("Update manager not initialized")?;
        if let Some(manager) = manager_guard.as_ref() {
            manager.is_updating.store(false, Ordering::SeqCst);
        }

        match result {
            Ok(_) => {
                log::info!("Update installed successfully");
                *UPDATE_STATE.lock().unwrap() = UpdateState::Completed {
                    version: version.clone(),
                };
                Ok(())
            }
            Err(e) => {
                log::error!("Installation failed: {}", e);
                *UPDATE_STATE.lock().unwrap() = UpdateState::Failed {
                    error: e.to_string(),
                    retry_count: 0,
                };
                Err(e)
            }
        }
    }

    /// Perform platform-specific installation
    #[cfg(target_os = "macos")]
    fn perform_installation(dmg_path: &Path, _version: &str) -> ResultType<()> {
        log::info!("Performing macOS installation");

        // Extract DMG to temp directory
        crate::platform::macos::extract_update_dmg(
            dmg_path.to_str().ok_or("Invalid DMG path")?,
        );

        // Wait for extraction to complete
        std::thread::sleep(Duration::from_secs(2));

        // Call platform update function
        crate::platform::update_to(crate::platform::macos::UPDATE_TEMP_DIR)?;

        Ok(())
    }

    #[cfg(not(target_os = "macos"))]
    fn perform_installation(_file_path: &Path, _version: &str) -> ResultType<()> {
        bail!("Installation not implemented for this platform");
    }

    /// Rollback to a previous version
    pub fn rollback_to_backup(backup_index: usize) -> ResultType<()> {
        let mut manager_guard = Self::get().ok_or("Update manager not initialized")?;
        let manager = manager_guard.as_mut().ok_or("Update manager not available")?;

        let backups = manager.rollback_manager.list_backups()?;
        let backup = backups.get(backup_index).ok_or("Backup not found")?;

        log::info!("Rolling back to version {}", backup.version);

        *UPDATE_STATE.lock().unwrap() = UpdateState::RollingBack {
            from_version: crate::VERSION.to_string(),
            to_version: backup.version.clone(),
        };

        manager.rollback_manager.restore_backup(backup)?;

        log::info!("Rollback completed. Please restart the application.");
        *UPDATE_STATE.lock().unwrap() = UpdateState::Idle;

        Ok(())
    }

    /// Get current update state
    pub fn get_state() -> UpdateState {
        UPDATE_STATE.lock().unwrap().clone()
    }

    /// Update configuration
    pub fn update_config(new_config: UpdateConfig) -> ResultType<()> {
        let mut manager_guard = Self::get().ok_or("Update manager not initialized")?;
        let manager = manager_guard.as_mut().ok_or("Update manager not available")?;

        manager.config = new_config.clone();
        new_config.save()?;

        log::info!("Update configuration updated");
        Ok(())
    }

    /// Cancel ongoing download
    pub fn cancel_download() -> ResultType<()> {
        let manager_guard = Self::get().ok_or("Update manager not initialized")?;
        let manager = manager_guard.as_ref().ok_or("Update manager not available")?;

        if let Some(download_id) = &manager.download_id {
            downloader::cancel(download_id);
            log::info!("Download cancelled");
        }

        *UPDATE_STATE.lock().unwrap() = UpdateState::Idle;
        Ok(())
    }

    // Helper methods

    fn fetch_manifest(config: &UpdateConfig) -> ResultType<UpdateManifest> {
        // TODO: Implement actual HTTP fetch from update server
        // For now, this is a placeholder
        let channel_str = match config.channel {
            crate::update_config::UpdateChannel::Stable => "stable",
            crate::update_config::UpdateChannel::Beta => "beta",
            crate::update_config::UpdateChannel::Nightly => "nightly",
        };

        let manifest_url = format!(
            "https://github.com/rustdesk/rustdesk/releases/latest/download/manifest-{}.json",
            channel_str
        );

        log::debug!("Fetching manifest from: {}", manifest_url);

        // This would normally do an HTTP request
        // For now, return an error to indicate not implemented
        bail!("Manifest fetching not yet implemented");
    }

    fn is_newer_version(current: &str, latest: &str) -> bool {
        // Simple version comparison
        // TODO: Use proper semver comparison
        current < latest
    }

    #[cfg(target_os = "macos")]
    fn get_current_app_path() -> ResultType<PathBuf> {
        let exe = std::env::current_exe()?;
        // /Applications/RustDesk.app/Contents/MacOS/RustDesk -> /Applications/RustDesk.app
        let app_path = exe
            .parent()
            .and_then(|p| p.parent())
            .and_then(|p| p.parent())
            .ok_or("Cannot determine app path")?;
        Ok(app_path.to_path_buf())
    }

    #[cfg(not(target_os = "macos"))]
    fn get_current_app_path() -> ResultType<PathBuf> {
        std::env::current_exe()
    }

    fn get_download_dir() -> ResultType<PathBuf> {
        let temp = std::env::temp_dir();
        Ok(temp.join("rustdesk_updates"))
    }

    fn send_notification(event: &str, version: &str) {
        #[cfg(feature = "flutter")]
        {
            let data = serde_json::json!({
                "name": event,
                "version": version,
            });
            let _ = crate::flutter::push_global_event(
                crate::flutter::APP_TYPE_MAIN,
                serde_json::to_string(&data).unwrap_or_default(),
            );
        }

        log::info!("Notification sent: {} for version {}", event, version);
    }
}
