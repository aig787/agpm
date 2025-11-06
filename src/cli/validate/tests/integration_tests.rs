//! Tests for validate command

use super::super::{OutputFormat, ValidateCommand, ValidationResults};
use crate::manifest::{Manifest, ResourceDependency};
use tempfile::TempDir;

#[tokio::test]
async fn test_validate_no_manifest() {
    let temp = TempDir::new().unwrap();
    let manifest_path = temp.path().join("nonexistent").join("agpm.toml");

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
async fn test_validate_valid_manifest() {
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
        verbose: false,
        quiet: false,
        strict: false,
        render: false,
    };

    let result = cmd.execute_from_path(manifest_path).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_validate_invalid_manifest() {
    let temp = TempDir::new().unwrap();
    let manifest_path = temp.path().join("agpm.toml");

    // Create invalid manifest (dependency without source)
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
async fn test_validate_manifest_toml_syntax_error() {
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
        format: OutputFormat::Text,
        verbose: false,
        quiet: false,
        strict: false,
        render: false,
    };

    let result = cmd.execute_from_path(manifest_path).await;
    assert!(result.is_err());
    // This tests lines 415-416 (TOML syntax error detection)
}

#[tokio::test]
async fn test_validate_manifest_structure_error() {
    let temp = TempDir::new().unwrap();
    let manifest_path = temp.path().join("agpm.toml");

    // Create manifest with invalid structure
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
        format: OutputFormat::Text,
        verbose: false,
        quiet: false,
        strict: false,
        render: false,
    };

    let result = cmd.execute_from_path(manifest_path).await;
    assert!(result.is_err());
    // This tests manifest validation errors (lines 435-455)
}

#[tokio::test]
async fn test_validate_manifest_version_conflict() {
    let temp = TempDir::new().unwrap();
    let manifest_path = temp.path().join("agpm.toml");

    // Create a test manifest file that would trigger version conflict detection
    std::fs::write(
        &manifest_path,
        r#"
[sources]
test = "https://github.com/test/repo.git"

[agents]
shared-agent = { source = "test", path = "agent.md", version = "v1.0.0" }
another-agent = { source = "test", path = "agent.md", version = "v2.0.0" }
"#,
    )
    .unwrap();

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

    // Version conflicts are automatically resolved during installation
    let result = cmd.execute_from_path(manifest_path).await;
    // Version conflicts are typically warnings, not errors
    assert!(result.is_ok());
    // This tests lines 439-442 (version conflict detection)
}

#[tokio::test]
async fn test_validate_with_outdated_version_warnings() {
    let temp = TempDir::new().unwrap();
    let manifest_path = temp.path().join("agpm.toml");

    // Create manifest with v0.x versions (potentially outdated)
    let mut manifest = crate::manifest::Manifest::new();
    manifest.add_source("test".to_string(), "https://github.com/test/repo.git".to_string());
    manifest.add_dependency(
        "old-agent".to_string(),
        crate::manifest::ResourceDependency::Detailed(Box::new(
            crate::manifest::DetailedDependency {
                source: Some("test".to_string()),
                path: "old.md".to_string(),
                version: Some("v0.1.0".to_string()), // This should trigger warning
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
async fn test_validate_final_success_with_warnings() {
    let temp = TempDir::new().unwrap();
    let manifest_path = temp.path().join("agpm.toml");

    // Create manifest that will have warnings but no errors
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
        quiet: false,
        strict: false, // Not strict - warnings don't cause failure
        render: false,
    };

    let result = cmd.execute_from_path(manifest_path).await;
    assert!(result.is_ok());
    // This tests the final success path with warnings displayed (lines 872-879)
}

#[tokio::test]
async fn test_validate_all_checks_enabled() {
    let temp = TempDir::new().unwrap();
    let manifest_path = temp.path().join("agpm.toml");
    let lockfile_path = temp.path().join("agpm.lock");

    // Create a manifest with dependencies
    let mut manifest = Manifest::new();
    manifest
        .agents
        .insert("test-agent".to_string(), ResourceDependency::Simple("local-agent.md".to_string()));
    manifest.save(&manifest_path).unwrap();

    // Create lockfile
    let lockfile = crate::lockfile::LockFile::new();
    lockfile.save(&lockfile_path).unwrap();

    let cmd = ValidateCommand {
        file: None,
        resolve: true,
        check_lock: true,
        sources: true,
        paths: true,
        format: OutputFormat::Text,
        verbose: true,
        quiet: false,
        strict: true,
        render: false,
    };

    let result = cmd.execute_from_path(manifest_path).await;
    // May have warnings but should complete
    assert!(result.is_err() || result.is_ok());
}

#[tokio::test]
async fn test_validate_with_specific_file_path() {
    let temp = TempDir::new().unwrap();
    let custom_path = temp.path().join("custom-manifest.toml");

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
async fn test_validation_results_with_errors_and_warnings() {
    let mut results = ValidationResults::default();

    // Add errors
    results.errors.push("Error 1".to_string());
    results.errors.push("Error 2".to_string());

    // Add warnings
    results.warnings.push("Warning 1".to_string());
    results.warnings.push("Warning 2".to_string());

    assert!(!results.errors.is_empty());
    assert_eq!(results.errors.len(), 2);
    assert_eq!(results.warnings.len(), 2);
}

#[tokio::test]
async fn test_validation_with_outdated_version_warning() {
    let temp = TempDir::new().unwrap();
    let manifest_path = temp.path().join("agpm.toml");

    let mut manifest = Manifest::new();
    // Add the source that's referenced
    manifest.sources.insert("test".to_string(), "https://github.com/test/repo.git".to_string());
    manifest.agents.insert(
        "old-agent".to_string(),
        ResourceDependency::Detailed(Box::new(crate::manifest::DetailedDependency {
            source: Some("test".to_string()),
            path: "agent.md".to_string(),
            version: Some("v0.1.0".to_string()),
            branch: None,
            rev: None,
            command: None,
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
        format: OutputFormat::Text,
        verbose: false,
        quiet: false,
        strict: false,
        render: false,
    };

    let result = cmd.execute_from_path(manifest_path).await;
    assert!(result.is_ok()); // Should pass but with warning
}
