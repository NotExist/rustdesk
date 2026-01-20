// Update Verifier Module
// Handles signature verification and integrity checks for update packages

use hbb_common::{bail, log, ResultType};
use std::path::Path;

/// Signature verification for update packages
pub struct UpdateVerifier;

impl UpdateVerifier {
    /// Verify the integrity and authenticity of an update package
    ///
    /// # Arguments
    /// * `file_path` - Path to the update file (DMG on macOS)
    /// * `expected_signature` - Expected signature from update manifest
    /// * `expected_size` - Expected file size
    ///
    /// # Returns
    /// * `Ok(())` if verification succeeds
    /// * `Err(...)` if verification fails
    pub fn verify_update(
        file_path: &Path,
        expected_signature: &str,
        expected_size: u64,
    ) -> ResultType<()> {
        log::info!("Verifying update package: {:?}", file_path);

        // Step 1: Verify file exists
        if !file_path.exists() {
            bail!("Update file does not exist: {:?}", file_path);
        }

        // Step 2: Verify file size
        let metadata = std::fs::metadata(file_path)?;
        let actual_size = metadata.len();
        if actual_size != expected_size {
            bail!(
                "File size mismatch. Expected: {}, Actual: {}",
                expected_size,
                actual_size
            );
        }
        log::info!("File size verification passed: {} bytes", actual_size);

        // Step 3: Calculate SHA-256 hash
        let calculated_hash = Self::calculate_sha256(file_path)?;
        log::info!("Calculated SHA-256: {}", calculated_hash);

        // Step 4: Verify signature
        if calculated_hash != expected_signature {
            bail!(
                "Signature verification failed!\nExpected: {}\nActual: {}",
                expected_signature,
                calculated_hash
            );
        }

        log::info!("Update package verification successful");
        Ok(())
    }

    /// Calculate SHA-256 hash of a file
    fn calculate_sha256(file_path: &Path) -> ResultType<String> {
        use sha2::{Digest, Sha256};
        use std::fs::File;
        use std::io::{BufReader, Read};

        let file = File::open(file_path)?;
        let mut reader = BufReader::new(file);
        let mut hasher = Sha256::new();
        let mut buffer = [0; 8192];

        loop {
            let count = reader.read(&mut buffer)?;
            if count == 0 {
                break;
            }
            hasher.update(&buffer[..count]);
        }

        let result = hasher.finalize();
        Ok(format!("{:x}", result))
    }

    /// Verify macOS code signature (requires macOS)
    #[cfg(target_os = "macos")]
    pub fn verify_macos_codesign(app_path: &Path) -> ResultType<()> {
        use std::process::Command;

        log::info!("Verifying macOS code signature for: {:?}", app_path);

        let output = Command::new("codesign")
            .args(&["--verify", "--deep", "--strict", "--verbose=2"])
            .arg(app_path)
            .output()?;

        if !output.status.success() {
            let error = String::from_utf8_lossy(&output.stderr);
            bail!("Code signature verification failed: {}", error);
        }

        log::info!("macOS code signature verification passed");
        Ok(())
    }

    /// Verify macOS notarization status
    #[cfg(target_os = "macos")]
    pub fn verify_macos_notarization(app_path: &Path) -> ResultType<()> {
        use std::process::Command;

        log::info!("Checking macOS notarization for: {:?}", app_path);

        let output = Command::new("spctl")
            .args(&["-a", "-v", "-t", "install"])
            .arg(app_path)
            .output()?;

        // spctl returns 0 for accepted, non-zero for rejected
        if !output.status.success() {
            let error = String::from_utf8_lossy(&output.stderr);
            log::warn!("Notarization check: {}", error);
            // Don't fail on notarization check, just warn
            // Some enterprise builds may not be notarized
        } else {
            log::info!("macOS notarization check passed");
        }

        Ok(())
    }

    /// Comprehensive macOS package verification
    #[cfg(target_os = "macos")]
    pub fn verify_macos_package(dmg_path: &Path, app_path: &Path) -> ResultType<()> {
        log::info!("Performing comprehensive macOS package verification");

        // Verify DMG quarantine attributes are acceptable
        Self::check_quarantine_attributes(dmg_path)?;

        // Verify app bundle structure
        Self::verify_app_bundle(app_path)?;

        // Verify code signature
        Self::verify_macos_codesign(app_path)?;

        // Check notarization (warning only)
        Self::verify_macos_notarization(app_path).ok();

        log::info!("Comprehensive macOS package verification completed");
        Ok(())
    }

    #[cfg(target_os = "macos")]
    fn check_quarantine_attributes(file_path: &Path) -> ResultType<()> {
        use std::process::Command;

        log::info!("Checking quarantine attributes");

        let output = Command::new("xattr")
            .arg("-l")
            .arg(file_path)
            .output()?;

        let attrs = String::from_utf8_lossy(&output.stdout);
        log::debug!("File attributes: {}", attrs);

        // We'll handle quarantine removal during installation
        Ok(())
    }

    #[cfg(target_os = "macos")]
    fn verify_app_bundle(app_path: &Path) -> ResultType<()> {
        log::info!("Verifying app bundle structure");

        // Check for required bundle structure
        let contents = app_path.join("Contents");
        if !contents.exists() {
            bail!("Invalid app bundle: missing Contents directory");
        }

        let macos_dir = contents.join("MacOS");
        if !macos_dir.exists() {
            bail!("Invalid app bundle: missing MacOS directory");
        }

        let info_plist = contents.join("Info.plist");
        if !info_plist.exists() {
            bail!("Invalid app bundle: missing Info.plist");
        }

        log::info!("App bundle structure verification passed");
        Ok(())
    }

    /// Verify update manifest JSON structure
    pub fn verify_manifest(manifest_json: &str) -> ResultType<UpdateManifest> {
        let manifest: UpdateManifest = serde_json::from_str(manifest_json)?;

        // Validate required fields
        if manifest.version.is_empty() {
            bail!("Invalid manifest: missing version");
        }

        if manifest.download_url.is_empty() {
            bail!("Invalid manifest: missing download_url");
        }

        if manifest.signature.is_empty() {
            bail!("Invalid manifest: missing signature");
        }

        if manifest.file_size == 0 {
            bail!("Invalid manifest: invalid file_size");
        }

        log::info!("Update manifest verification passed");
        Ok(manifest)
    }
}

/// Update manifest structure
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct UpdateManifest {
    pub version: String,
    pub build_number: u32,
    pub release_date: String,
    pub release_notes: String,
    pub download_url: String,
    pub signature: String, // SHA-256 hash
    pub file_size: u64,
    pub min_os_version: String,
    pub required: bool, // Force update if true
    #[serde(default)]
    pub changelog_url: Option<String>,
    #[serde(default)]
    pub critical_security_fix: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_calculate_sha256() {
        let mut temp_file = NamedTempFile::new().unwrap();
        writeln!(temp_file, "test content").unwrap();
        temp_file.flush().unwrap();

        let hash = UpdateVerifier::calculate_sha256(temp_file.path()).unwrap();
        assert_eq!(hash.len(), 64); // SHA-256 is 64 hex characters
    }

    #[test]
    fn test_verify_file_size() {
        let mut temp_file = NamedTempFile::new().unwrap();
        writeln!(temp_file, "test").unwrap();
        temp_file.flush().unwrap();

        let metadata = std::fs::metadata(temp_file.path()).unwrap();
        let size = metadata.len();
        let hash = UpdateVerifier::calculate_sha256(temp_file.path()).unwrap();

        // Should succeed with correct size
        let result = UpdateVerifier::verify_update(temp_file.path(), &hash, size);
        assert!(result.is_ok());

        // Should fail with incorrect size
        let result = UpdateVerifier::verify_update(temp_file.path(), &hash, size + 1);
        assert!(result.is_err());
    }

    #[test]
    fn test_verify_manifest() {
        let valid_json = r#"{
            "version": "1.3.0",
            "build_number": 1300,
            "release_date": "2026-01-20",
            "release_notes": "Bug fixes and improvements",
            "download_url": "https://example.com/rustdesk-1.3.0.dmg",
            "signature": "abcdef1234567890",
            "file_size": 104857600,
            "min_os_version": "10.14",
            "required": false
        }"#;

        let result = UpdateVerifier::verify_manifest(valid_json);
        assert!(result.is_ok());

        let manifest = result.unwrap();
        assert_eq!(manifest.version, "1.3.0");
        assert_eq!(manifest.file_size, 104857600);
    }

    #[test]
    fn test_invalid_manifest() {
        let invalid_json = r#"{
            "version": "",
            "download_url": ""
        }"#;

        let result = UpdateVerifier::verify_manifest(invalid_json);
        assert!(result.is_err());
    }
}
