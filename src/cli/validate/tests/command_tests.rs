//! Tests for validate command

use super::super::{OutputFormat, ValidateCommand};
use crate::manifest::Manifest;
use tempfile::TempDir;

#[tokio::test]
async fn test_validate_custom_file_path() {
    let temp = TempDir::new().unwrap();

    // Create manifest in custom location
    let custom_dir = temp.path().join("custom");
    std::fs::create_dir_all(&custom_dir).unwrap();
    let manifest_path = custom_dir.join("custom.toml");

    let mut manifest = crate::manifest::Manifest::new();
    manifest.add_source("test".to_string(), "https://github.com/test/repo.git".to_string());
    manifest.save(&manifest_path).unwrap();

    let cmd = ValidateCommand {
        file: Some(manifest_path.to_str().unwrap().to_string()),
        resolve: false,
        check_lock: false,
        sources: false,
        paths: false,
        format: OutputFormat::Text,
        verbose: false,
        quiet: false,
        strict: false,
        render: false,
    };

    let result = cmd.execute_from_path(manifest_path).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_execute_with_no_manifest_json_format() {
    let temp = TempDir::new().unwrap();
    let manifest_path = temp.path().join("non_existent.toml");

    let cmd = ValidateCommand {
        file: Some(manifest_path.to_string_lossy().to_string()),
        resolve: false,
        check_lock: false,
        sources: false,
        paths: false,
        format: OutputFormat::Json, // Test JSON output for no manifest found
        verbose: false,
        quiet: false,
        strict: false,
        render: false,
    };

    let result = cmd.execute().await;
    assert!(result.is_err());
    // This tests lines 335-342 (JSON format for missing manifest)
}

#[tokio::test]
async fn test_execute_with_no_manifest_text_format() {
    let temp = TempDir::new().unwrap();
    let manifest_path = temp.path().join("non_existent.toml");

    let cmd = ValidateCommand {
        file: Some(manifest_path.to_string_lossy().to_string()),
        resolve: false,
        check_lock: false,
        sources: false,
        paths: false,
        format: OutputFormat::Text,
        verbose: false,
        quiet: false, // Not quiet - should print error message
        strict: false,
        render: false,
    };

    let result = cmd.execute().await;
    assert!(result.is_err());
    // This tests lines 343-344 (text format for missing manifest)
}

#[tokio::test]
async fn test_execute_with_no_manifest_quiet_mode() {
    let temp = TempDir::new().unwrap();
    let manifest_path = temp.path().join("non_existent.toml");

    let cmd = ValidateCommand {
        file: Some(manifest_path.to_string_lossy().to_string()),
        resolve: false,
        check_lock: false,
        sources: false,
        paths: false,
        format: OutputFormat::Text,
        verbose: false,
        quiet: true, // Quiet mode - should not print
        strict: false,
        render: false,
    };

    let result = cmd.execute().await;
    assert!(result.is_err());
    // This tests the else branch (quiet mode)
}

#[tokio::test]
async fn test_execute_from_path_nonexistent_file_json() {
    let temp = TempDir::new().unwrap();
    let nonexistent_path = temp.path().join("nonexistent.toml");

    let cmd = ValidateCommand {
        file: None,
        resolve: false,
        check_lock: false,
        sources: false,
        paths: false,
        format: OutputFormat::Json,
        verbose: false,
        quiet: false,
        strict: false,
        render: false,
    };

    let result = cmd.execute_from_path(nonexistent_path).await;
    assert!(result.is_err());
    // This tests lines 379-385 (JSON output for nonexistent manifest file)
}

#[tokio::test]
async fn test_execute_from_path_nonexistent_file_text() {
    let temp = TempDir::new().unwrap();
    let nonexistent_path = temp.path().join("nonexistent.toml");

    let cmd = ValidateCommand {
        file: None,
        resolve: false,
        check_lock: false,
        sources: false,
        paths: false,
        format: OutputFormat::Text,
        verbose: false,
        quiet: false,
        strict: false,
        render: false,
    };

    let result = cmd.execute_from_path(nonexistent_path).await;
    assert!(result.is_err());
    // This tests lines 386-387 (text output for nonexistent manifest file)
}

#[tokio::test]
async fn test_validate_command_defaults() {
    let cmd = ValidateCommand {
        file: None,
        resolve: false,
        check_lock: false,
        sources: false,
        paths: false,
        format: OutputFormat::Text,
        verbose: false,
        quiet: false,
        strict: false,
        render: false,
    };
    assert_eq!(cmd.file, None);
    assert!(!cmd.resolve);
    assert!(!cmd.check_lock);
    assert!(!cmd.sources);
    assert!(!cmd.paths);
    assert_eq!(cmd.format, OutputFormat::Text);
    assert!(!cmd.verbose);
    assert!(!cmd.quiet);
    assert!(!cmd.strict);
}

#[tokio::test]
async fn test_execute_without_manifest_file() {
    // Test when no manifest file exists - use temp directory with specific non-existent file
    let temp = TempDir::new().unwrap();
    let non_existent_manifest = temp.path().join("non_existent.toml");

    let cmd = ValidateCommand {
        file: Some(non_existent_manifest.to_string_lossy().to_string()),
        resolve: false,
        check_lock: false,
        sources: false,
        paths: false,
        format: OutputFormat::Text,
        verbose: false,
        quiet: false,
        strict: false,
        render: false,
    };

    let result = cmd.execute().await;
    assert!(result.is_err()); // Should fail when no manifest found
}

#[tokio::test]
async fn test_execute_with_specified_file() {
    let temp = TempDir::new().unwrap();
    let custom_path = temp.path().join("custom.toml");

    let manifest = Manifest::new();
    manifest.save(&custom_path).unwrap();

    let cmd = ValidateCommand {
        file: Some(custom_path.to_string_lossy().to_string()),
        resolve: false,
        check_lock: false,
        sources: false,
        paths: false,
        format: OutputFormat::Text,
        verbose: false,
        quiet: false,
        strict: false,
        render: false,
    };

    let result = cmd.execute().await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_execute_with_nonexistent_specified_file() {
    let temp = TempDir::new().unwrap();
    let nonexistent = temp.path().join("nonexistent.toml");

    let cmd = ValidateCommand {
        file: Some(nonexistent.to_string_lossy().to_string()),
        resolve: false,
        check_lock: false,
        sources: false,
        paths: false,
        format: OutputFormat::Text,
        verbose: false,
        quiet: false,
        strict: false,
        render: false,
    };

    let result = cmd.execute().await;
    assert!(result.is_err());
}
