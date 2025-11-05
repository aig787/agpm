//! Tests for validate command

use super::super::{OutputFormat, ValidateCommand};
use crate::manifest::{Manifest, ResourceDependency};
use crate::utils::normalize_path_for_storage;
use tempfile::TempDir;

#[tokio::test]
async fn test_validate_json_format() {
    let temp = TempDir::new().unwrap();
    let manifest_path = temp.path().join("agpm.toml");

    // Create valid manifest
    let mut manifest = crate::manifest::Manifest::new();
    manifest.add_source("test".to_string(), "https://github.com/test/repo.git".to_string());
    manifest.save(&manifest_path).unwrap();

    let cmd = ValidateCommand {
        file: None,
        resolve: false,
        check_lock: false,
        sources: false,
        paths: false,
        format: OutputFormat::Json,
        verbose: false,
        quiet: true,
        strict: false,
        render: false,
    };

    let result = cmd.execute_from_path(manifest_path).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_validate_strict_mode() {
    let temp = TempDir::new().unwrap();
    let manifest_path = temp.path().join("agpm.toml");

    // Create manifest with warning (empty sources)
    let manifest = crate::manifest::Manifest::new();
    manifest.save(&manifest_path).unwrap();

    let cmd = ValidateCommand {
        file: None,
        resolve: false,
        check_lock: false,
        sources: false,
        paths: false,
        format: OutputFormat::Text,
        verbose: false,
        quiet: true,
        strict: true, // Strict mode treats warnings as errors
        render: false,
    };

    let result = cmd.execute_from_path(manifest_path).await;
    // Should fail in strict mode due to warnings
    assert!(result.is_err());
}

#[tokio::test]
async fn test_validate_verbose_mode() {
    let temp = TempDir::new().unwrap();
    let manifest_path = temp.path().join("agpm.toml");

    // Create valid manifest
    let mut manifest = crate::manifest::Manifest::new();
    manifest.add_source("test".to_string(), "https://github.com/test/repo.git".to_string());
    manifest.save(&manifest_path).unwrap();

    let cmd = ValidateCommand {
        file: None,
        resolve: false,
        check_lock: false,
        sources: false,
        paths: false,
        format: OutputFormat::Text,
        verbose: true, // Enable verbose output
        quiet: false,
        strict: false,
        render: false,
    };

    let result = cmd.execute_from_path(manifest_path).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_validate_json_error_format() {
    let temp = TempDir::new().unwrap();
    let manifest_path = temp.path().join("agpm.toml");

    // Create invalid manifest
    let mut manifest = crate::manifest::Manifest::new();
    manifest.add_dependency(
        "test".to_string(),
        crate::manifest::ResourceDependency::Detailed(Box::new(
            crate::manifest::DetailedDependency {
                source: Some("nonexistent".to_string()),
                path: "test.md".to_string(),
                version: None,
                command: None,
                branch: None,
                rev: None,
                args: None,
                target: None,
                filename: None,
                dependencies: None,
                tool: Some("claude-code".to_string()),
                flatten: None,
                install: None,

                template_vars: Some(serde_json::Value::Object(serde_json::Map::new())),
            },
        )),
        true,
    );
    manifest.save(&manifest_path).unwrap();

    let cmd = ValidateCommand {
        file: None,
        resolve: false,
        check_lock: false,
        sources: false,
        paths: false,
        format: OutputFormat::Json, // JSON format for errors
        verbose: false,
        quiet: true,
        strict: false,
        render: false,
    };

    let result = cmd.execute_from_path(manifest_path).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_validate_verbose_output() {
    let temp = TempDir::new().unwrap();
    let manifest_path = temp.path().join("agpm.toml");

    let manifest = crate::manifest::Manifest::new();
    manifest.save(&manifest_path).unwrap();

    let cmd = ValidateCommand {
        file: None,
        resolve: false,
        check_lock: false,
        sources: false,
        paths: false,
        format: OutputFormat::Text,
        verbose: true,
        quiet: false,
        strict: false,
        render: false,
    };

    let result = cmd.execute_from_path(manifest_path).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_validate_strict_mode_with_warnings() {
    let temp = TempDir::new().unwrap();
    let manifest_path = temp.path().join("agpm.toml");

    // Create manifest that will have warnings
    let manifest = crate::manifest::Manifest::new();
    manifest.save(&manifest_path).unwrap();

    // Without lockfile, should have warning
    let cmd = ValidateCommand {
        file: None,
        resolve: false,
        check_lock: true,
        sources: false,
        paths: false,
        format: OutputFormat::Text,
        verbose: false,
        quiet: false,
        strict: true, // Strict mode
        render: false,
    };

    let result = cmd.execute_from_path(manifest_path).await;
    assert!(result.is_err()); // Should fail in strict mode with warnings
}

#[tokio::test]
async fn test_validate_quiet_mode() {
    let temp = TempDir::new().unwrap();
    let manifest_path = temp.path().join("agpm.toml");

    // Create valid manifest
    let manifest = crate::manifest::Manifest::new();
    manifest.save(&manifest_path).unwrap();

    let cmd = ValidateCommand {
        file: None,
        resolve: false,
        check_lock: false,
        sources: false,
        paths: false,
        format: OutputFormat::Text,
        verbose: false,
        quiet: true, // Enable quiet
        strict: false,
        render: false,
    };

    let result = cmd.execute_from_path(manifest_path).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_validate_json_output_success() {
    let temp = TempDir::new().unwrap();
    let manifest_path = temp.path().join("agpm.toml");

    // Create valid manifest with dependencies
    let mut manifest = crate::manifest::Manifest::new();
    use crate::manifest::{DetailedDependency, ResourceDependency};

    manifest.agents.insert(
        "test".to_string(),
        ResourceDependency::Detailed(Box::new(DetailedDependency {
            source: None,
            path: "test.md".to_string(),
            version: None,
            command: None,
            branch: None,
            rev: None,
            args: None,
            target: None,
            filename: None,
            dependencies: None,
            tool: Some("claude-code".to_string()),
            flatten: None,
            install: None,

            template_vars: Some(serde_json::Value::Object(serde_json::Map::new())),
        })),
    );
    manifest.save(&manifest_path).unwrap();

    let cmd = ValidateCommand {
        file: None,
        resolve: false,
        check_lock: false,
        sources: false,
        paths: false,
        format: OutputFormat::Json, // JSON output
        verbose: false,
        quiet: false,
        strict: false,
        render: false,
    };

    let result = cmd.execute_from_path(manifest_path).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_validate_manifest_toml_syntax_error_json() {
    let temp = TempDir::new().unwrap();
    let manifest_path = temp.path().join("agpm.toml");

    // Create invalid TOML file
    std::fs::write(&manifest_path, "invalid toml syntax [[[").unwrap();

    let cmd = ValidateCommand {
        file: None,
        resolve: false,
        check_lock: false,
        sources: false,
        paths: false,
        format: OutputFormat::Json,
        verbose: false,
        quiet: true,
        strict: false,
        render: false,
    };

    let result = cmd.execute_from_path(manifest_path).await;
    assert!(result.is_err());
    // This tests lines 422-426 (JSON output for TOML syntax error)
}

#[tokio::test]
async fn test_validate_resolve_with_error_json_output() {
    let temp = TempDir::new().unwrap();
    let manifest_path = temp.path().join("agpm.toml");

    // Create manifest with dependency that will fail to resolve
    let mut manifest = crate::manifest::Manifest::new();
    manifest.add_source("test".to_string(), "https://github.com/nonexistent/repo.git".to_string());
    manifest.add_dependency(
        "failing-agent".to_string(),
        crate::manifest::ResourceDependency::Detailed(Box::new(
            crate::manifest::DetailedDependency {
                source: Some("test".to_string()),
                path: "test.md".to_string(),
                version: None,
                command: None,
                branch: None,
                rev: None,
                args: None,
                target: None,
                filename: None,
                dependencies: None,
                tool: Some("claude-code".to_string()),
                flatten: None,
                install: None,

                template_vars: Some(serde_json::Value::Object(serde_json::Map::new())),
            },
        )),
        true,
    );
    manifest.save(&manifest_path).unwrap();

    let cmd = ValidateCommand {
        file: None,
        resolve: true,
        check_lock: false,
        sources: false,
        paths: false,
        format: OutputFormat::Json,
        verbose: false,
        quiet: true,
        strict: false,
        render: false,
    };

    let result = cmd.execute_from_path(manifest_path).await;
    // This will likely fail due to network issues or nonexistent repo
    // This tests lines 515-520 and 549-554 (JSON output for resolve errors)
    let _ = result; // Don't assert success/failure as it depends on network
}

#[tokio::test]
async fn test_validate_sources_accessibility_error_json() {
    let temp = TempDir::new().unwrap();
    let manifest_path = temp.path().join("agpm.toml");

    // Create manifest with sources that will fail accessibility check
    // Use file:// URLs pointing to non-existent local paths
    let nonexistent_path1 = temp.path().join("nonexistent1");
    let nonexistent_path2 = temp.path().join("nonexistent2");

    // Convert to file:// URLs with proper formatting for Windows
    let url1 = format!("file://{}", normalize_path_for_storage(&nonexistent_path1));
    let url2 = format!("file://{}", normalize_path_for_storage(&nonexistent_path2));

    let mut manifest = crate::manifest::Manifest::new();
    manifest.add_source("official".to_string(), url1);
    manifest.add_source("community".to_string(), url2);
    manifest.save(&manifest_path).unwrap();

    let cmd = ValidateCommand {
        file: None,
        resolve: false,
        check_lock: false,
        sources: true,
        paths: false,
        format: OutputFormat::Json,
        verbose: false,
        quiet: true,
        strict: false,
        render: false,
    };

    let result = cmd.execute_from_path(manifest_path).await;
    // This tests lines 586-590, 621-625 (JSON source accessibility error)
    let _ = result;
}

#[tokio::test]
async fn test_validate_check_paths_missing_snippets_json() {
    let temp = TempDir::new().unwrap();
    let manifest_path = temp.path().join("agpm.toml");

    // Create manifest with missing local snippet
    let mut manifest = crate::manifest::Manifest::new();
    manifest.snippets.insert(
        "missing-snippet".to_string(),
        crate::manifest::ResourceDependency::Detailed(Box::new(
            crate::manifest::DetailedDependency {
                source: None,
                path: "./missing/snippet.md".to_string(),
                version: None,
                command: None,
                branch: None,
                rev: None,
                args: None,
                target: None,
                filename: None,
                dependencies: None,
                tool: Some("claude-code".to_string()),
                flatten: None,
                install: None,

                template_vars: Some(serde_json::Value::Object(serde_json::Map::new())),
            },
        )),
    );
    manifest.save(&manifest_path).unwrap();

    let cmd = ValidateCommand {
        file: None,
        resolve: false,
        check_lock: false,
        sources: false,
        paths: true,
        format: OutputFormat::Json, // Test JSON output for missing paths
        verbose: false,
        quiet: true,
        strict: false,
        render: false,
    };

    let result = cmd.execute_from_path(manifest_path).await;
    assert!(result.is_err());
    // This tests lines 734-738 (JSON output for missing local paths)
}

#[tokio::test]
async fn test_validate_lockfile_syntax_error_json() {
    let temp = TempDir::new().unwrap();
    let manifest_path = temp.path().join("agpm.toml");
    let lockfile_path = temp.path().join("agpm.lock");

    // Create valid manifest
    let manifest = crate::manifest::Manifest::new();
    manifest.save(&manifest_path).unwrap();

    // Create invalid lockfile
    std::fs::write(&lockfile_path, "invalid toml [[[").unwrap();

    let cmd = ValidateCommand {
        file: None,
        resolve: false,
        check_lock: true,
        sources: false,
        paths: false,
        format: OutputFormat::Json,
        verbose: false,
        quiet: true,
        strict: false,
        render: false,
    };

    let result = cmd.execute_from_path(manifest_path).await;
    assert!(result.is_err());
    // This tests lines 829-834 (JSON output for invalid lockfile syntax)
}

#[tokio::test]
async fn test_validate_strict_mode_with_json_output() {
    let temp = TempDir::new().unwrap();
    let manifest_path = temp.path().join("agpm.toml");

    // Create manifest that will generate warnings
    let manifest = crate::manifest::Manifest::new(); // Empty manifest generates "no dependencies" warning
    manifest.save(&manifest_path).unwrap();

    let cmd = ValidateCommand {
        file: None,
        resolve: false,
        check_lock: false,
        sources: false,
        paths: false,
        format: OutputFormat::Json,
        verbose: false,
        quiet: true,
        strict: true, // Strict mode with JSON output
        render: false,
    };

    let result = cmd.execute_from_path(manifest_path).await;
    assert!(result.is_err()); // Strict mode treats warnings as errors
    // This tests lines 849-852 (strict mode with JSON output)
}

#[tokio::test]
async fn test_validate_strict_mode_text_output() {
    let temp = TempDir::new().unwrap();
    let manifest_path = temp.path().join("agpm.toml");

    // Create manifest that will generate warnings
    let manifest = crate::manifest::Manifest::new();
    manifest.save(&manifest_path).unwrap();

    let cmd = ValidateCommand {
        file: None,
        resolve: false,
        check_lock: false,
        sources: false,
        paths: false,
        format: OutputFormat::Text,
        verbose: false,
        quiet: false, // Not quiet - should print error message
        strict: true,
        render: false,
    };

    let result = cmd.execute_from_path(manifest_path).await;
    assert!(result.is_err());
    // This tests lines 854-855 (strict mode with text output)
}

#[tokio::test]
async fn test_validate_verbose_mode_with_summary() {
    let temp = TempDir::new().unwrap();
    let manifest_path = temp.path().join("agpm.toml");

    // Create manifest with some content for summary
    let mut manifest = crate::manifest::Manifest::new();
    manifest.add_source("test".to_string(), "https://github.com/test/repo.git".to_string());
    manifest.add_dependency(
        "test-agent".to_string(),
        crate::manifest::ResourceDependency::Simple("test.md".to_string()),
        true,
    );
    manifest.add_dependency(
        "test-snippet".to_string(),
        crate::manifest::ResourceDependency::Simple("snippet.md".to_string()),
        false,
    );
    manifest.save(&manifest_path).unwrap();

    let cmd = ValidateCommand {
        file: None,
        resolve: false,
        check_lock: false,
        sources: false,
        paths: false,
        format: OutputFormat::Text,
        verbose: true, // Verbose mode to show summary
        quiet: false,
        strict: false,
        render: false,
    };

    let result = cmd.execute_from_path(manifest_path).await;
    assert!(result.is_ok());
    // This tests lines 484-490 (verbose mode summary output)
}

#[tokio::test]
async fn test_output_format_equality() {
    // Test PartialEq implementation
    assert_eq!(OutputFormat::Text, OutputFormat::Text);
    assert_eq!(OutputFormat::Json, OutputFormat::Json);
    assert_ne!(OutputFormat::Text, OutputFormat::Json);
}

#[tokio::test]
async fn test_json_output_format() {
    let temp = TempDir::new().unwrap();
    let manifest_path = temp.path().join("agpm.toml");

    let manifest = Manifest::new();
    manifest.save(&manifest_path).unwrap();

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

    let result = cmd.execute_from_path(manifest_path).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_validation_with_verbose_mode() {
    let temp = TempDir::new().unwrap();
    let manifest_path = temp.path().join("agpm.toml");

    let manifest = Manifest::new();
    manifest.save(&manifest_path).unwrap();

    let cmd = ValidateCommand {
        file: None,
        resolve: false,
        check_lock: false,
        sources: false,
        paths: false,
        format: OutputFormat::Text,
        verbose: true,
        quiet: false,
        strict: false,
        render: false,
    };

    let result = cmd.execute_from_path(manifest_path).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_validation_with_quiet_mode() {
    let temp = TempDir::new().unwrap();
    let manifest_path = temp.path().join("agpm.toml");

    let manifest = Manifest::new();
    manifest.save(&manifest_path).unwrap();

    let cmd = ValidateCommand {
        file: None,
        resolve: false,
        check_lock: false,
        sources: false,
        paths: false,
        format: OutputFormat::Text,
        verbose: false,
        quiet: true,
        strict: false,
        render: false,
    };

    let result = cmd.execute_from_path(manifest_path).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_validation_with_strict_mode_and_warnings() {
    let temp = TempDir::new().unwrap();
    let manifest_path = temp.path().join("agpm.toml");

    // Create empty manifest to trigger warning
    let manifest = Manifest::new();
    manifest.save(&manifest_path).unwrap();

    let cmd = ValidateCommand {
        file: None,
        resolve: false,
        check_lock: false,
        sources: false,
        paths: false,
        format: OutputFormat::Text,
        verbose: false,
        quiet: false,
        strict: true, // Strict mode will fail on warnings
        render: false,
    };

    let result = cmd.execute_from_path(manifest_path).await;
    assert!(result.is_err()); // Should fail due to warning in strict mode
}

#[tokio::test]
async fn test_validation_json_output_with_errors() {
    let temp = TempDir::new().unwrap();
    let manifest_path = temp.path().join("agpm.toml");

    // Write invalid TOML
    std::fs::write(&manifest_path, "invalid toml [[[ syntax").unwrap();

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

    let result = cmd.execute_from_path(manifest_path).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_validation_with_manifest_not_found_json() {
    let temp = TempDir::new().unwrap();
    let manifest_path = temp.path().join("nonexistent.toml");

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

    let result = cmd.execute_from_path(manifest_path).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_validation_with_manifest_not_found_text() {
    let temp = TempDir::new().unwrap();
    let manifest_path = temp.path().join("nonexistent.toml");

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

    let result = cmd.execute_from_path(manifest_path).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_validation_with_verbose_and_text_format() {
    let temp = TempDir::new().unwrap();
    let manifest_path = temp.path().join("agpm.toml");

    let mut manifest = Manifest::new();
    manifest.sources.insert("test".to_string(), "https://github.com/test/repo.git".to_string());
    manifest
        .agents
        .insert("agent1".to_string(), ResourceDependency::Simple("agent.md".to_string()));
    manifest
        .snippets
        .insert("snippet1".to_string(), ResourceDependency::Simple("snippet.md".to_string()));
    manifest.save(&manifest_path).unwrap();

    let cmd = ValidateCommand {
        file: None,
        resolve: false,
        check_lock: false,
        sources: false,
        paths: false,
        format: OutputFormat::Text,
        verbose: true,
        quiet: false,
        strict: false,
        render: false,
    };

    let result = cmd.execute_from_path(manifest_path).await;
    assert!(result.is_ok());
}
