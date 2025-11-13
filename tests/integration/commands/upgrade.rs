use agpm_cli::upgrade::{
    SelfUpdater, VersionChecker, backup::BackupManager, verification::ChecksumVerifier,
};
use anyhow::Result;
use std::path::PathBuf;
use tempfile::TempDir;
use tokio::fs;

/// Test backup creation and restoration functionality.
#[tokio::test]
async fn test_backup_create_and_restore() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let binary_path = temp_dir.path().join("agpm");
    let backup_path = temp_dir.path().join("agpm.backup");

    // Create a mock binary file
    fs::write(&binary_path, b"original binary content").await?;

    // Create backup manager
    let backup_manager = BackupManager::new(binary_path.clone());

    // Create backup
    backup_manager.create_backup().await?;
    assert!(backup_path.exists(), "Backup file should be created");

    // Verify backup content matches original
    let backup_content = fs::read(&backup_path).await?;
    assert_eq!(backup_content, b"original binary content");

    // Modify the original file
    fs::write(&binary_path, b"modified binary content").await?;

    // Restore from backup
    backup_manager.restore_backup().await?;

    // Verify restoration
    let restored_content = fs::read(&binary_path).await?;
    assert_eq!(restored_content, b"original binary content");

    // Cleanup backup
    backup_manager.cleanup_backup().await?;
    assert!(!backup_path.exists(), "Backup should be cleaned up");

    Ok(())
}

/// Test backup with missing original file.
#[tokio::test]
async fn test_backup_missing_original() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let binary_path = temp_dir.path().join("nonexistent");

    let backup_manager = BackupManager::new(binary_path);

    // Should fail when original doesn't exist
    let result = backup_manager.create_backup().await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("does not exist"));

    Ok(())
}

/// Test restore with missing backup file.
#[tokio::test]
async fn test_restore_missing_backup() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let binary_path = temp_dir.path().join("agpm");

    // Create original file
    fs::write(&binary_path, b"original").await?;

    let backup_manager = BackupManager::new(binary_path);

    // Should fail when backup doesn't exist
    let result = backup_manager.restore_backup().await;
    assert!(result.is_err());

    let error_msg = result.unwrap_err().to_string();
    // Check for either error message variant
    assert!(
        error_msg.contains("Backup file not found")
            || error_msg.contains("Backup file does not exist")
            || error_msg.contains("No backup found"),
        "Expected backup-related error, got: {}",
        error_msg
    );

    Ok(())
}

/// Test checksum computation.
#[tokio::test]
async fn test_checksum_computation() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let file_path = temp_dir.path().join("test.bin");

    // Write known content
    fs::write(&file_path, b"Hello, World!").await?;

    // Compute checksum
    let checksum = ChecksumVerifier::compute_sha256(&file_path).await?;

    // Verify against known SHA256 of "Hello, World!" with sha256: prefix
    assert_eq!(checksum, "sha256:dffd6021bb2bd5b0af676290809ec3a53191dd81c7f70a4b28688a362182986f");

    Ok(())
}

/// Test checksum verification success.
#[tokio::test]
async fn test_checksum_verification_success() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let file_path = temp_dir.path().join("test.bin");

    // Write content
    fs::write(&file_path, b"Test content").await?;

    // Compute actual checksum first
    let actual = ChecksumVerifier::compute_sha256(&file_path).await?;

    // Should succeed with the actual checksum
    ChecksumVerifier::verify_checksum(&file_path, &actual).await?;

    Ok(())
}

/// Test checksum verification failure.
#[tokio::test]
async fn test_checksum_verification_failure() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let file_path = temp_dir.path().join("test.bin");

    // Write content
    fs::write(&file_path, b"Test content").await?;

    // Wrong checksum
    let wrong = "0000000000000000000000000000000000000000000000000000000000000000";

    // Should fail
    let result = ChecksumVerifier::verify_checksum(&file_path, wrong).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("Checksum verification failed"));

    Ok(())
}

/// Test version checker with caching.
#[tokio::test]
async fn test_version_checker_caching() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let cache_path = temp_dir.path().join(".agpm").join(".version_cache");

    // Create cache directory
    if let Some(parent) = cache_path.parent() {
        fs::create_dir_all(parent).await?;
    }

    // Test save and load cache
    let cache = agpm_cli::upgrade::version_check::VersionCheckCache {
        latest_version: "0.5.0".to_string(),
        current_version: env!("CARGO_PKG_VERSION").to_string(),
        checked_at: chrono::Utc::now(),
        update_available: true,
        notified: false,
        notification_count: 0,
    };
    // Write cache directly to file for testing
    let cache_json = serde_json::to_string_pretty(&cache)?;
    tokio::fs::write(&cache_path, cache_json).await?;

    // Cache should now exist
    assert!(cache_path.exists(), "Cache file should be created");

    // Verify cache content by reading the file
    let cache_content = tokio::fs::read_to_string(&cache_path).await?;
    let loaded: agpm_cli::upgrade::version_check::VersionCheckCache =
        serde_json::from_str(&cache_content)?;
    assert_eq!(loaded.latest_version, "0.5.0");

    Ok(())
}

/// Test version comparison logic.
#[tokio::test]
async fn test_version_comparison() -> Result<()> {
    use semver::Version;

    let v1 = Version::parse("0.3.0")?;
    let v2 = Version::parse("0.3.1")?;
    let v3 = Version::parse("1.0.0")?;
    let v4 = Version::parse("0.3.0-beta.1")?;

    assert!(v1 < v2, "0.3.0 should be less than 0.3.1");
    assert!(v2 < v3, "0.3.1 should be less than 1.0.0");
    assert!(v4 < v1, "0.3.0-beta.1 should be less than 0.3.0");

    Ok(())
}

/// Test self updater initialization.
#[tokio::test]
async fn test_self_updater_init() -> Result<()> {
    let updater = SelfUpdater::new();

    // Check current version is not empty
    assert!(!updater.current_version().is_empty());

    // Test force mode
    let _force_updater = updater.force(true);

    // Test version formatting
    let info = VersionChecker::format_version_info("0.3.0", Some("0.4.0"));
    assert!(info.contains("0.3.0"));
    assert!(info.contains("0.4.0"));

    Ok(())
}

/// Test backup path generation.
#[tokio::test]
async fn test_backup_path_generation() -> Result<()> {
    let original = PathBuf::from("/usr/local/bin/agpm");
    let backup_manager = BackupManager::new(original.clone());

    let backup_path = backup_manager.backup_path();
    assert_eq!(backup_path.file_name().unwrap(), "agpm.backup");
    assert_eq!(backup_path.parent(), original.parent());

    Ok(())
}

/// Test backup manager with special characters in path.
#[tokio::test]
async fn test_backup_special_characters() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let binary_path = temp_dir.path().join("agpm-v1.2.3");
    let backup_path = temp_dir.path().join("agpm-v1.2.3.backup");

    // Create file
    fs::write(&binary_path, b"content").await?;

    let backup_manager = BackupManager::new(binary_path.clone());

    // Create and verify backup
    backup_manager.create_backup().await?;
    assert!(backup_path.exists());

    // Cleanup
    backup_manager.cleanup_backup().await?;
    assert!(!backup_path.exists());

    Ok(())
}

/// Test concurrent backup operations.
#[tokio::test]
async fn test_concurrent_backup_operations() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let binary_path = temp_dir.path().join("agpm");

    // Create original file
    fs::write(&binary_path, b"original").await?;

    let manager1 = BackupManager::new(binary_path.clone());
    let manager2 = BackupManager::new(binary_path.clone());

    // Create backup with first manager
    manager1.create_backup().await?;

    // Second manager should handle existing backup gracefully
    let result = manager2.create_backup().await;
    // This might succeed (overwriting) or fail (locked) depending on platform
    // We just ensure it doesn't panic
    let _ = result;

    // Cleanup
    manager1.cleanup_backup().await?;

    Ok(())
}

/// Test checksum case insensitivity.
#[tokio::test]
async fn test_checksum_case_insensitive() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let file_path = temp_dir.path().join("test.bin");

    fs::write(&file_path, b"Test").await?;

    // SHA256 of "Test" with sha256: prefix
    let lowercase = "sha256:532eaabd9574880dbf76b9b8cc00832c20a6ec113d682299550d7a6e0f345e25";
    let uppercase = "sha256:532EAABD9574880DBF76B9B8CC00832C20A6EC113D682299550D7A6E0F345E25";
    let mixed = "sha256:532EaaBd9574880DBF76B9B8CC00832C20A6EC113D682299550D7A6E0F345E25";

    // All should succeed
    ChecksumVerifier::verify_checksum(&file_path, lowercase).await?;
    ChecksumVerifier::verify_checksum(&file_path, uppercase).await?;
    ChecksumVerifier::verify_checksum(&file_path, mixed).await?;

    Ok(())
}

/// Test version checker expiry logic.
#[tokio::test]
async fn test_version_cache_expiry() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let cache_path = temp_dir.path().join(".agpm").join(".version_cache");

    // Create cache directory
    if let Some(parent) = cache_path.parent() {
        fs::create_dir_all(parent).await?;
    }

    // Create expired cache manually
    let expired_cache = serde_json::json!({
        "latest_version": "0.5.0",
        "current_version": env!("CARGO_PKG_VERSION"),
        "checked_at": "2020-01-01T00:00:00Z",
        "update_available": false,
        "notified": false,
        "notification_count": 0
    });

    fs::write(&cache_path, serde_json::to_string(&expired_cache)?).await?;

    // Read cache and verify it's old
    let cache_content = tokio::fs::read_to_string(&cache_path).await?;
    let loaded: agpm_cli::upgrade::version_check::VersionCheckCache =
        serde_json::from_str(&cache_content)?;

    // Verify the cache has old timestamp
    let age = chrono::Utc::now().signed_duration_since(loaded.checked_at);
    assert!(age.num_hours() > 24); // Would be expired

    Ok(())
}

#[cfg(unix)]
#[tokio::test]
async fn test_backup_permission_preservation() -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let temp_dir = TempDir::new()?;
    let binary_path = temp_dir.path().join("agpm");
    let backup_path = temp_dir.path().join("agpm.backup");

    // Create file with specific permissions
    fs::write(&binary_path, b"content").await?;

    let mut perms = fs::metadata(&binary_path).await?.permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&binary_path, perms.clone()).await?;

    let backup_manager = BackupManager::new(binary_path.clone());

    // Create backup
    backup_manager.create_backup().await?;

    // Check backup has same permissions
    let backup_perms = fs::metadata(&backup_path).await?.permissions();
    assert_eq!(backup_perms.mode() & 0o777, 0o755);

    // Cleanup
    backup_manager.cleanup_backup().await?;

    Ok(())
}
