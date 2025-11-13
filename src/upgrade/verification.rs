use anyhow::{Context, Result, bail};
use sha2::{Digest, Sha256};
use std::path::Path;
use tokio::fs;
use tracing::{debug, info, warn};

/// Verifies the integrity of a downloaded binary using SHA256 checksum.
///
/// This module provides checksum verification functionality for downloaded binaries
/// to ensure they haven't been corrupted or tampered with during download.
///
/// # Security Benefits
///
/// - **Download Integrity**: Detects corrupted or incomplete downloads
/// - **Tamper Detection**: Identifies potentially modified binaries
/// - **Supply Chain Security**: Helps ensure binary authenticity
/// - **Network Reliability**: Catches network-induced corruption
pub struct ChecksumVerifier;

impl ChecksumVerifier {
    /// Compute the SHA256 checksum of a file.
    ///
    /// # Arguments
    ///
    /// * `file_path` - Path to the file to compute checksum for
    ///
    /// # Returns
    ///
    /// The hex-encoded SHA256 checksum string
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use agpm_cli::upgrade::verification::ChecksumVerifier;
    /// use std::path::Path;
    ///
    /// # async fn example() -> anyhow::Result<()> {
    /// let checksum = ChecksumVerifier::compute_sha256(Path::new("/path/to/binary")).await?;
    /// println!("SHA256: {}", checksum);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn compute_sha256(file_path: &Path) -> Result<String> {
        debug!("Computing SHA256 checksum for: {:?}", file_path);

        let contents = fs::read(file_path)
            .await
            .with_context(|| format!("Failed to read file: {file_path:?}"))?;

        let mut hasher = Sha256::new();
        hasher.update(&contents);
        let result = hasher.finalize();

        Ok(format!("sha256:{result:x}"))
    }

    /// Verify a file against an expected checksum.
    ///
    /// # Arguments
    ///
    /// * `file_path` - Path to the file to verify
    /// * `expected_checksum` - The expected SHA256 checksum (hex-encoded)
    ///
    /// # Returns
    ///
    /// - `Ok(())` if checksums match
    /// - `Err` if checksums don't match or verification fails
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use agpm_cli::upgrade::verification::ChecksumVerifier;
    /// use std::path::Path;
    ///
    /// # async fn example() -> anyhow::Result<()> {
    /// let file_path = Path::new("/path/to/binary");
    /// let expected = "abc123...";
    ///
    /// ChecksumVerifier::verify_checksum(file_path, expected).await?;
    /// println!("Checksum verified successfully!");
    /// # Ok(())
    /// # }
    /// ```
    pub async fn verify_checksum(file_path: &Path, expected_checksum: &str) -> Result<()> {
        info!("Verifying checksum for: {:?}", file_path);

        let actual_checksum = Self::compute_sha256(file_path).await?;

        // Case-insensitive comparison (checksums may be uppercase or lowercase)
        if actual_checksum.to_lowercase() != expected_checksum.to_lowercase() {
            bail!(
                "Checksum verification failed!\n  Expected: {expected_checksum}\n  Actual:   {actual_checksum}"
            );
        }

        info!("Checksum verification successful");
        Ok(())
    }

    /// Download and parse a checksums file from a GitHub release.
    ///
    /// GitHub releases often include a checksums.txt or SHA256SUMS file containing
    /// checksums for all release artifacts. This function downloads and parses such files.
    ///
    /// # Arguments
    ///
    /// * `checksums_url` - URL to the checksums file
    /// * `binary_name` - Name of the binary to find checksum for
    ///
    /// # Returns
    ///
    /// The expected checksum for the specified binary, or None if not found
    ///
    /// # Checksum File Format
    ///
    /// Expected format (one per line):
    /// ```text
    /// abc123def456...  agpm-linux-x86_64
    /// 789ghi012jkl...  agpm-macos-aarch64
    /// ```
    pub async fn fetch_expected_checksum(
        checksums_url: &str,
        binary_name: &str,
    ) -> Result<Option<String>> {
        debug!("Fetching checksums from: {}", checksums_url);

        // Use reqwest to download the checksums file
        let client = reqwest::Client::new();
        let response =
            client.get(checksums_url).send().await.context("Failed to fetch checksums file")?;

        if !response.status().is_success() {
            warn!("Failed to fetch checksums file: HTTP {}", response.status());
            return Ok(None);
        }

        let content = response.text().await.context("Failed to read checksums file content")?;

        // Parse the checksums file
        for line in content.lines() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() == 2 {
                let (checksum, filename) = (parts[0], parts[1]);

                // Check if this line is for our binary
                // More precise matching to avoid false positives like "agpm" matching "agpm-dev"
                if filename == binary_name
                    || filename.starts_with(&format!("{}-", binary_name))
                    || filename.ends_with(&format!("/{}", binary_name))
                {
                    debug!("Found checksum for {}: {}", binary_name, checksum);
                    return Ok(Some(checksum.to_string()));
                }
            }
        }

        warn!("No checksum found for binary: {}", binary_name);
        Ok(None)
    }

    /// Verify a downloaded binary using checksums from GitHub release.
    ///
    /// This is a convenience method that combines fetching the expected checksum
    /// and verifying the downloaded file.
    ///
    /// # Arguments
    ///
    /// * `file_path` - Path to the downloaded binary
    /// * `checksums_url` - URL to the checksums file in the GitHub release
    /// * `binary_name` - Name of the binary in the checksums file
    ///
    /// # Returns
    ///
    /// - `Ok(true)` if verification succeeded
    /// - `Ok(false)` if no checksum was available (verification skipped)
    /// - `Err` if verification failed
    pub async fn verify_from_release(
        file_path: &Path,
        checksums_url: &str,
        binary_name: &str,
    ) -> Result<bool> {
        if let Some(expected) = Self::fetch_expected_checksum(checksums_url, binary_name).await? {
            Self::verify_checksum(file_path, &expected).await?;
            Ok(true)
        } else {
            warn!("No checksum available for verification, skipping");
            Ok(false)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[tokio::test]
    async fn test_compute_sha256() {
        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(b"Hello, World!").unwrap();

        let checksum = ChecksumVerifier::compute_sha256(temp_file.path()).await.unwrap();

        // Known SHA256 of "Hello, World!" with sha256: prefix
        assert_eq!(
            checksum,
            "sha256:dffd6021bb2bd5b0af676290809ec3a53191dd81c7f70a4b28688a362182986f"
        );
    }

    #[tokio::test]
    async fn test_verify_checksum_success() {
        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(b"Test content").unwrap();

        // Compute actual checksum first
        let actual = ChecksumVerifier::compute_sha256(temp_file.path()).await.unwrap();

        // Now verify with the correct checksum
        ChecksumVerifier::verify_checksum(temp_file.path(), &actual).await.unwrap();
    }

    #[tokio::test]
    async fn test_verify_checksum_failure() {
        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(b"Test content").unwrap();

        let wrong_checksum = "0000000000000000000000000000000000000000000000000000000000000000";

        let result = ChecksumVerifier::verify_checksum(temp_file.path(), wrong_checksum).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Checksum verification failed"));
    }

    #[tokio::test]
    async fn test_verify_checksum_case_insensitive() {
        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(b"Test").unwrap();

        // SHA256 of "Test" with sha256: prefix
        let lowercase = "sha256:532eaabd9574880dbf76b9b8cc00832c20a6ec113d682299550d7a6e0f345e25";
        let uppercase = "sha256:532EAABD9574880DBF76B9B8CC00832C20A6EC113D682299550D7A6E0F345E25";

        // Both should succeed
        ChecksumVerifier::verify_checksum(temp_file.path(), lowercase).await.unwrap();
        ChecksumVerifier::verify_checksum(temp_file.path(), uppercase).await.unwrap();
    }
}
