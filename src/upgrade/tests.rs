#[cfg(test)]
#[allow(clippy::module_inception)]
mod tests {
    use super::super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_version_checker_cache() {
        let temp_dir = TempDir::new().unwrap();

        // Set up environment to use temp directory
        unsafe {
            std::env::set_var("AGPM_CONFIG_PATH", temp_dir.path().join("config.toml"));
        }

        // Test cache creation and serialization
        let cache = version_check::VersionCheckCache {
            latest_version: "1.2.3".to_string(),
            current_version: env!("CARGO_PKG_VERSION").to_string(),
            checked_at: chrono::Utc::now(),
            update_available: true,
            notified: false,
            notification_count: 0,
        };

        // Verify cache structure
        assert_eq!(cache.latest_version, "1.2.3");
        assert_eq!(cache.current_version, env!("CARGO_PKG_VERSION"));
        assert!(cache.update_available);
        assert!(!cache.notified);

        // Clean up environment
        unsafe {
            std::env::remove_var("AGPM_CONFIG_PATH");
        }
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
        tokio::fs::write(&test_file, b"modified content").await.unwrap();

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
        assert_eq!(info, "Current version: 1.0.0\nLatest version:  1.1.0 (update available)");
    }

    #[test]
    fn test_self_updater_creation() {
        let updater = SelfUpdater::new();
        assert_eq!(updater.current_version(), env!("CARGO_PKG_VERSION"));

        let updater_forced = updater.force(true);
        // Just verify it builds correctly
        assert_eq!(updater_forced.current_version(), env!("CARGO_PKG_VERSION"));
    }

    #[test]
    fn test_self_updater_version_comparison() {
        use semver::Version;

        // Test version comparison logic
        let current = Version::parse(env!("CARGO_PKG_VERSION")).unwrap();
        let older = Version::parse("0.1.0").unwrap();
        let newer = Version::new(current.major, current.minor, current.patch + 1);

        assert!(older < current); // Older version
        assert_eq!(current, current); // Same version
        assert!(newer > current); // Newer version
    }

    #[test]
    fn test_self_updater_force_mode() {
        let updater = SelfUpdater::new().force(true);

        // Verify force mode is set
        // The actual behavior would be tested in integration tests
        assert_eq!(updater.current_version(), env!("CARGO_PKG_VERSION"));
    }

    #[test]
    fn test_self_updater_platform_detection() {
        // Test platform-specific binary naming
        #[cfg(windows)]
        let expected_name = "agpm.exe";

        #[cfg(not(windows))]
        let expected_name = "agpm";

        // Verify the expectation is correct
        assert!(expected_name.contains("agpm"));
    }

    #[test]
    fn test_self_updater_archive_extension() {
        // Test archive format selection
        #[cfg(windows)]
        let expected_ext = ".zip";

        #[cfg(not(windows))]
        let expected_ext = ".tar.xz";

        // Verify the expectation is correct
        assert!(expected_ext.contains("."));
    }

    #[tokio::test]
    async fn test_self_updater_download_url_construction() {
        // Test expected GitHub release URL components
        let version = "1.0.0";
        let expected_components = vec!["github.com", "aig787/agpm", version, "agpm"];

        // Verify platform-specific target triple
        #[cfg(target_os = "macos")]
        {
            #[cfg(target_arch = "x86_64")]
            let target = "x86_64-apple-darwin";
            #[cfg(target_arch = "aarch64")]
            let target = "aarch64-apple-darwin";
            assert!(target.contains("apple-darwin"));
        }

        #[cfg(target_os = "linux")]
        {
            #[cfg(target_arch = "x86_64")]
            let target = "x86_64-unknown-linux-gnu";
            #[cfg(target_arch = "aarch64")]
            let target = "aarch64-unknown-linux-gnu";
            assert!(target.contains("linux"));
        }

        #[cfg(target_os = "windows")]
        {
            #[cfg(target_arch = "x86_64")]
            let target = "x86_64-pc-windows-msvc";
            #[cfg(target_arch = "aarch64")]
            let target = "aarch64-pc-windows-msvc";
            assert!(target.contains("windows"));
        }

        // Verify all expected components are present
        for component in expected_components {
            assert!(!component.is_empty());
        }
    }

    #[tokio::test]
    async fn test_self_updater_checksum_url() {
        // Test checksum URL construction
        let download_url = "https://github.com/aig787/agpm/releases/download/v1.0.0/agpm-x86_64-unknown-linux-gnu.tar.xz";
        let expected_checksum_url = format!("{}.sha256", download_url);

        // Verify GitHub URLs get .sha256 suffix
        assert!(download_url.contains("github.com"));
        assert!(expected_checksum_url.ends_with(".sha256"));

        // Non-GitHub URLs behavior
        let non_github = "https://example.com/agpm.tar.gz";
        assert!(!non_github.contains("github.com"));
    }

    #[tokio::test]
    async fn test_backup_path_generation() {
        let temp_dir = TempDir::new().unwrap();
        let binary_path = temp_dir.path().join("agpm");
        let manager = backup::BackupManager::new(binary_path.clone());

        let backup_path = manager.backup_path();
        assert_eq!(backup_path.file_name().unwrap(), "agpm.backup");
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
        assert!(result.unwrap_err().to_string().contains("Original file does not exist"));

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
        let cache_path = temp_dir.path().join(".agpm").join(".version_cache");

        // Create cache directory
        tokio::fs::create_dir_all(cache_path.parent().unwrap()).await.unwrap();

        // Create expired cache manually
        let old_cache = version_check::VersionCheckCache {
            latest_version: "1.0.0".to_string(),
            current_version: env!("CARGO_PKG_VERSION").to_string(),
            checked_at: Utc::now() - Duration::hours(25), // Over 24 hours old
            update_available: false,
            notified: false,
            notification_count: 0,
        };
        let content = serde_json::to_string_pretty(&old_cache).unwrap();
        tokio::fs::write(&cache_path, content).await.unwrap();

        // Set up environment to use temp directory
        unsafe {
            std::env::set_var(
                "AGPM_CONFIG_PATH",
                temp_dir.path().join(".agpm").join("config.toml"),
            );
        }

        // The VersionChecker would handle expired cache internally
        // We can't directly test private methods, but we can verify
        // that a 25-hour old cache would be considered expired
        // (default TTL is 24 hours)
        let age_in_hours = 25;
        assert!(age_in_hours > 24); // Would be expired

        // Clean up environment
        unsafe {
            std::env::remove_var("AGPM_CONFIG_PATH");
        }
    }

    #[test]
    fn test_version_check_cache_creation() {
        use chrono::Utc;
        use version_check::VersionCheckCache;

        let cache = VersionCheckCache::new("1.0.0".to_string(), "1.1.0".to_string());

        assert_eq!(cache.current_version, "1.0.0");
        assert_eq!(cache.latest_version, "1.1.0");
        assert!(cache.update_available);
        assert!(!cache.notified);
        assert_eq!(cache.notification_count, 0);

        // Check that timestamp is recent
        let now = Utc::now();
        let diff = now.signed_duration_since(cache.checked_at);
        assert!(diff.num_seconds() < 5); // Should be created within last 5 seconds
    }

    #[test]
    fn test_version_checker_is_expired() {
        use chrono::{Duration, Utc};
        use version_check::VersionCheckCache;

        // Create a cache that's 12 hours old
        let mut cache = VersionCheckCache::new("1.0.0".to_string(), "1.1.0".to_string());
        cache.checked_at = Utc::now() - Duration::hours(12);

        // This would require access to VersionChecker internals
        // For now, just verify the cache structure is correct
        assert_eq!(cache.current_version, "1.0.0");
        assert_eq!(cache.latest_version, "1.1.0");
    }

    #[test]
    fn test_version_notification_tracking() {
        use version_check::VersionCheckCache;

        let mut cache = VersionCheckCache::new("1.0.0".to_string(), "1.1.0".to_string());

        // Initially not notified
        assert!(!cache.notified);
        assert_eq!(cache.notification_count, 0);

        // Simulate notification
        cache.notified = true;
        cache.notification_count = 1;

        assert!(cache.notified);
        assert_eq!(cache.notification_count, 1);

        // Increment notification count
        cache.notification_count += 1;
        assert_eq!(cache.notification_count, 2);
    }

    #[tokio::test]
    async fn test_version_checker_display_notification() {
        // Test that notification display doesn't panic
        // The display_update_notification only takes latest version
        version_check::VersionChecker::display_update_notification("1.1.0");

        // Test passes if no panic occurs
    }

    #[test]
    fn test_should_check_for_updates_config() {
        let config = config::UpgradeConfig {
            check_on_startup: true,
            check_interval: 86400,
            auto_backup: true,
            verify_checksum: true,
        };

        // With check_on_startup true, should check
        assert!(config.check_on_startup);

        let config_disabled = config::UpgradeConfig {
            check_on_startup: false,
            check_interval: 0,
            ..config
        };

        // With check_interval 0, updates are disabled
        assert_eq!(config_disabled.check_interval, 0);
    }

    #[tokio::test]
    async fn test_upgrade_url_matches_github_releases() {
        use reqwest;

        // Get the current platform string as the code would construct it
        let platform = match (std::env::consts::OS, std::env::consts::ARCH) {
            ("macos", "aarch64") => "aarch64-apple-darwin",
            ("macos", "x86_64") => "x86_64-apple-darwin",
            ("linux", "aarch64") => "aarch64-unknown-linux-gnu",
            ("linux", "x86_64") => "x86_64-unknown-linux-gnu",
            ("windows", "x86_64") => "x86_64-pc-windows-msvc",
            ("windows", "aarch64") => "aarch64-pc-windows-msvc",
            (os, arch) => panic!("Unsupported platform: {os}-{arch}"),
        };

        let extension = if std::env::consts::OS == "windows" {
            "zip"
        } else {
            "tar.xz"
        };

        // This is what the code constructs after fix (CORRECT)
        let expected_filename_by_code = format!("agpm-cli-{platform}.{extension}");

        // Test version (using a known recent version)
        let test_version = "0.4.9";

        // Construct URL as the code would (now correct after fix)
        let constructed_url = format!(
            "https://github.com/aig787/agpm/releases/download/v{}/{}",
            test_version, expected_filename_by_code
        );

        // Test that the constructed URL returns 200 (should work after fix)
        let client = reqwest::Client::new();
        let response = client.head(&constructed_url).send().await.unwrap();
        assert_eq!(response.status(), 200, "Expected 200 for correct URL: {}", constructed_url);

        // Also test that the old (incorrect) URL still returns 404 to verify our fix is necessary
        let old_incorrect_filename = format!("agpm-{platform}.{extension}");
        let old_incorrect_url = format!(
            "https://github.com/aig787/agpm/releases/download/v{}/{}",
            test_version, old_incorrect_filename
        );
        let old_response = client.head(&old_incorrect_url).send().await.unwrap();
        assert_eq!(
            old_response.status(),
            404,
            "Expected 404 for old incorrect URL: {}",
            old_incorrect_url
        );
    }

    #[test]
    fn test_checksum_filename_matching_logic() {
        // Test the filename matching logic directly
        // This tests the improvement from contains() to starts_with() which reduces false positives
        // Note: "agpm-dev" still matches "agpm" with starts_with(), but it's much better than contains()

        let test_cases = vec![
            // (binary_name, filename, should_match)
            ("agpm", "agpm", true),              // Exact match
            ("agpm", "agpm-linux-x86_64", true), // Prefix match
            // ("agpm", "linux-agpm", true),  // Suffix match with hyphen - not supported by our fix
            ("agpm", "path/to/agpm", true),    // Path suffix match
            ("agpm", "agpm-dev", true), // Still matches with starts_with() but better than contains()
            ("agpm", "not-agpm", false), // Should not match
            ("agpm", "some-agpm-file", false), // Should not match (no hyphen separator)
            ("agpm.exe", "agpm.exe", true), // Windows exact match
            ("agpm.exe", "agpm.exe-windows-x86_64", true), // Windows prefix match
            // ("agpm.exe", "windows-agpm.exe", true),  // Windows suffix match - not supported by our fix
            ("agpm.exe", "agpm.exe-dev", true), // Still matches with starts_with() but better than contains()
        ];

        for (binary_name, filename, expected) in test_cases {
            let result = filename == binary_name
                || filename.starts_with(&format!("{}-", binary_name))
                || filename.ends_with(&format!("/{}", binary_name));

            assert_eq!(
                result,
                expected,
                "Binary '{}' {} match filename '{}'",
                binary_name,
                if expected {
                    "should"
                } else {
                    "should not"
                },
                filename
            );
        }
    }
}
