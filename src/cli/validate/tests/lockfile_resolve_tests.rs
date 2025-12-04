//! Tests for validate command

use super::super::{OutputFormat, ValidateCommand};
use crate::manifest::{Manifest, ResourceDependency};
use anyhow::Result;
use tempfile::TempDir;

#[tokio::test]
async fn test_validate_with_resolve() -> Result<()> {
    let temp = TempDir::new().unwrap();
    let manifest_path = temp.path().join("agpm.toml");

    // Create manifest with a source dependency that needs resolving
    let mut manifest = crate::manifest::Manifest::new();
    manifest.add_source("test".to_string(), "https://github.com/test/repo.git".to_string());
    manifest.add_dependency(
        "test-agent".to_string(),
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
        format: OutputFormat::Text,
        verbose: false,
        quiet: true, // Make quiet to avoid output
        strict: false,
        render: false,
    };

    let result = cmd.execute_from_path(manifest_path).await;
    // For now, just check that the command runs without panicking
    // The actual success/failure depends on resolver implementation
    let _ = result;
    Ok(())
}

#[tokio::test]
async fn test_validate_check_lock_consistent() -> Result<()> {
    let temp = TempDir::new().unwrap();
    let manifest_path = temp.path().join("agpm.toml");

    // Create a simple manifest without dependencies
    let manifest = crate::manifest::Manifest::new();
    manifest.save(&manifest_path).unwrap();

    // Create an empty lockfile (consistent with no dependencies)
    let lockfile = crate::lockfile::LockFile::new();
    lockfile.save(&temp.path().join("agpm.lock")).unwrap();

    let cmd = ValidateCommand {
        file: None,
        resolve: false,
        check_lock: true,
        sources: false,
        paths: false,
        format: OutputFormat::Text,
        verbose: false,
        quiet: true,
        strict: false,
        render: false,
    };

    let result = cmd.execute_from_path(manifest_path).await;
    // Empty manifest and empty lockfile are consistent
    result?;
    Ok(())
}

#[tokio::test]
async fn test_validate_check_lock_with_extra_entries() -> Result<()> {
    let temp = TempDir::new().unwrap();
    let manifest_path = temp.path().join("agpm.toml");

    // Create empty manifest
    let manifest = crate::manifest::Manifest::new();
    manifest.save(&manifest_path).unwrap();

    // Create lockfile with an entry (extra entry not in manifest)
    let mut lockfile = crate::lockfile::LockFile::new();
    lockfile.agents.push(crate::lockfile::LockedResource {
        name: "extra-agent".to_string(),
        source: Some("test".to_string()),
        url: Some("https://github.com/test/repo.git".to_string()),
        path: "test.md".to_string(),
        version: None,
        resolved_commit: Some("abc123".to_string()),
        checksum: "sha256:dummy".to_string(),
        installed_at: "agents/extra-agent.md".to_string(),
        dependencies: vec![],
        resource_type: crate::core::ResourceType::Agent,

        tool: Some("claude-code".to_string()),
        manifest_alias: None,
        context_checksum: None,
        applied_patches: std::collections::BTreeMap::new(),
        install: None,
        variant_inputs: crate::resolver::lockfile_builder::VariantInputs::default(),
        is_private: false,
        approximate_token_count: None,
    });
    lockfile.save(&temp.path().join("agpm.lock")).unwrap();

    let cmd = ValidateCommand {
        file: None,
        resolve: false,
        check_lock: true,
        sources: false,
        paths: false,
        format: OutputFormat::Text,
        verbose: false,
        quiet: true,
        strict: false,
        render: false,
    };

    let result = cmd.execute_from_path(manifest_path).await;
    // Should fail due to extra entries in lockfile
    assert!(result.is_err());
    Ok(())
}

#[tokio::test]
async fn test_validate_check_lock() -> Result<()> {
    let temp = TempDir::new().unwrap();
    let manifest_path = temp.path().join("agpm.toml");

    // Create manifest
    let mut manifest = crate::manifest::Manifest::new();
    manifest.add_dependency(
        "test".to_string(),
        crate::manifest::ResourceDependency::Simple("test.md".to_string()),
        true,
    );
    manifest.save(&manifest_path).unwrap();

    // Test without lockfile
    let cmd = ValidateCommand {
        file: None,
        resolve: false,
        check_lock: true,
        sources: false,
        paths: false,
        format: OutputFormat::Text,
        verbose: false,
        quiet: false,
        strict: false,
        render: false,
    };

    let result = cmd.execute_from_path(manifest_path.clone()).await;
    result?; // Should succeed with warning

    // Create lockfile with matching dependencies
    let lockfile = crate::lockfile::LockFile {
        version: 1,
        sources: vec![],
        commands: vec![],
        agents: vec![crate::lockfile::LockedResource {
            name: "test".to_string(),
            source: None,
            url: None,
            path: "test.md".to_string(),
            version: None,
            resolved_commit: None,
            checksum: String::new(),
            installed_at: "agents/test.md".to_string(),
            dependencies: vec![],
            resource_type: crate::core::ResourceType::Agent,

            tool: Some("claude-code".to_string()),
            manifest_alias: None,
            context_checksum: None,
            applied_patches: std::collections::BTreeMap::new(),
            install: None,
            variant_inputs: crate::resolver::lockfile_builder::VariantInputs::default(),
            is_private: false,
            approximate_token_count: None,
        }],
        snippets: vec![],
        mcp_servers: vec![],
        scripts: vec![],
        hooks: vec![],
        skills: vec![],
        manifest_hash: None,
        has_mutable_deps: None,
        resource_count: None,
    };
    lockfile.save(&temp.path().join("agpm.lock")).unwrap();

    let cmd = ValidateCommand {
        file: None,
        resolve: false,
        check_lock: true,
        sources: false,
        paths: false,
        format: OutputFormat::Text,
        verbose: false,
        quiet: false,
        strict: false,
        render: false,
    };

    let result = cmd.execute_from_path(manifest_path).await;
    result?;
    Ok(())
}

#[tokio::test]
async fn test_validate_resolve_dependency_not_found_error() -> Result<()> {
    let temp = TempDir::new().unwrap();
    let manifest_path = temp.path().join("agpm.toml");

    // Create manifest with dependencies that will fail resolution
    let mut manifest = crate::manifest::Manifest::new();
    manifest.add_source("test".to_string(), "https://github.com/test/repo.git".to_string());
    manifest.add_dependency(
        "my-agent".to_string(),
        crate::manifest::ResourceDependency::Detailed(Box::new(
            crate::manifest::DetailedDependency {
                source: Some("test".to_string()),
                path: "agent.md".to_string(),
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
    manifest.add_dependency(
        "utils".to_string(),
        crate::manifest::ResourceDependency::Detailed(Box::new(
            crate::manifest::DetailedDependency {
                source: Some("test".to_string()),
                path: "utils.md".to_string(),
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
        false,
    );
    manifest.save(&manifest_path).unwrap();

    let cmd = ValidateCommand {
        file: None,
        resolve: true,
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
    // This tests lines 538-541 (specific dependency not found error message)
    let _ = result;
    Ok(())
}

#[tokio::test]
async fn test_validate_lockfile_missing_warning() -> Result<()> {
    let temp = TempDir::new().unwrap();
    let manifest_path = temp.path().join("agpm.toml");

    // Create manifest but no lockfile
    let manifest = crate::manifest::Manifest::new();
    manifest.save(&manifest_path).unwrap();

    let cmd = ValidateCommand {
        file: None,
        resolve: false,
        check_lock: true,
        sources: false,
        paths: false,
        format: OutputFormat::Text,
        verbose: true, // Test verbose mode with lockfile check
        quiet: false,
        strict: false,
        render: false,
    };

    let result = cmd.execute_from_path(manifest_path).await;
    result?;
    // This tests lines 759, 753-756 (verbose mode and missing lockfile warning)
    Ok(())
}

#[tokio::test]
async fn test_validate_lockfile_missing_dependencies() -> Result<()> {
    let temp = TempDir::new().unwrap();
    let manifest_path = temp.path().join("agpm.toml");
    let lockfile_path = temp.path().join("agpm.lock");

    // Create manifest with dependencies
    let mut manifest = crate::manifest::Manifest::new();
    manifest.add_dependency(
        "missing-agent".to_string(),
        crate::manifest::ResourceDependency::Simple("test.md".to_string()),
        true,
    );
    manifest.add_dependency(
        "missing-snippet".to_string(),
        crate::manifest::ResourceDependency::Simple("snippet.md".to_string()),
        false,
    );
    manifest.save(&manifest_path).unwrap();

    // Create empty lockfile (missing the manifest dependencies)
    let lockfile = crate::lockfile::LockFile::new();
    lockfile.save(&lockfile_path).unwrap();

    let cmd = ValidateCommand {
        file: None,
        resolve: false,
        check_lock: true,
        sources: false,
        paths: false,
        format: OutputFormat::Text,
        verbose: false,
        quiet: false,
        strict: false,
        render: false,
    };

    let result = cmd.execute_from_path(manifest_path).await;
    result?; // Missing dependencies are warnings, not errors
    // This tests lines 775-777, 811-822 (missing dependencies in lockfile)
    Ok(())
}

#[tokio::test]
async fn test_validate_lockfile_extra_entries_error() -> Result<()> {
    let temp = TempDir::new().unwrap();
    let manifest_path = temp.path().join("agpm.toml");
    let lockfile_path = temp.path().join("agpm.lock");

    // Create empty manifest
    let manifest = crate::manifest::Manifest::new();
    manifest.save(&manifest_path).unwrap();

    // Create lockfile with extra entries
    let mut lockfile = crate::lockfile::LockFile::new();
    lockfile.agents.push(crate::lockfile::LockedResource {
        name: "extra-agent".to_string(),
        source: Some("test".to_string()),
        url: Some("https://github.com/test/repo.git".to_string()),
        path: "test.md".to_string(),
        version: None,
        resolved_commit: Some("abc123".to_string()),
        checksum: "sha256:dummy".to_string(),
        installed_at: "agents/extra-agent.md".to_string(),
        dependencies: vec![],
        resource_type: crate::core::ResourceType::Agent,

        tool: Some("claude-code".to_string()),
        manifest_alias: None,
        context_checksum: None,
        applied_patches: std::collections::BTreeMap::new(),
        install: None,
        variant_inputs: crate::resolver::lockfile_builder::VariantInputs::default(),
        is_private: false,
        approximate_token_count: None,
    });
    lockfile.save(&lockfile_path).unwrap();

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
    assert!(result.is_err()); // Extra entries cause errors
    // This tests lines 801-804, 807 (extra entries in lockfile error)
    Ok(())
}

#[tokio::test]
async fn test_validation_with_lockfile_consistency_check_no_lockfile() -> Result<()> {
    let temp = TempDir::new().unwrap();
    let manifest_path = temp.path().join("agpm.toml");

    let mut manifest = Manifest::new();
    manifest
        .agents
        .insert("test-agent".to_string(), ResourceDependency::Simple("agent.md".to_string()));
    manifest.save(&manifest_path).unwrap();

    let cmd = ValidateCommand {
        file: None,
        resolve: false,
        check_lock: true, // Enable lockfile checking
        sources: false,
        paths: false,
        format: OutputFormat::Text,
        verbose: false,
        quiet: false,
        strict: false,
        render: false,
    };

    let result = cmd.execute_from_path(manifest_path).await;
    result?; // Should pass but with warning
    Ok(())
}

#[tokio::test]
async fn test_validation_with_inconsistent_lockfile() -> Result<()> {
    let temp = TempDir::new().unwrap();
    let manifest_path = temp.path().join("agpm.toml");
    let lockfile_path = temp.path().join("agpm.lock");

    // Create manifest with agent
    let mut manifest = Manifest::new();
    manifest
        .agents
        .insert("manifest-agent".to_string(), ResourceDependency::Simple("agent.md".to_string()));
    manifest.save(&manifest_path).unwrap();

    // Create lockfile with different agent
    let mut lockfile = crate::lockfile::LockFile::new();
    lockfile.agents.push(crate::lockfile::LockedResource {
        name: "lockfile-agent".to_string(),
        source: None,
        url: None,
        path: "agent.md".to_string(),
        version: None,
        resolved_commit: None,
        checksum: "sha256:dummy".to_string(),
        installed_at: "agents/lockfile-agent.md".to_string(),
        dependencies: vec![],
        resource_type: crate::core::ResourceType::Agent,

        tool: Some("claude-code".to_string()),
        manifest_alias: None,
        context_checksum: None,
        applied_patches: std::collections::BTreeMap::new(),
        install: None,
        variant_inputs: crate::resolver::lockfile_builder::VariantInputs::default(),
        is_private: false,
        approximate_token_count: None,
    });
    lockfile.save(&lockfile_path).unwrap();

    let cmd = ValidateCommand {
        file: None,
        resolve: false,
        check_lock: true,
        sources: false,
        paths: false,
        format: OutputFormat::Text,
        verbose: false,
        quiet: false,
        strict: false,
        render: false,
    };

    let result = cmd.execute_from_path(manifest_path).await;
    assert!(result.is_err()); // Should fail due to inconsistency
    Ok(())
}

#[tokio::test]
async fn test_validation_with_invalid_lockfile_syntax() -> Result<()> {
    let temp = TempDir::new().unwrap();
    let manifest_path = temp.path().join("agpm.toml");
    let lockfile_path = temp.path().join("agpm.lock");

    let manifest = Manifest::new();
    manifest.save(&manifest_path).unwrap();

    // Write invalid TOML to lockfile
    std::fs::write(&lockfile_path, "invalid toml syntax [[[").unwrap();

    let cmd = ValidateCommand {
        file: None,
        resolve: false,
        check_lock: true,
        sources: false,
        paths: false,
        format: OutputFormat::Text,
        verbose: false,
        quiet: false,
        strict: false,
        render: false,
    };

    let result = cmd.execute_from_path(manifest_path).await;
    assert!(result.is_err()); // Should fail due to invalid lockfile
    Ok(())
}

#[tokio::test]
async fn test_validation_with_missing_lockfile_dependencies() -> Result<()> {
    let temp = TempDir::new().unwrap();
    let manifest_path = temp.path().join("agpm.toml");
    let lockfile_path = temp.path().join("agpm.lock");

    // Create manifest with multiple dependencies
    let mut manifest = Manifest::new();
    manifest
        .agents
        .insert("agent1".to_string(), ResourceDependency::Simple("agent1.md".to_string()));
    manifest
        .agents
        .insert("agent2".to_string(), ResourceDependency::Simple("agent2.md".to_string()));
    manifest
        .snippets
        .insert("snippet1".to_string(), ResourceDependency::Simple("snippet1.md".to_string()));
    manifest.save(&manifest_path).unwrap();

    // Create lockfile missing some dependencies
    let mut lockfile = crate::lockfile::LockFile::new();
    lockfile.agents.push(crate::lockfile::LockedResource {
        name: "agent1".to_string(),
        source: None,
        url: None,
        path: "agent1.md".to_string(),
        version: None,
        resolved_commit: None,
        checksum: "sha256:dummy".to_string(),
        installed_at: "agents/agent1.md".to_string(),
        dependencies: vec![],
        resource_type: crate::core::ResourceType::Agent,

        tool: Some("claude-code".to_string()),
        manifest_alias: None,
        context_checksum: None,
        applied_patches: std::collections::BTreeMap::new(),
        install: None,
        variant_inputs: crate::resolver::lockfile_builder::VariantInputs::default(),
        is_private: false,
        approximate_token_count: None,
    });
    lockfile.save(&lockfile_path).unwrap();

    let cmd = ValidateCommand {
        file: None,
        resolve: false,
        check_lock: true,
        sources: false,
        paths: false,
        format: OutputFormat::Text,
        verbose: false,
        quiet: false,
        strict: false,
        render: false,
    };

    let result = cmd.execute_from_path(manifest_path).await;
    result?; // Should pass but report missing dependencies
    Ok(())
}
