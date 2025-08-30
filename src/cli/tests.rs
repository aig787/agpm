//! Additional integration tests for CLI modules.
//!
//! This module provides comprehensive testing for the CLI interface, including
//! argument parsing, configuration building, and command execution paths.
//!
//! # Test Categories
//!
//! - **Argument Parsing**: Tests that CLI flags and options are parsed correctly
//! - **Configuration Building**: Tests that CLI arguments are converted to [`CliConfig`](crate::cli::CliConfig)
//! - **Command Execution**: Integration tests that verify commands execute successfully
//! - **Environment Handling**: Tests for environment variable application and restoration
//!
//! # Test Safety
//!
//! Tests that modify the working directory or environment variables use:
//! - Temporary directories for file operations
//! - Explicit environment variable restoration
//!
//! This ensures tests don't interfere with each other or the development environment.

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
            assert!(result.is_ok(), "Failed to parse: {cmd:?}");
        }
    }

    #[tokio::test]
    async fn test_cli_execute_with_flags() {
        use crate::cli::CliConfig;
        use tempfile::TempDir;

        // This test verifies that CLI commands execute successfully with various flags
        // We test using config injection to avoid modifying global environment variables

        let temp_dir = TempDir::new().unwrap();
        let temp_path = temp_dir.path().canonicalize().unwrap();

        // Create the manifest
        let manifest_path = temp_path.join("ccpm.toml");
        std::fs::write(&manifest_path, "[sources]\n").unwrap();

        // Verify the file exists
        assert!(manifest_path.exists(), "Manifest file was not created");

        // Test that verbose flag creates correct config and executes successfully
        // Use config path command which doesn't need a manifest in current dir
        let cli = Cli::try_parse_from(["ccpm", "--verbose", "config", "path"]).unwrap();
        assert!(cli.verbose);
        let config = cli.build_config();
        assert_eq!(config.log_level, Some("debug".to_string()));
        assert!(!config.no_progress);

        // Use a test config that doesn't modify environment
        let test_config = CliConfig::new(); // Empty config for testing
        let result = cli.execute_with_config(test_config).await;
        assert!(result.is_ok(), "Failed to execute with verbose flag");

        // Test that quiet flag creates correct config
        let cli = Cli::try_parse_from(["ccpm", "--quiet", "config", "path"]).unwrap();
        assert!(cli.quiet);
        let config = cli.build_config();
        assert_eq!(config.log_level, None);

        let test_config = CliConfig::new();
        let result = cli.execute_with_config(test_config).await;
        assert!(result.is_ok(), "Failed to execute with quiet flag");

        // Test that no-progress flag creates correct config
        let cli = Cli::try_parse_from(["ccpm", "--no-progress", "config", "path"]).unwrap();
        assert!(cli.no_progress);
        let config = cli.build_config();
        assert!(config.no_progress);
        assert_eq!(config.log_level, Some("info".to_string()));

        let test_config = CliConfig::new();
        let result = cli.execute_with_config(test_config).await;
        assert!(result.is_ok(), "Failed to execute with no-progress flag");

        // Test combined flags
        let cli =
            Cli::try_parse_from(["ccpm", "--verbose", "--no-progress", "config", "path"]).unwrap();
        assert!(cli.verbose);
        assert!(cli.no_progress);
        let config = cli.build_config();
        assert_eq!(config.log_level, Some("debug".to_string()));
        assert!(config.no_progress);

        let test_config = CliConfig::new();
        let result = cli.execute_with_config(test_config).await;
        assert!(result.is_ok(), "Failed to execute with combined flags");
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
        // Note: RUST_LOG is no longer set by apply_to_env, it's handled in main.rs
        let config = CliConfig {
            log_level: Some("debug".to_string()),
            no_progress: true,
            config_path: Some("/test/path".to_string()),
        };
        config.apply_to_env();

        // RUST_LOG should not be modified by apply_to_env anymore
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
        use tempfile::TempDir;

        // In coverage/CI environments, current dir might not exist, so set a safe one first

        let temp_dir = TempDir::new().unwrap();
        let temp_path = temp_dir.path().canonicalize().unwrap();

        // Create the manifest in temp directory
        let manifest_path = temp_path.join("ccpm.toml");
        std::fs::write(&manifest_path, "[sources]\n").unwrap();

        // Verify the file exists
        assert!(manifest_path.exists(), "Manifest file was not created");

        // Use a test config that doesn't modify environment for all commands
        let test_config = CliConfig::new();

        // Test each command execution path - use manifest-path parameter for all
        let cli = Cli::try_parse_from([
            "ccpm",
            "--manifest-path",
            manifest_path.to_str().unwrap(),
            "list",
        ])
        .unwrap();
        let result = cli.execute_with_config(test_config.clone()).await;
        assert!(result.is_ok(), "list command failed: {result:?}");

        // Verify the manifest still exists
        assert!(manifest_path.exists(), "Manifest disappeared after list");

        let cli = Cli::try_parse_from([
            "ccpm",
            "--manifest-path",
            manifest_path.to_str().unwrap(),
            "validate",
        ])
        .unwrap();
        let result = cli.execute_with_config(test_config.clone()).await;
        assert!(result.is_ok(), "validate command failed: {result:?}");

        let cli = Cli::try_parse_from([
            "ccpm",
            "--manifest-path",
            manifest_path.to_str().unwrap(),
            "cache",
            "info",
        ])
        .unwrap();
        let result = cli.execute_with_config(test_config.clone()).await;
        assert!(result.is_ok(), "cache info command failed: {result:?}");

        // Skip config commands that modify global state
        // These would create side effects that affect other tests

        // Test install
        let cli = Cli::try_parse_from([
            "ccpm",
            "--manifest-path",
            manifest_path.to_str().unwrap(),
            "install",
        ])
        .unwrap();
        let result = cli.execute_with_config(test_config.clone()).await;
        assert!(result.is_ok(), "install command failed: {result:?}");

        // Test update
        let cli = Cli::try_parse_from([
            "ccpm",
            "--manifest-path",
            manifest_path.to_str().unwrap(),
            "update",
        ])
        .unwrap();
        let result = cli.execute_with_config(test_config.clone()).await;
        assert!(result.is_ok(), "update command failed: {result:?}");

        // Test add source
        let cli = Cli::try_parse_from([
            "ccpm",
            "--manifest-path",
            manifest_path.to_str().unwrap(),
            "add",
            "source",
            "test",
            "https://github.com/test/repo.git",
        ])
        .unwrap();
        let result = cli.execute_with_config(test_config.clone()).await;
        assert!(result.is_ok(), "add source command failed: {result:?}");
    }

    #[tokio::test]
    async fn test_cli_execute_method() {
        // Test with config path command which doesn't require a manifest
        let cli = Cli::try_parse_from(["ccpm", "config", "path"]).unwrap();

        // This tests the execute method path (lines 582-584)
        let result = cli.execute().await;
        assert!(result.is_ok(), "execute method failed: {result:?}");
    }

    #[tokio::test]
    async fn test_cli_execute_init_command() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let temp_path = temp_dir.path().to_path_buf();

        // Test Init command execution (line 692)
        // Parse the CLI to properly create the command with path option
        let cli =
            Cli::try_parse_from(["ccpm", "init", "--path", temp_path.to_str().unwrap()]).unwrap();

        let result = cli.execute().await;
        assert!(result.is_ok(), "Init command failed: {result:?}");

        // Verify the manifest was created
        let manifest_path = temp_path.join("ccpm.toml");
        assert!(manifest_path.exists());
    }

    #[tokio::test]
    async fn test_cli_execute_cache_command() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let temp_path = temp_dir.path().canonicalize().unwrap();

        // Create a manifest for cache operations
        std::fs::write(temp_path.join("ccpm.toml"), "[sources]\n").unwrap();

        // Test Cache command execution (line 698)
        // Parse the CLI to properly create the command with default subcommand
        let cli = Cli::try_parse_from(["ccpm", "cache", "info"]).unwrap();

        let result = cli.execute().await;
        assert!(result.is_ok(), "Cache command failed: {result:?}");
    }

    #[tokio::test]
    async fn test_cli_execute_remove_command() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let temp_path = temp_dir.path().to_path_buf();

        // Change to temp directory

        // Create a manifest with a source to remove
        std::fs::write(
            temp_path.join("ccpm.toml"),
            r#"[sources]
test-source = "https://github.com/test/repo.git"

[agents]
[snippets]
[commands]
[mcp-servers]
"#,
        )
        .unwrap();

        // Test Remove command execution - try to remove non-existent source
        let cli = Cli::try_parse_from(["ccpm", "remove", "source", "nonexistent"]).unwrap();

        let result = cli.execute().await;

        // Should fail because source doesn't exist
        assert!(
            result.is_err(),
            "Remove command should fail for non-existent source"
        );
    }

    #[tokio::test]
    async fn test_cli_execute_config_command() {
        use tempfile::TempDir;

        let _temp_dir = TempDir::new().unwrap();

        // Test Config command execution (line 699)
        // Config path command doesn't need a manifest
        let cli = Cli::try_parse_from(["ccpm", "config", "path"]).unwrap();

        let result = cli.execute().await;
        assert!(result.is_ok(), "Config command failed: {result:?}");
    }

    #[test]
    fn test_cli_global_flags_work_with_all_commands() {
        let commands = vec!["init", "install", "update", "list", "validate"];
        let flags = vec!["--verbose", "--quiet", "--no-progress"];

        for cmd in &commands {
            for flag in &flags {
                let result = Cli::try_parse_from(["ccpm", flag, cmd]);
                assert!(result.is_ok(), "Failed with {flag} {cmd}");
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
