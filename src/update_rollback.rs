// Update Rollback Module
// Manages backup and rollback functionality for safe updates

use hbb_common::{bail, log, ResultType};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// Backup metadata
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BackupMetadata {
    pub version: String,
    pub backup_time: u64, // Unix timestamp
    pub backup_path: PathBuf,
    pub original_path: PathBuf,
    pub file_size: u64,
}

/// Rollback manager for handling update backups and recovery
pub struct RollbackManager {
    backup_dir: PathBuf,
}

impl RollbackManager {
    /// Create a new rollback manager
    pub fn new(backup_dir: PathBuf) -> ResultType<Self> {
        // Ensure backup directory exists
        if !backup_dir.exists() {
            std::fs::create_dir_all(&backup_dir)?;
            log::info!("Created backup directory: {:?}", backup_dir);
        }

        Ok(Self { backup_dir })
    }

    /// Get default backup directory for the platform
    #[cfg(target_os = "macos")]
    pub fn default_backup_dir() -> PathBuf {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
        PathBuf::from(home)
            .join("Library")
            .join("Application Support")
            .join(crate::get_app_name())
            .join("Backups")
    }

    #[cfg(not(target_os = "macos"))]
    pub fn default_backup_dir() -> PathBuf {
        let temp = std::env::temp_dir();
        temp.join("rustdesk_backups")
    }

    /// Create a backup of the current application before update
    ///
    /// # Arguments
    /// * `app_path` - Path to the current application
    /// * `version` - Current version being backed up
    ///
    /// # Returns
    /// * `BackupMetadata` containing backup information
    pub fn create_backup(&self, app_path: &Path, version: &str) -> ResultType<BackupMetadata> {
        log::info!("Creating backup of version {} from {:?}", version, app_path);

        if !app_path.exists() {
            bail!("Application path does not exist: {:?}", app_path);
        }

        // Generate backup filename with timestamp
        let timestamp = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)?
            .as_secs();

        let backup_name = format!("{}_{}.backup", version, timestamp);
        let backup_path = self.backup_dir.join(&backup_name);

        // Perform the backup copy
        #[cfg(target_os = "macos")]
        {
            // Use ditto for app bundles on macOS to preserve metadata
            self.backup_macos_app(app_path, &backup_path)?;
        }

        #[cfg(not(target_os = "macos"))]
        {
            self.backup_generic(app_path, &backup_path)?;
        }

        // Calculate backup size
        let file_size = Self::calculate_size(&backup_path)?;

        let metadata = BackupMetadata {
            version: version.to_string(),
            backup_time: timestamp,
            backup_path: backup_path.clone(),
            original_path: app_path.to_path_buf(),
            file_size,
        };

        // Save metadata
        self.save_metadata(&metadata)?;

        log::info!(
            "Backup created successfully: {:?} ({} bytes)",
            backup_path,
            file_size
        );

        Ok(metadata)
    }

    #[cfg(target_os = "macos")]
    fn backup_macos_app(&self, source: &Path, dest: &Path) -> ResultType<()> {
        use std::process::Command;

        log::info!("Using ditto to backup macOS app bundle");

        let output = Command::new("ditto")
            .arg(source)
            .arg(dest)
            .output()?;

        if !output.status.success() {
            let error = String::from_utf8_lossy(&output.stderr);
            bail!("Failed to create backup using ditto: {}", error);
        }

        Ok(())
    }

    #[cfg(not(target_os = "macos"))]
    fn backup_generic(&self, source: &Path, dest: &Path) -> ResultType<()> {
        log::info!("Creating backup using standard copy");

        if source.is_dir() {
            Self::copy_dir_recursive(source, dest)?;
        } else {
            std::fs::copy(source, dest)?;
        }

        Ok(())
    }

    fn copy_dir_recursive(source: &Path, dest: &Path) -> ResultType<()> {
        std::fs::create_dir_all(dest)?;

        for entry in std::fs::read_dir(source)? {
            let entry = entry?;
            let file_type = entry.file_type()?;
            let source_path = entry.path();
            let dest_path = dest.join(entry.file_name());

            if file_type.is_dir() {
                Self::copy_dir_recursive(&source_path, &dest_path)?;
            } else {
                std::fs::copy(&source_path, &dest_path)?;
            }
        }

        Ok(())
    }

    /// Restore from a backup
    ///
    /// # Arguments
    /// * `metadata` - Backup metadata to restore from
    ///
    /// # Returns
    /// * `Ok(())` if restoration succeeds
    pub fn restore_backup(&self, metadata: &BackupMetadata) -> ResultType<()> {
        log::info!(
            "Restoring backup: {} from {:?}",
            metadata.version,
            metadata.backup_path
        );

        if !metadata.backup_path.exists() {
            bail!("Backup file does not exist: {:?}", metadata.backup_path);
        }

        // Verify backup integrity before restore
        let current_size = Self::calculate_size(&metadata.backup_path)?;
        if current_size != metadata.file_size {
            bail!(
                "Backup integrity check failed. Size mismatch: expected {}, got {}",
                metadata.file_size,
                current_size
            );
        }

        // Remove current application if it exists
        if metadata.original_path.exists() {
            log::info!("Removing current application: {:?}", metadata.original_path);
            if metadata.original_path.is_dir() {
                std::fs::remove_dir_all(&metadata.original_path)?;
            } else {
                std::fs::remove_file(&metadata.original_path)?;
            }
        }

        // Restore from backup
        #[cfg(target_os = "macos")]
        {
            self.restore_macos_app(&metadata.backup_path, &metadata.original_path)?;
        }

        #[cfg(not(target_os = "macos"))]
        {
            self.restore_generic(&metadata.backup_path, &metadata.original_path)?;
        }

        log::info!("Backup restored successfully");
        Ok(())
    }

    #[cfg(target_os = "macos")]
    fn restore_macos_app(&self, source: &Path, dest: &Path) -> ResultType<()> {
        use std::process::Command;

        log::info!("Using ditto to restore macOS app bundle");

        let output = Command::new("ditto")
            .arg(source)
            .arg(dest)
            .output()?;

        if !output.status.success() {
            let error = String::from_utf8_lossy(&output.stderr);
            bail!("Failed to restore backup using ditto: {}", error);
        }

        // Remove quarantine attribute
        Command::new("xattr")
            .args(&["-r", "-d", "com.apple.quarantine"])
            .arg(dest)
            .output()
            .ok(); // Ignore errors, not critical

        Ok(())
    }

    #[cfg(not(target_os = "macos"))]
    fn restore_generic(&self, source: &Path, dest: &Path) -> ResultType<()> {
        log::info!("Restoring backup using standard copy");

        if source.is_dir() {
            Self::copy_dir_recursive(source, dest)?;
        } else {
            std::fs::copy(source, dest)?;
        }

        Ok(())
    }

    /// List all available backups
    pub fn list_backups(&self) -> ResultType<Vec<BackupMetadata>> {
        let mut backups = Vec::new();

        if !self.backup_dir.exists() {
            return Ok(backups);
        }

        for entry in std::fs::read_dir(&self.backup_dir)? {
            let entry = entry?;
            let path = entry.path();

            // Look for metadata files
            if path.extension().and_then(|s| s.to_str()) == Some("json") {
                if let Ok(metadata) = self.load_metadata(&path) {
                    // Verify backup file still exists
                    if metadata.backup_path.exists() {
                        backups.push(metadata);
                    }
                }
            }
        }

        // Sort by timestamp (newest first)
        backups.sort_by(|a, b| b.backup_time.cmp(&a.backup_time));

        Ok(backups)
    }

    /// Clean old backups, keeping only the specified number
    pub fn cleanup_old_backups(&self, keep_count: usize) -> ResultType<usize> {
        let mut backups = self.list_backups()?;

        if backups.len() <= keep_count {
            return Ok(0);
        }

        let to_remove = backups.split_off(keep_count);
        let removed_count = to_remove.len();

        for backup in to_remove {
            log::info!("Removing old backup: {:?}", backup.backup_path);

            // Remove backup file
            if backup.backup_path.exists() {
                if backup.backup_path.is_dir() {
                    std::fs::remove_dir_all(&backup.backup_path)?;
                } else {
                    std::fs::remove_file(&backup.backup_path)?;
                }
            }

            // Remove metadata file
            let metadata_path = self.metadata_path(&backup);
            if metadata_path.exists() {
                std::fs::remove_file(&metadata_path)?;
            }
        }

        log::info!("Removed {} old backup(s)", removed_count);
        Ok(removed_count)
    }

    /// Delete a specific backup
    pub fn delete_backup(&self, metadata: &BackupMetadata) -> ResultType<()> {
        log::info!("Deleting backup: {:?}", metadata.backup_path);

        // Remove backup file
        if metadata.backup_path.exists() {
            if metadata.backup_path.is_dir() {
                std::fs::remove_dir_all(&metadata.backup_path)?;
            } else {
                std::fs::remove_file(&metadata.backup_path)?;
            }
        }

        // Remove metadata file
        let metadata_path = self.metadata_path(metadata);
        if metadata_path.exists() {
            std::fs::remove_file(&metadata_path)?;
        }

        log::info!("Backup deleted successfully");
        Ok(())
    }

    /// Calculate total size of a file or directory
    fn calculate_size(path: &Path) -> ResultType<u64> {
        let mut total = 0u64;

        if path.is_dir() {
            for entry in std::fs::read_dir(path)? {
                let entry = entry?;
                let path = entry.path();
                if path.is_dir() {
                    total += Self::calculate_size(&path)?;
                } else {
                    total += entry.metadata()?.len();
                }
            }
        } else {
            total = std::fs::metadata(path)?.len();
        }

        Ok(total)
    }

    /// Save backup metadata to JSON file
    fn save_metadata(&self, metadata: &BackupMetadata) -> ResultType<()> {
        let metadata_path = self.metadata_path(metadata);
        let json = serde_json::to_string_pretty(metadata)?;
        std::fs::write(&metadata_path, json)?;
        log::debug!("Saved metadata: {:?}", metadata_path);
        Ok(())
    }

    /// Load backup metadata from JSON file
    fn load_metadata(&self, path: &Path) -> ResultType<BackupMetadata> {
        let json = std::fs::read_to_string(path)?;
        let metadata: BackupMetadata = serde_json::from_str(&json)?;
        Ok(metadata)
    }

    /// Get metadata file path for a backup
    fn metadata_path(&self, metadata: &BackupMetadata) -> PathBuf {
        let backup_name = metadata
            .backup_path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown");
        self.backup_dir.join(format!("{}.json", backup_name))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    #[test]
    fn test_create_and_list_backups() {
        let temp_dir = TempDir::new().unwrap();
        let backup_dir = temp_dir.path().join("backups");
        let manager = RollbackManager::new(backup_dir.clone()).unwrap();

        // Create a test app
        let app_path = temp_dir.path().join("test.app");
        std::fs::create_dir(&app_path).unwrap();
        let test_file = app_path.join("test.txt");
        std::fs::write(&test_file, "test content").unwrap();

        // Create backup
        let metadata = manager.create_backup(&app_path, "1.0.0").unwrap();
        assert!(metadata.backup_path.exists());

        // List backups
        let backups = manager.list_backups().unwrap();
        assert_eq!(backups.len(), 1);
        assert_eq!(backups[0].version, "1.0.0");
    }

    #[test]
    fn test_cleanup_old_backups() {
        let temp_dir = TempDir::new().unwrap();
        let backup_dir = temp_dir.path().join("backups");
        let manager = RollbackManager::new(backup_dir.clone()).unwrap();

        let app_path = temp_dir.path().join("test.app");
        std::fs::create_dir(&app_path).unwrap();

        // Create multiple backups
        for i in 0..5 {
            std::thread::sleep(std::time::Duration::from_millis(10)); // Ensure different timestamps
            let _ = manager
                .create_backup(&app_path, &format!("1.0.{}", i))
                .unwrap();
        }

        assert_eq!(manager.list_backups().unwrap().len(), 5);

        // Keep only 2
        let removed = manager.cleanup_old_backups(2).unwrap();
        assert_eq!(removed, 3);
        assert_eq!(manager.list_backups().unwrap().len(), 2);
    }
}
