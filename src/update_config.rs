// Update Configuration Module
// Manages all update-related settings and preferences

use hbb_common::{config::Config, log, ResultType};
use serde_derive::{Deserialize, Serialize};
use std::time::Duration;

/// Update check frequency
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum UpdateCheckFrequency {
    Never,
    Daily,
    Weekly,
    Monthly,
    OnStartup,
}

impl Default for UpdateCheckFrequency {
    fn default() -> Self {
        UpdateCheckFrequency::Daily
    }
}

/// Update installation timing
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum UpdateInstallTiming {
    Immediate,           // Install immediately (will interrupt connections)
    WhenIdle,            // Install when no connections are active
    Scheduled(i64),      // Install at specific timestamp (Unix epoch)
    Manual,              // User must manually trigger installation
}

impl Default for UpdateInstallTiming {
    fn default() -> Self {
        UpdateInstallTiming::WhenIdle
    }
}

/// Update channel
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum UpdateChannel {
    Stable,
    Beta,
    Nightly,
}

impl Default for UpdateChannel {
    fn default() -> Self {
        UpdateChannel::Stable
    }
}

/// Comprehensive update configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateConfig {
    /// Enable automatic update checking
    pub auto_check: bool,

    /// Check frequency
    pub check_frequency: UpdateCheckFrequency,

    /// Automatically download updates
    pub auto_download: bool,

    /// Update installation timing
    pub install_timing: UpdateInstallTiming,

    /// Update channel
    pub channel: UpdateChannel,

    /// Maximum download retries
    pub max_retries: u32,

    /// Connection timeout in seconds
    pub timeout_secs: u64,

    /// Allow updates over metered connections
    pub allow_metered: bool,

    /// Minimum idle time before auto-update (in seconds)
    pub min_idle_time: u64,

    /// Enable update notifications
    pub show_notifications: bool,

    /// Keep old version for rollback
    pub keep_backup: bool,

    /// Maximum number of backup versions to keep
    pub max_backups: usize,
}

impl Default for UpdateConfig {
    fn default() -> Self {
        Self {
            auto_check: true,
            check_frequency: UpdateCheckFrequency::Daily,
            auto_download: true,
            install_timing: UpdateInstallTiming::WhenIdle,
            channel: UpdateChannel::Stable,
            max_retries: 3,
            timeout_secs: 300,
            allow_metered: false,
            min_idle_time: 300, // 5 minutes
            show_notifications: true,
            keep_backup: true,
            max_backups: 2,
        }
    }
}

impl UpdateConfig {
    const KEY_AUTO_CHECK: &'static str = "update_auto_check";
    const KEY_CHECK_FREQUENCY: &'static str = "update_check_frequency";
    const KEY_AUTO_DOWNLOAD: &'static str = "update_auto_download";
    const KEY_INSTALL_TIMING: &'static str = "update_install_timing";
    const KEY_CHANNEL: &'static str = "update_channel";
    const KEY_MAX_RETRIES: &'static str = "update_max_retries";
    const KEY_TIMEOUT: &'static str = "update_timeout_secs";
    const KEY_ALLOW_METERED: &'static str = "update_allow_metered";
    const KEY_MIN_IDLE_TIME: &'static str = "update_min_idle_time";
    const KEY_SHOW_NOTIFICATIONS: &'static str = "update_show_notifications";
    const KEY_KEEP_BACKUP: &'static str = "update_keep_backup";
    const KEY_MAX_BACKUPS: &'static str = "update_max_backups";

    /// Load configuration from storage
    pub fn load() -> Self {
        let config = Config::get();

        Self {
            auto_check: config
                .get(Self::KEY_AUTO_CHECK)
                .and_then(|v| v.parse().ok())
                .unwrap_or(true),
            check_frequency: config
                .get(Self::KEY_CHECK_FREQUENCY)
                .and_then(|v| serde_json::from_str(&v).ok())
                .unwrap_or_default(),
            auto_download: config
                .get(Self::KEY_AUTO_DOWNLOAD)
                .and_then(|v| v.parse().ok())
                .unwrap_or(true),
            install_timing: config
                .get(Self::KEY_INSTALL_TIMING)
                .and_then(|v| serde_json::from_str(&v).ok())
                .unwrap_or_default(),
            channel: config
                .get(Self::KEY_CHANNEL)
                .and_then(|v| serde_json::from_str(&v).ok())
                .unwrap_or_default(),
            max_retries: config
                .get(Self::KEY_MAX_RETRIES)
                .and_then(|v| v.parse().ok())
                .unwrap_or(3),
            timeout_secs: config
                .get(Self::KEY_TIMEOUT)
                .and_then(|v| v.parse().ok())
                .unwrap_or(300),
            allow_metered: config
                .get(Self::KEY_ALLOW_METERED)
                .and_then(|v| v.parse().ok())
                .unwrap_or(false),
            min_idle_time: config
                .get(Self::KEY_MIN_IDLE_TIME)
                .and_then(|v| v.parse().ok())
                .unwrap_or(300),
            show_notifications: config
                .get(Self::KEY_SHOW_NOTIFICATIONS)
                .and_then(|v| v.parse().ok())
                .unwrap_or(true),
            keep_backup: config
                .get(Self::KEY_KEEP_BACKUP)
                .and_then(|v| v.parse().ok())
                .unwrap_or(true),
            max_backups: config
                .get(Self::KEY_MAX_BACKUPS)
                .and_then(|v| v.parse().ok())
                .unwrap_or(2),
        }
    }

    /// Save configuration to storage
    pub fn save(&self) -> ResultType<()> {
        let mut config = Config::get();

        config.set(Self::KEY_AUTO_CHECK, &self.auto_check.to_string());
        config.set(
            Self::KEY_CHECK_FREQUENCY,
            &serde_json::to_string(&self.check_frequency)?,
        );
        config.set(Self::KEY_AUTO_DOWNLOAD, &self.auto_download.to_string());
        config.set(
            Self::KEY_INSTALL_TIMING,
            &serde_json::to_string(&self.install_timing)?,
        );
        config.set(
            Self::KEY_CHANNEL,
            &serde_json::to_string(&self.channel)?,
        );
        config.set(Self::KEY_MAX_RETRIES, &self.max_retries.to_string());
        config.set(Self::KEY_TIMEOUT, &self.timeout_secs.to_string());
        config.set(Self::KEY_ALLOW_METERED, &self.allow_metered.to_string());
        config.set(Self::KEY_MIN_IDLE_TIME, &self.min_idle_time.to_string());
        config.set(
            Self::KEY_SHOW_NOTIFICATIONS,
            &self.show_notifications.to_string(),
        );
        config.set(Self::KEY_KEEP_BACKUP, &self.keep_backup.to_string());
        config.set(Self::KEY_MAX_BACKUPS, &self.max_backups.to_string());

        Config::set(config);
        log::info!("Update configuration saved");
        Ok(())
    }

    /// Get check interval as Duration
    pub fn check_interval(&self) -> Duration {
        match self.check_frequency {
            UpdateCheckFrequency::Never => Duration::from_secs(u64::MAX),
            UpdateCheckFrequency::Daily => Duration::from_secs(60 * 60 * 24),
            UpdateCheckFrequency::Weekly => Duration::from_secs(60 * 60 * 24 * 7),
            UpdateCheckFrequency::Monthly => Duration::from_secs(60 * 60 * 24 * 30),
            UpdateCheckFrequency::OnStartup => Duration::from_secs(0),
        }
    }

    /// Check if update should be installed now based on timing configuration
    pub fn should_install_now(&self) -> bool {
        match self.install_timing {
            UpdateInstallTiming::Immediate => true,
            UpdateInstallTiming::WhenIdle => {
                // Checked by caller (connection state)
                false
            }
            UpdateInstallTiming::Scheduled(timestamp) => {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs() as i64)
                    .unwrap_or(0);
                now >= timestamp
            }
            UpdateInstallTiming::Manual => false,
        }
    }
}

/// Update state tracking
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum UpdateState {
    Idle,
    Checking,
    Available {
        version: String,
        release_notes: String,
        download_url: String,
        signature: String,
        file_size: u64,
    },
    Downloading {
        version: String,
        downloaded: u64,
        total: u64,
        speed_bps: u64,
    },
    Downloaded {
        version: String,
        file_path: String,
    },
    Verifying {
        version: String,
    },
    ReadyToInstall {
        version: String,
        file_path: String,
    },
    Installing {
        version: String,
        progress: u8,
    },
    Completed {
        version: String,
    },
    Failed {
        error: String,
        retry_count: u32,
    },
    RollingBack {
        from_version: String,
        to_version: String,
    },
}

impl Default for UpdateState {
    fn default() -> Self {
        UpdateState::Idle
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = UpdateConfig::default();
        assert!(config.auto_check);
        assert_eq!(config.check_frequency, UpdateCheckFrequency::Daily);
        assert!(config.auto_download);
    }

    #[test]
    fn test_check_interval() {
        let config = UpdateConfig::default();
        let daily_interval = config.check_interval();
        assert_eq!(daily_interval.as_secs(), 60 * 60 * 24);
    }

    #[test]
    fn test_scheduled_install() {
        let future_time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64
            + 3600; // 1 hour from now

        let mut config = UpdateConfig::default();
        config.install_timing = UpdateInstallTiming::Scheduled(future_time);
        assert!(!config.should_install_now());

        config.install_timing = UpdateInstallTiming::Scheduled(0);
        assert!(config.should_install_now());
    }
}
