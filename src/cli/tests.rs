// Additional tests for CLI modules

#[cfg(test)]
mod cli_tests {
    use crate::cli::Cli;
    use clap::Parser;

    #[test]
    fn test_cli_parsing() {
        // Test basic command parsing
        let cli = Cli::try_parse_from(["ccpm", "--help"]);
        assert!(cli.is_err()); // --help causes a special error

        let cli = Cli::try_parse_from(["ccpm", "list"]);
        assert!(cli.is_ok());
    }

    #[test]
    fn test_cli_verbose_flag() {
        let cli = Cli::try_parse_from(["ccpm", "--verbose", "list"]);
        assert!(cli.is_ok());
        let cli = cli.unwrap();
        assert!(cli.verbose);
    }

    #[test]
    fn test_cli_quiet_flag() {
        let cli = Cli::try_parse_from(["ccpm", "--quiet", "list"]);
        assert!(cli.is_ok());
        let cli = cli.unwrap();
        assert!(cli.quiet);
    }

    #[test]
    fn test_cli_no_progress_flag() {
        let cli = Cli::try_parse_from(["ccpm", "--no-progress", "list"]);
        assert!(cli.is_ok());
        let cli = cli.unwrap();
        assert!(cli.no_progress);
    }

    #[test]
    fn test_cli_config_option() {
        let cli = Cli::try_parse_from(["ccpm", "--config", "/path/to/config", "list"]);
        assert!(cli.is_ok());
        let cli = cli.unwrap();
        assert_eq!(cli.config, Some("/path/to/config".to_string()));
    }

    #[test]
    fn test_cli_all_commands() {
        // Test that all commands can be parsed
        let commands = vec![
            vec!["ccpm", "init"],
            vec![
                "ccpm",
                "add",
                "source",
                "test",
                "https://github.com/test/repo.git",
            ],
            vec!["ccpm", "install"],
            vec!["ccpm", "update"],
            vec!["ccpm", "list"],
            vec!["ccpm", "validate"],
            vec!["ccpm", "cache", "info"],
            vec!["ccpm", "config", "show"],
        ];

        for cmd in commands {
            let result = Cli::try_parse_from(cmd.clone());
            assert!(result.is_ok(), "Failed to parse: {:?}", cmd);
        }
    }

    #[tokio::test]
    async fn test_cli_execute_with_flags() {
        use crate::cli::CliConfig;
        use crate::test_utils::WorkingDirGuard;
        use tempfile::TempDir;

        // This test verifies that CLI commands execute successfully with various flags
        // We test using config injection to avoid modifying global environment variables

        // Use WorkingDirGuard to serialize tests that change working directory
        let _guard = WorkingDirGuard::new().unwrap();

        let temp_dir = TempDir::new().unwrap();
        let manifest_path = temp_dir.path().join("ccpm.toml");
        std::fs::write(&manifest_path, "[sources]\n").unwrap();

        // Change to temp dir for the test
        std::env::set_current_dir(temp_dir.path()).unwrap();

        // Test that verbose flag creates correct config and executes successfully
        let cli = Cli::try_parse_from(["ccpm", "--verbose", "list"]).unwrap();
        assert!(cli.verbose);
        let config = cli.build_config();
        assert_eq!(config.log_level, Some("debug".to_string()));
        assert!(!config.no_progress);

        // Use a test config that doesn't modify environment
        let test_config = CliConfig::new(); // Empty config for testing
        let result = cli.execute_with_config(test_config).await;
        assert!(result.is_ok(), "Failed to execute with verbose flag");

        // Test that quiet flag creates correct config
        let cli = Cli::try_parse_from(["ccpm", "--quiet", "list"]).unwrap();
        assert!(cli.quiet);
        let config = cli.build_config();
        assert_eq!(config.log_level, None);

        let test_config = CliConfig::new();
        let result = cli.execute_with_config(test_config).await;
        assert!(result.is_ok(), "Failed to execute with quiet flag");

        // Test that no-progress flag creates correct config
        let cli = Cli::try_parse_from(["ccpm", "--no-progress", "list"]).unwrap();
        assert!(cli.no_progress);
        let config = cli.build_config();
        assert!(config.no_progress);
        assert_eq!(config.log_level, Some("info".to_string()));

        let test_config = CliConfig::new();
        let result = cli.execute_with_config(test_config).await;
        assert!(result.is_ok(), "Failed to execute with no-progress flag");

        // Test combined flags
        let cli = Cli::try_parse_from(["ccpm", "--verbose", "--no-progress", "list"]).unwrap();
        assert!(cli.verbose);
        assert!(cli.no_progress);
        let config = cli.build_config();
        assert_eq!(config.log_level, Some("debug".to_string()));
        assert!(config.no_progress);

        let test_config = CliConfig::new();
        let result = cli.execute_with_config(test_config).await;
        assert!(result.is_ok(), "Failed to execute with combined flags");

        // WorkingDirGuard will restore the original directory when dropped
    }

    #[test]
    fn test_cli_config_builder() {
        // Test verbose flag sets debug log level
        let cli = Cli::try_parse_from(["ccpm", "--verbose", "list"]).unwrap();
        let config = cli.build_config();
        assert_eq!(config.log_level, Some("debug".to_string()));
        assert!(!config.no_progress);

        // Test quiet flag sets no log level
        let cli = Cli::try_parse_from(["ccpm", "--quiet", "list"]).unwrap();
        let config = cli.build_config();
        assert_eq!(config.log_level, None);

        // Test default sets info log level
        let cli = Cli::try_parse_from(["ccpm", "list"]).unwrap();
        let config = cli.build_config();
        assert_eq!(config.log_level, Some("info".to_string()));

        // Test no-progress flag
        let cli = Cli::try_parse_from(["ccpm", "--no-progress", "list"]).unwrap();
        let config = cli.build_config();
        assert!(config.no_progress);

        // Test config path
        let cli = Cli::try_parse_from(["ccpm", "--config", "/custom/path", "list"]).unwrap();
        let config = cli.build_config();
        assert_eq!(config.config_path, Some("/custom/path".to_string()));
    }

    // NOTE: This test explicitly tests CliConfig's apply_to_env functionality
    // It uses std::env::set_var to test that the config correctly sets environment variables
    // If this test becomes flaky, run with: cargo test -- --test-threads=1
    #[test]
    fn test_cli_config_apply_to_env() {
        use crate::cli::CliConfig;

        // Save original env vars
        let orig_rust_log = std::env::var("RUST_LOG").ok();
        let orig_no_progress = std::env::var("CCPM_NO_PROGRESS").ok();
        let orig_config = std::env::var("CCPM_CONFIG").ok();

        // Test applying config with all values
        let config = CliConfig {
            log_level: Some("debug".to_string()),
            no_progress: true,
            config_path: Some("/test/path".to_string()),
        };
        config.apply_to_env();

        assert_eq!(std::env::var("RUST_LOG").unwrap(), "debug");
        assert_eq!(std::env::var("CCPM_NO_PROGRESS").unwrap(), "1");
        assert_eq!(std::env::var("CCPM_CONFIG").unwrap(), "/test/path");

        // Test applying config with no values doesn't crash
        let config = CliConfig::new();
        config.apply_to_env(); // Should not panic

        // Restore original env vars
        match orig_rust_log {
            Some(val) => std::env::set_var("RUST_LOG", val),
            None => std::env::remove_var("RUST_LOG"),
        }
        match orig_no_progress {
            Some(val) => std::env::set_var("CCPM_NO_PROGRESS", val),
            None => std::env::remove_var("CCPM_NO_PROGRESS"),
        }
        match orig_config {
            Some(val) => std::env::set_var("CCPM_CONFIG", val),
            None => std::env::remove_var("CCPM_CONFIG"),
        }
    }

    #[tokio::test]
    async fn test_cli_execute_all_commands() {
        use crate::cli::CliConfig;
        use crate::test_utils::WorkingDirGuard;
        use tempfile::TempDir;

        // Use WorkingDirGuard to serialize tests that change working directory
        let _guard = WorkingDirGuard::new().unwrap();

        let temp_dir = TempDir::new().unwrap();
        let manifest_path = temp_dir.path().join("ccpm.toml");
        std::fs::write(&manifest_path, "[sources]\n").unwrap();

        // Change to temp dir for the test
        std::env::set_current_dir(temp_dir.path()).unwrap();

        // Use a test config that doesn't modify environment for all commands
        let test_config = CliConfig::new();

        // Test each command execution path
        let cli = Cli::try_parse_from(["ccpm", "list"]).unwrap();
        let result = cli.execute_with_config(test_config.clone()).await;
        assert!(result.is_ok(), "list command failed: {:?}", result);

        let cli = Cli::try_parse_from(["ccpm", "validate"]).unwrap();
        let result = cli.execute_with_config(test_config.clone()).await;
        assert!(result.is_ok(), "validate command failed: {:?}", result);

        let cli = Cli::try_parse_from(["ccpm", "cache", "info"]).unwrap();
        let result = cli.execute_with_config(test_config.clone()).await;
        assert!(result.is_ok(), "cache info command failed: {:?}", result);

        // Skip config commands that modify global state
        // These would create side effects that affect other tests

        // Test install
        let cli = Cli::try_parse_from(["ccpm", "install"]).unwrap();
        let result = cli.execute_with_config(test_config.clone()).await;
        assert!(result.is_ok(), "install command failed: {:?}", result);

        // Test update
        let cli = Cli::try_parse_from(["ccpm", "update"]).unwrap();
        let result = cli.execute_with_config(test_config.clone()).await;
        assert!(result.is_ok(), "update command failed: {:?}", result);

        // Test add source
        let cli = Cli::try_parse_from([
            "ccpm",
            "add",
            "source",
            "test",
            "https://github.com/test/repo.git",
        ])
        .unwrap();
        let result = cli.execute_with_config(test_config.clone()).await;
        assert!(result.is_ok(), "add source command failed: {:?}", result);

        // WorkingDirGuard will restore the original directory when dropped
    }

    #[test]
    fn test_cli_global_flags_work_with_all_commands() {
        let commands = vec!["init", "install", "update", "list", "validate"];
        let flags = vec!["--verbose", "--quiet", "--no-progress"];

        for cmd in &commands {
            for flag in &flags {
                let result = Cli::try_parse_from(["ccpm", flag, cmd]);
                assert!(result.is_ok(), "Failed with {} {}", flag, cmd);
            }
        }
    }

    // Individual command parsing tests removed - commands don't directly implement Parser trait
    // They are tested through the main CLI interface and integration tests
}

// Update command tests removed - fields are private

#[cfg(test)]
mod cli_execution_tests {
    // List command test removed - fields are private

    // Info command test removed - fields are private

    // Validate and Init command tests removed - fields are private and don't match struct definitions
}
