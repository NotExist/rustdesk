# RustDesk Update System Documentation

## Overview

RustDesk implements a comprehensive, enterprise-grade update system designed for safe, seamless, and user-friendly application updates across all platforms, with special focus on macOS.

## Table of Contents

1. [Architecture](#architecture)
2. [Components](#components)
3. [Update Workflow](#update-workflow)
4. [API Reference](#api-reference)
5. [Configuration](#configuration)
6. [Security](#security)
7. [macOS Privileged Helper Tool](#macos-privileged-helper-tool)
8. [Flutter Integration](#flutter-integration)
9. [Troubleshooting](#troubleshooting)

---

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                      Flutter UI Layer                        │
│  - Update notifications                                      │
│  - Progress display                                          │
│  - Configuration interface                                   │
└─────────────────────┬───────────────────────────────────────┘
                      │ FFI Bindings
┌─────────────────────▼───────────────────────────────────────┐
│                    Update Manager (Rust)                     │
│  - Orchestrates entire update process                        │
│  - State management                                          │
│  - Connection checking                                       │
└──────┬────────┬─────────┬──────────┬────────────────────────┘
       │        │         │          │
   ┌───▼───┐┌──▼────┐┌───▼────┐┌────▼────┐
   │Config ││Verify ││Rollback││Download │
   │Module ││Module ││Module  ││Manager  │
   └───────┘└───────┘└────────┘└─────────┘
                      │
              ┌───────▼───────┐
              │Platform Layer │
              │  - macOS      │
              │  - Windows    │
              │  - Linux      │
              └───────────────┘
```

## Components

### 1. Update Configuration (`update_config.rs`)

Manages all update-related settings and preferences.

**Key Features:**
- Check frequency control (Never/Daily/Weekly/Monthly/OnStartup)
- Installation timing (Immediate/WhenIdle/Scheduled/Manual)
- Update channels (Stable/Beta/Nightly)
- Download retry and timeout configuration
- Persistent storage of preferences

**Example Configuration:**
```rust
UpdateConfig {
    auto_check: true,
    check_frequency: UpdateCheckFrequency::Daily,
    auto_download: true,
    install_timing: UpdateInstallTiming::WhenIdle,
    channel: UpdateChannel::Stable,
    max_retries: 3,
    timeout_secs: 300,
    keep_backup: true,
    max_backups: 2,
}
```

### 2. Signature Verifier (`update_verifier.rs`)

Ensures update packages are authentic and unmodified.

**Verification Steps:**
1. File existence check
2. File size validation
3. SHA-256 hash calculation and verification
4. macOS code signature verification (codesign)
5. macOS notarization check (warning only)
6. App bundle structure validation

**Security Features:**
- Multi-layer verification
- Protection against supply chain attacks
- Validation of update manifest structure

### 3. Rollback Manager (`update_rollback.rs`)

Provides backup and recovery functionality.

**Features:**
- Automatic backup creation before updates
- Timestamp-based backup naming
- Metadata tracking (version, size, paths)
- Integrity verification before restore
- Automatic cleanup of old backups
- Platform-specific backup methods (ditto on macOS)

**Backup Metadata:**
```rust
BackupMetadata {
    version: String,
    backup_time: u64, // Unix timestamp
    backup_path: PathBuf,
    original_path: PathBuf,
    file_size: u64,
}
```

### 4. Update Manager (`update_manager.rs`)

Core coordinator for the entire update system.

**Responsibilities:**
- Check for updates from server
- Download management with progress monitoring
- Verification integration
- Backup creation before installation
- Connection state checking
- Safe installation orchestration
- Rollback support

**State Machine:**
```
Idle → Checking → Available → Downloading → Downloaded →
Verifying → ReadyToInstall → Installing → Completed
                                ↓ (on error)
                             Failed → (can retry or rollback)
```

---

## Update Workflow

### Standard Update Flow

```
1. CHECK FOR UPDATES
   ↓
   UpdateManager::check_for_updates()
   ↓
   Fetch manifest from server
   ↓
   Compare versions
   ↓
   [New version available?]
   ├─ Yes → State: Available
   └─ No  → State: Idle

2. DOWNLOAD UPDATE (if auto_download enabled)
   ↓
   UpdateManager::download_update()
   ↓
   Download to temp directory
   ↓
   Monitor progress
   ↓
   State: Downloading → Downloaded

3. VERIFY UPDATE
   ↓
   UpdateVerifier::verify_update()
   ↓
   Check SHA-256 hash
   ↓
   Verify file size
   ↓
   [macOS] Verify code signature
   ↓
   State: Verifying → ReadyToInstall

4. CHECK INSTALLATION TIMING
   ↓
   [Are connections active?]
   ├─ Yes → Wait for idle or user action
   └─ No  → Proceed to installation

5. CREATE BACKUP
   ↓
   RollbackManager::create_backup()
   ↓
   Copy current application
   ↓
   Save metadata
   ↓
   Cleanup old backups

6. INSTALL UPDATE
   ↓
   UpdateManager::install_update()
   ↓
   Extract update package
   ↓
   [Platform-specific installation]
   ├─ macOS → Extract DMG → update_me()
   ├─ Windows → Run installer
   └─ Linux → Package manager
   ↓
   State: Installing → Completed

7. RESTART APPLICATION
   ↓
   New version runs
```

### Rollback Flow

```
1. User or system triggers rollback
   ↓
2. RollbackManager::restore_backup()
   ↓
3. Verify backup integrity
   ↓
4. Remove current application
   ↓
5. Restore from backup
   ↓
6. User restarts application
   ↓
7. Previous version runs
```

---

## API Reference

### Rust API

#### Update Manager

```rust
// Initialize the update system
UpdateManager::init() -> ResultType<()>

// Check for updates
UpdateManager::check_for_updates() -> ResultType<Option<UpdateManifest>>

// Download update
UpdateManager::download_update() -> ResultType<()>

// Install update
UpdateManager::install_update() -> ResultType<()>

// Cancel download
UpdateManager::cancel_download() -> ResultType<()>

// Get current state
UpdateManager::get_state() -> UpdateState

// Update configuration
UpdateManager::update_config(config: UpdateConfig) -> ResultType<()>

// Rollback to backup
UpdateManager::rollback_to_backup(index: usize) -> ResultType<()>
```

#### Connection Checking

```rust
// Check if any connections are active
has_no_active_conns() -> bool
```

### Flutter FFI API

```dart
// Get current update state
String mainGetUpdateState()

// Check for updates
void mainCheckForUpdates()

// Download update
void mainDownloadUpdate()

// Install update
void mainInstallUpdate()

// Cancel download
bool mainCancelDownload()

// Get configuration
String mainGetUpdateConfig()

// Set configuration
bool mainSetUpdateConfig(String configJson)

// Rollback
void mainRollbackUpdate(int backupIndex)
```

### Flutter Events

The update system emits the following events via `push_global_event`:

```dart
// Update available
{
  "name": "update_available",
  "version": "1.3.0",
  "release_notes": "...",
  "file_size": 104857600,
  "required": false
}

// Update not available
{
  "name": "update_not_available"
}

// Update check failed
{
  "name": "update_check_failed",
  "error": "Network error"
}

// Download failed
{
  "name": "download_failed",
  "error": "Connection timeout"
}

// Update installed
{
  "name": "update_installed"
}

// Install failed
{
  "name": "install_failed",
  "error": "Verification failed"
}

// Update blocked (connections active)
{
  "name": "update_blocked",
  "error": "Cannot update while connections are active..."
}

// Rollback success
{
  "name": "rollback_success"
}

// Rollback failed
{
  "name": "rollback_failed",
  "error": "Backup not found"
}
```

---

## Configuration

### User-Accessible Settings

```json
{
  "auto_check": true,
  "check_frequency": "Daily",
  "auto_download": true,
  "install_timing": "WhenIdle",
  "channel": "Stable",
  "max_retries": 3,
  "timeout_secs": 300,
  "allow_metered": false,
  "min_idle_time": 300,
  "show_notifications": true,
  "keep_backup": true,
  "max_backups": 2
}
```

### Check Frequency Options

- **Never**: Disable automatic checks
- **Daily**: Check once per day
- **Weekly**: Check once per week
- **Monthly**: Check once per month
- **OnStartup**: Check on application start

### Install Timing Options

- **Immediate**: Install immediately (interrupts connections)
- **WhenIdle**: Install when no connections are active (recommended)
- **Scheduled**: Install at specific time
- **Manual**: User must manually trigger installation

### Update Channels

- **Stable**: Production releases (recommended)
- **Beta**: Pre-release versions for testing
- **Nightly**: Daily builds with latest features

---

## Security

### Threat Model

The update system protects against:

1. **Man-in-the-Middle (MITM) Attacks**
   - SHA-256 signature verification
   - HTTPS for all update downloads

2. **Supply Chain Attacks**
   - Code signature verification (macOS)
   - Notarization check (macOS)
   - Bundle structure validation

3. **Corrupted Downloads**
   - File size verification
   - Hash verification
   - Integrity checks before installation

4. **Unauthorized Updates**
   - Connection state checking
   - User consent required
   - Configuration controls

### Verification Process

```rust
// Step 1: Verify file size
if actual_size != expected_size {
    return Err("Size mismatch");
}

// Step 2: Calculate and verify SHA-256
let calculated_hash = calculate_sha256(file_path)?;
if calculated_hash != expected_signature {
    return Err("Signature verification failed");
}

// Step 3: macOS-specific checks
#[cfg(target_os = "macos")]
{
    verify_macos_codesign(app_path)?;
    verify_macos_notarization(app_path).ok(); // Warning only
    verify_app_bundle(app_path)?;
}
```

### Update Manifest

```json
{
  "version": "1.3.0",
  "build_number": 1300,
  "release_date": "2026-01-20",
  "release_notes": "Bug fixes and improvements",
  "download_url": "https://github.com/rustdesk/rustdesk/releases/...",
  "signature": "sha256_hash_here",
  "file_size": 104857600,
  "min_os_version": "10.14",
  "required": false,
  "changelog_url": "https://...",
  "critical_security_fix": false
}
```

---

## macOS Privileged Helper Tool

### Current Implementation

RustDesk currently requires administrator privileges for each update because:
- Updates overwrite `/Applications/RustDesk.app`
- System LaunchDaemons need modification
- Process termination requires privileges

### Recommended: Implementing Privileged Helper

For truly unattended updates on macOS, implement a privileged helper tool using SMJobBless or SMAppService.

#### Architecture

```
RustDesk.app (User space)
    ↕ XPC Connection
RustDeskUpdateHelper (Root daemon)
```

#### Implementation Steps

**1. Create Helper Tool Project**

```swift
// RustDeskUpdateHelper/main.swift
import Foundation

class UpdateHelper: NSObject, UpdateHelperProtocol {
    func installUpdate(from dmgPath: String,
                       reply: @escaping (Bool, String?) -> Void) {
        // Verify signature
        guard verifySignature(dmgPath) else {
            reply(false, "Signature verification failed")
            return
        }

        // Perform installation
        let success = performUpdate(dmgPath)
        reply(success, success ? nil : "Installation failed")
    }

    private func verifySignature(_ path: String) -> Bool {
        // Use codesign to verify
        let task = Process()
        task.launchPath = "/usr/bin/codesign"
        task.arguments = ["--verify", "--deep", "--strict", path]
        task.launch()
        task.waitUntilExit()
        return task.terminationStatus == 0
    }

    private func performUpdate(_ dmgPath: String) -> Bool {
        // Mount DMG, copy files, unmount
        // Implementation details...
        return true
    }
}
```

**2. Define XPC Protocol**

```swift
@objc protocol UpdateHelperProtocol {
    func installUpdate(from dmgPath: String,
                       reply: @escaping (Bool, String?) -> Void)
}
```

**3. Install Helper (one-time)**

```swift
// In main application
func installUpdateHelper() {
    var authRef: AuthorizationRef?
    let status = AuthorizationCreate(nil, nil, [], &authRef)
    guard status == errAuthorizationSuccess else { return }

    var error: Unmanaged<CFError>?
    let success = SMJobBless(
        kSMDomainSystemLaunchd,
        "com.rustdesk.UpdateHelper" as CFString,
        authRef,
        &error
    )

    if !success {
        print("Failed to install helper: \(error!.takeRetainedValue())")
    }
}
```

**4. Use Helper from Rust**

```rust
#[cfg(target_os = "macos")]
fn install_update_via_helper(dmg_path: &Path) -> ResultType<()> {
    use std::process::Command;

    // Connect to helper via XPC (requires FFI or Swift wrapper)
    // For simplicity, use command-line tool wrapper

    let output = Command::new("/Library/PrivilegedHelperTools/RustDeskUpdateHelper")
        .arg("install")
        .arg(dmg_path)
        .output()?;

    if !output.status.success() {
        bail!("Helper installation failed");
    }

    Ok(())
}
```

#### Benefits

- ✅ One-time authorization
- ✅ Unattended updates
- ✅ No user interruption
- ✅ Apple-approved method
- ✅ Secure communication via XPC

#### Resources

- [Apple SMJobBless Documentation](https://developer.apple.com/documentation/servicemanagement/1431078-smjobbless)
- [EvenBetterAuthorizationSample](https://developer.apple.com/library/archive/samplecode/EvenBetterAuthorizationSample/)
- [SwiftPrivilegedHelper Example](https://github.com/erikberglund/SwiftPrivilegedHelper)

---

## Flutter Integration

### Update UI Example

```dart
class UpdateScreen extends StatefulWidget {
  @override
  _UpdateScreenState createState() => _UpdateScreenState();
}

class _UpdateScreenState extends State<UpdateScreen> {
  String _updateState = 'Idle';

  @override
  void initState() {
    super.initState();
    // Listen for update events
    EventBus.on('global_event', _handleUpdateEvent);

    // Get current state
    _refreshState();
  }

  void _refreshState() {
    final stateJson = platform.mainGetUpdateState();
    final state = jsonDecode(stateJson);
    setState(() {
      _updateState = state['state'] ?? 'Idle';
    });
  }

  void _handleUpdateEvent(Map<String, dynamic> event) {
    if (event['name'] == 'update_available') {
      _showUpdateDialog(event);
    } else if (event['name'] == 'update_blocked') {
      _showBlockedDialog(event['error']);
    }
    _refreshState();
  }

  void _checkForUpdates() {
    platform.mainCheckForUpdates();
  }

  void _downloadUpdate() {
    platform.mainDownloadUpdate();
  }

  void _installUpdate() {
    // Check for active connections
    if (hasActiveConnections()) {
      _showWarningDialog();
    } else {
      platform.mainInstallUpdate();
    }
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(title: Text('Update')),
      body: Column(
        children: [
          Text('Status: $_updateState'),
          ElevatedButton(
            onPressed: _checkForUpdates,
            child: Text('Check for Updates'),
          ),
          if (_updateState == 'Available')
            ElevatedButton(
              onPressed: _downloadUpdate,
              child: Text('Download'),
            ),
          if (_updateState == 'ReadyToInstall')
            ElevatedButton(
              onPressed: _installUpdate,
              child: Text('Install Now'),
            ),
        ],
      ),
    );
  }
}
```

---

## Troubleshooting

### Common Issues

#### 1. Update Check Fails

**Symptoms:** "update_check_failed" event

**Causes:**
- Network connectivity issues
- Manifest server unreachable
- Invalid manifest JSON

**Solutions:**
- Check network connection
- Verify manifest URL in logs
- Try manual download

#### 2. Download Fails

**Symptoms:** Download stalls or "download_failed" event

**Causes:**
- Network timeout
- Insufficient disk space
- Corrupted download

**Solutions:**
- Check available disk space
- Retry download
- Verify download URL

#### 3. Verification Fails

**Symptoms:** "Signature verification failed"

**Causes:**
- Corrupted download
- Tampered update package
- Incorrect signature in manifest

**Solutions:**
- Delete downloaded file and retry
- Verify signature matches manifest
- Contact support if persistent

#### 4. Installation Blocked

**Symptoms:** "update_blocked" event

**Causes:**
- Active remote connections
- Connection check malfunction

**Solutions:**
- Close all connections
- Wait for idle period
- Use manual installation timing

#### 5. Rollback Needed

**Symptoms:** New version causes issues

**Solutions:**
```rust
// List available backups
let manager = RollbackManager::new(backup_dir)?;
let backups = manager.list_backups()?;

// Rollback to most recent backup
UpdateManager::rollback_to_backup(0)?;

// Restart application
```

### Debug Logging

Enable verbose logging:

```rust
log::set_max_level(log::LevelFilter::Debug);
```

Key log messages to look for:
- `"Update manager initialized"` - System ready
- `"Checking for updates..."` - Check started
- `"New version available: X.Y.Z"` - Update found
- `"Update package verified successfully"` - Verification passed
- `"Backup created successfully"` - Backup complete
- `"Update installed successfully"` - Installation complete

---

## Best Practices

### For Users

1. **Enable Auto-Updates**
   - Set `auto_check: true` and `auto_download: true`
   - Use `install_timing: WhenIdle` for seamless updates

2. **Keep Backups**
   - Set `keep_backup: true`
   - Maintain at least 2 backup versions

3. **Review Release Notes**
   - Check release notes before updating
   - Understand breaking changes

### For Developers

1. **Always Verify Updates**
   - Never skip signature verification
   - Validate manifest structure

2. **Handle Failures Gracefully**
   - Provide clear error messages
   - Implement automatic retry logic
   - Maintain rollback capability

3. **Test Update Path**
   - Test update from multiple versions
   - Verify rollback functionality
   - Test on all supported OS versions

4. **Version Manifest Carefully**
   - Use semantic versioning
   - Sign all release packages
   - Maintain accurate file sizes

---

## Future Enhancements

### Planned Features

1. **Delta Updates**
   - Download only changed files
   - Reduce bandwidth usage
   - Faster update application

2. **Background Updates**
   - Install updates without restart
   - Hot-reload capability (where possible)

3. **MDM Integration**
   - Enterprise update policies
   - Centralized management
   - Forced updates for critical security fixes

4. **Update Channels API**
   - Easy channel switching
   - Beta testing participation
   - Rollback to stable from beta

5. **Improved Progress Reporting**
   - Detailed stage information
   - Estimated time remaining
   - Network speed monitoring

---

## Support

For issues or questions:
- GitHub Issues: https://github.com/rustdesk/rustdesk/issues
- Documentation: https://rustdesk.com/docs
- Community Forum: https://rustdesk.com/forum

---

## License

This update system is part of RustDesk and follows the same license terms.
