#[cfg(test)]
#[allow(clippy::module_inception)]
mod tests {
    use super::super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_version_checker_cache() {
        let temp_dir = TempDir::new().unwrap();
        let checker = version_check::VersionChecker::new(temp_dir.path().to_path_buf());

        // Test no cache initially
        let cached = checker.get_cached_version().await.unwrap();
        assert!(cached.is_none());

        // Save version
        checker.save_version("1.2.3".to_string()).await.unwrap();

        // Check cached version
        let cached = checker.get_cached_version().await.unwrap();
        assert_eq!(cached, Some("1.2.3".to_string()));

        // Clear cache
        checker.clear_cache().await.unwrap();
        let cached = checker.get_cached_version().await.unwrap();
        assert!(cached.is_none());
    }

    #[tokio::test]
    async fn test_backup_manager() {
        let temp_dir = TempDir::new().unwrap();
        let test_file = temp_dir.path().join("test_binary");
        let test_content = b"test binary content";

        // Create test file
        tokio::fs::write(&test_file, test_content).await.unwrap();

        let manager = backup::BackupManager::new(test_file.clone());

        // Test backup creation
        assert!(!manager.backup_exists());
        manager.create_backup().await.unwrap();
        assert!(manager.backup_exists());

        // Modify original file
        tokio::fs::write(&test_file, b"modified content")
            .await
            .unwrap();

        // Test restore
        manager.restore_backup().await.unwrap();
        let restored_content = tokio::fs::read(&test_file).await.unwrap();
        assert_eq!(restored_content, test_content);

        // Test cleanup
        manager.cleanup_backup().await.unwrap();
        assert!(!manager.backup_exists());
    }

    #[test]
    fn test_upgrade_config_defaults() {
        let config = config::UpgradeConfig::default();
        assert!(!config.check_on_startup);
        assert_eq!(config.check_interval, 86400);
        assert!(config.auto_backup);
        assert!(config.verify_checksum);
    }

    #[test]
    fn test_version_format_info() {
        use version_check::VersionChecker;

        let info = VersionChecker::format_version_info("1.0.0", None);
        assert_eq!(info, "Current version: 1.0.0 (up to date)");

        let info = VersionChecker::format_version_info("1.0.0", Some("1.0.0"));
        assert_eq!(info, "Current version: 1.0.0 (up to date)");

        let info = VersionChecker::format_version_info("1.0.0", Some("1.1.0"));
        assert_eq!(
            info,
            "Current version: 1.0.0\nLatest version:  1.1.0 (update available)"
        );
    }

    #[test]
    fn test_self_updater_creation() {
        let updater = SelfUpdater::new();
        assert_eq!(updater.current_version(), env!("CARGO_PKG_VERSION"));

        let updater_forced = updater.force(true);
        // Just verify it builds correctly
        assert_eq!(updater_forced.current_version(), env!("CARGO_PKG_VERSION"));
    }

    #[tokio::test]
    async fn test_backup_path_generation() {
        let temp_dir = TempDir::new().unwrap();
        let binary_path = temp_dir.path().join("ccpm");
        let manager = backup::BackupManager::new(binary_path.clone());

        let backup_path = manager.backup_path();
        assert_eq!(backup_path.file_name().unwrap(), "ccpm.backup");
        assert_eq!(backup_path.parent().unwrap(), binary_path.parent().unwrap());
    }

    #[tokio::test]
    async fn test_backup_error_handling() {
        let temp_dir = TempDir::new().unwrap();
        let non_existent = temp_dir.path().join("non_existent");
        let manager = backup::BackupManager::new(non_existent);

        // Should fail when trying to backup non-existent file
        let result = manager.create_backup().await;
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Original file does not exist")
        );

        // Should fail when trying to restore without backup
        let result = manager.restore_backup().await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("No backup found"));
    }

    #[tokio::test]
    async fn test_version_cache_ttl() {
        use chrono::{Duration, Utc};
        use serde_json;

        let temp_dir = TempDir::new().unwrap();
        let cache_path = temp_dir.path().join("version_check_cache.json");

        // Create expired cache manually
        let old_cache = version_check::VersionCheckCache {
            latest_version: "1.0.0".to_string(),
            checked_at: Utc::now() - Duration::hours(2),
        };
        let content = serde_json::to_string(&old_cache).unwrap();
        tokio::fs::write(&cache_path, content).await.unwrap();

        // Check that expired cache is not used
        let checker =
            version_check::VersionChecker::new(temp_dir.path().to_path_buf()).with_ttl(3600); // 1 hour TTL

        let cached = checker.get_cached_version().await.unwrap();
        assert!(cached.is_none());
    }
}
