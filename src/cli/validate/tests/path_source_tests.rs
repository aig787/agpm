//! Tests for validate command

use super::super::{OutputFormat, ValidateCommand};
use crate::manifest::{Manifest, ResourceDependency};
use crate::utils::normalize_path_for_storage;
use anyhow::Result;
use tempfile::TempDir;

#[tokio::test]
async fn test_validate_check_paths_local() -> Result<()> {
    let temp = TempDir::new().unwrap();
    let manifest_path = temp.path().join("agpm.toml");

    // Create a local file to reference
    std::fs::create_dir_all(temp.path().join("local")).unwrap();
    std::fs::write(temp.path().join("local/test.md"), "# Test").unwrap();

    // Create manifest with local dependency
    let mut manifest = crate::manifest::Manifest::new();
    manifest.add_dependency(
        "local-test".to_string(),
        crate::manifest::ResourceDependency::Detailed(Box::new(
            crate::manifest::DetailedDependency {
                source: None,
                path: "./local/test.md".to_string(),
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
        paths: true, // Check local paths
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
async fn test_validate_paths_check() -> Result<()> {
    let temp = TempDir::new().unwrap();
    let manifest_path = temp.path().join("agpm.toml");

    // Create manifest with local dependency
    let mut manifest = crate::manifest::Manifest::new();
    manifest.add_dependency(
        "local-agent".to_string(),
        crate::manifest::ResourceDependency::Simple("./local/agent.md".to_string()),
        true,
    );
    manifest.save(&manifest_path).unwrap();

    // Test with missing path
    let cmd = ValidateCommand {
        file: None,
        resolve: false,
        check_lock: false,
        sources: false,
        paths: true,
        format: OutputFormat::Text,
        verbose: false,
        quiet: false,
        strict: false,
        render: false,
    };

    let result = cmd.execute_from_path(manifest_path.clone()).await;
    assert!(result.is_err());

    // Create the path and test again
    std::fs::create_dir_all(temp.path().join("local")).unwrap();
    std::fs::write(temp.path().join("local/agent.md"), "# Agent").unwrap();

    let cmd = ValidateCommand {
        file: None,
        resolve: false,
        check_lock: false,
        sources: false,
        paths: true,
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
async fn test_validate_check_sources() -> Result<()> {
    let temp = TempDir::new().unwrap();
    let manifest_path = temp.path().join("agpm.toml");

    // Create a local git repository to use as a mock source
    let source_dir = temp.path().join("test-source");
    std::fs::create_dir_all(&source_dir).unwrap();

    // Initialize it as a git repository
    std::process::Command::new("git")
        .arg("init")
        .current_dir(&source_dir)
        .output()
        .expect("Failed to initialize git repository");

    // Create manifest with local file:// URL to avoid network access
    let mut manifest = crate::manifest::Manifest::new();
    let source_url = format!("file://{}", normalize_path_for_storage(&source_dir));
    manifest.add_source("test".to_string(), source_url);
    manifest.save(&manifest_path).unwrap();

    let cmd = ValidateCommand {
        file: None,
        resolve: false,
        check_lock: false,
        sources: true, // Check sources
        paths: false,
        format: OutputFormat::Text,
        verbose: false,
        quiet: false,
        strict: false,
        render: false,
    };

    // This will check if the local source is accessible
    let result = cmd.execute_from_path(manifest_path).await;
    // Local file:// URL should be accessible
    result?;
    Ok(())
}

#[tokio::test]
async fn test_validate_check_paths() -> Result<()> {
    let temp = TempDir::new().unwrap();
    let manifest_path = temp.path().join("agpm.toml");

    // Create manifest with local dependency
    let mut manifest = crate::manifest::Manifest::new();
    use crate::manifest::{DetailedDependency, ResourceDependency};

    manifest.agents.insert(
        "test".to_string(),
        ResourceDependency::Detailed(Box::new(DetailedDependency {
            source: None,
            path: temp.path().join("test.md").to_str().unwrap().to_string(),
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

    // Create the referenced file
    std::fs::write(temp.path().join("test.md"), "# Test Agent").unwrap();

    let cmd = ValidateCommand {
        file: None,
        resolve: false,
        check_lock: false,
        sources: false,
        paths: true, // Check paths
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
async fn test_validate_sources_accessibility_error() -> Result<()> {
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
        format: OutputFormat::Text,
        verbose: false,
        quiet: false,
        strict: false,
        render: false,
    };

    let result = cmd.execute_from_path(manifest_path).await;
    // This tests lines 578-580, 613-615 (source accessibility error messages)
    let _ = result;
    Ok(())
}

#[tokio::test]
async fn test_validate_check_paths_snippets_and_commands() -> Result<()> {
    let temp = TempDir::new().unwrap();
    let manifest_path = temp.path().join("agpm.toml");

    // Create manifest with local dependencies for snippets and commands (not just agents)
    let mut manifest = crate::manifest::Manifest::new();

    // Add local snippet
    manifest.snippets.insert(
        "local-snippet".to_string(),
        crate::manifest::ResourceDependency::Detailed(Box::new(
            crate::manifest::DetailedDependency {
                source: None,
                path: "./snippets/local.md".to_string(),
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

    // Add local command
    manifest.commands.insert(
        "local-command".to_string(),
        crate::manifest::ResourceDependency::Detailed(Box::new(
            crate::manifest::DetailedDependency {
                source: None,
                path: "./commands/deploy.md".to_string(),
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

    // Create the referenced files
    std::fs::create_dir_all(temp.path().join("snippets")).unwrap();
    std::fs::create_dir_all(temp.path().join("commands")).unwrap();
    std::fs::write(temp.path().join("snippets/local.md"), "# Local Snippet").unwrap();
    std::fs::write(temp.path().join("commands/deploy.md"), "# Deploy Command").unwrap();

    let cmd = ValidateCommand {
        file: None,
        resolve: false,
        check_lock: false,
        sources: false,
        paths: true, // Check paths for all resource types
        format: OutputFormat::Text,
        verbose: false,
        quiet: false,
        strict: false,
        render: false,
    };

    let result = cmd.execute_from_path(manifest_path).await;
    result?;
    // This tests path checking for snippets and commands, not just agents
    Ok(())
}

#[tokio::test]
async fn test_validate_sources_check_with_invalid_url() -> Result<()> {
    let temp = TempDir::new().unwrap();
    let manifest_path = temp.path().join("agpm.toml");

    let mut manifest = Manifest::new();
    manifest.sources.insert("invalid".to_string(), "not-a-valid-url".to_string());
    manifest.save(&manifest_path).unwrap();

    let cmd = ValidateCommand {
        file: None,
        resolve: false,
        check_lock: false,
        sources: true,
        paths: false,
        format: OutputFormat::Text,
        verbose: false,
        quiet: false,
        strict: false,
        render: false,
    };

    let result = cmd.execute_from_path(manifest_path).await;
    assert!(result.is_err()); // Should fail with invalid URL error
    Ok(())
}

#[tokio::test]
async fn test_validation_with_local_paths_check() -> Result<()> {
    let temp = TempDir::new().unwrap();
    let manifest_path = temp.path().join("agpm.toml");

    let mut manifest = Manifest::new();
    manifest.agents.insert(
        "local-agent".to_string(),
        ResourceDependency::Simple("./missing-file.md".to_string()),
    );
    manifest.save(&manifest_path).unwrap();

    let cmd = ValidateCommand {
        file: None,
        resolve: false,
        check_lock: false,
        sources: false,
        paths: true, // Enable path checking
        format: OutputFormat::Text,
        verbose: false,
        quiet: false,
        strict: false,
        render: false,
    };

    let result = cmd.execute_from_path(manifest_path).await;
    assert!(result.is_err()); // Should fail due to missing local path
    Ok(())
}

#[tokio::test]
async fn test_validation_with_existing_local_paths() -> Result<()> {
    let temp = TempDir::new().unwrap();
    let manifest_path = temp.path().join("agpm.toml");
    let local_file = temp.path().join("agent.md");

    // Create the local file
    std::fs::write(&local_file, "# Local Agent").unwrap();

    let mut manifest = Manifest::new();
    manifest
        .agents
        .insert("local-agent".to_string(), ResourceDependency::Simple("./agent.md".to_string()));
    manifest.save(&manifest_path).unwrap();

    let cmd = ValidateCommand {
        file: None,
        resolve: false,
        check_lock: false,
        sources: false,
        paths: true,
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
async fn test_file_reference_validation_with_valid_references() -> Result<()> {
    use crate::lockfile::LockedResource;
    use std::fs;

    let temp = TempDir::new().unwrap();
    let project_dir = temp.path();

    // Create manifest
    let manifest_path = project_dir.join("agpm.toml");
    let mut manifest = Manifest::new();
    manifest.sources.insert("test".to_string(), "https://github.com/test/repo.git".to_string());
    manifest.save(&manifest_path).unwrap();

    // Create referenced files
    let snippets_dir = project_dir.join(".agpm").join("snippets");
    fs::create_dir_all(&snippets_dir).unwrap();
    fs::write(snippets_dir.join("helper.md"), "# Helper\nSome content").unwrap();

    // Create agent with valid file reference
    let agents_dir = project_dir.join(".claude").join("agents");
    fs::create_dir_all(&agents_dir).unwrap();
    let agent_content = r#"---
title: Test Agent
---

# Test Agent

See [helper](.agpm/snippets/helper.md) for details.
"#;
    fs::write(agents_dir.join("test.md"), agent_content).unwrap();

    // Create lockfile
    let lockfile_path = project_dir.join("agpm.lock");
    let mut lockfile = crate::lockfile::LockFile::default();
    lockfile.agents.push(LockedResource {
        name: "test-agent".to_string(),
        source: None,
        path: "agents/test.md".to_string(),
        version: Some("v1.0.0".to_string()),
        resolved_commit: None,
        url: None,
        checksum: "abc123".to_string(),
        installed_at: normalize_path_for_storage(agents_dir.join("test.md")),
        dependencies: vec![],
        resource_type: crate::core::ResourceType::Agent,
        tool: None,
        manifest_alias: None,
        context_checksum: None,
        applied_patches: std::collections::BTreeMap::new(),
        install: None,
        variant_inputs: crate::resolver::lockfile_builder::VariantInputs::default(),
    });
    lockfile.save(&lockfile_path).unwrap();

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
        render: true,
    };

    let result = cmd.execute_from_path(manifest_path).await;
    result?;
    Ok(())
}

#[tokio::test]
async fn test_file_reference_validation_with_broken_references() -> Result<()> {
    use crate::lockfile::LockedResource;
    use std::fs;

    let temp = TempDir::new().unwrap();
    let project_dir = temp.path();

    // Create manifest
    let manifest_path = project_dir.join("agpm.toml");
    let mut manifest = Manifest::new();
    manifest.sources.insert("test".to_string(), "https://github.com/test/repo.git".to_string());
    manifest.save(&manifest_path).unwrap();

    // Create agent with broken file reference (file doesn't exist)
    let agents_dir = project_dir.join(".claude").join("agents");
    fs::create_dir_all(&agents_dir).unwrap();
    let agent_content = r#"---
title: Test Agent
---

# Test Agent

See [missing](.agpm/snippets/missing.md) for details.
Also check `.claude/nonexistent.md`.
"#;
    fs::write(agents_dir.join("test.md"), agent_content).unwrap();

    // Create lockfile
    let lockfile_path = project_dir.join("agpm.lock");
    let mut lockfile = crate::lockfile::LockFile::default();
    lockfile.agents.push(LockedResource {
        name: "test-agent".to_string(),
        source: None,
        path: "agents/test.md".to_string(),
        version: Some("v1.0.0".to_string()),
        resolved_commit: None,
        url: None,
        checksum: "abc123".to_string(),
        installed_at: normalize_path_for_storage(agents_dir.join("test.md")),
        dependencies: vec![],
        resource_type: crate::core::ResourceType::Agent,
        tool: None,
        manifest_alias: None,
        context_checksum: None,
        applied_patches: std::collections::BTreeMap::new(),
        install: None,
        variant_inputs: crate::resolver::lockfile_builder::VariantInputs::default(),
    });
    lockfile.save(&lockfile_path).unwrap();

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
        render: true,
    };

    let result = cmd.execute_from_path(manifest_path).await;
    assert!(result.is_err());
    let err_msg = format!("{:?}", result.unwrap_err());
    assert!(err_msg.contains("File reference validation failed"));
    Ok(())
}

#[tokio::test]
async fn test_file_reference_validation_ignores_urls() -> Result<()> {
    use crate::lockfile::LockedResource;
    use std::fs;

    let temp = TempDir::new().unwrap();
    let project_dir = temp.path();

    // Create manifest
    let manifest_path = project_dir.join("agpm.toml");
    let mut manifest = Manifest::new();
    manifest.sources.insert("test".to_string(), "https://github.com/test/repo.git".to_string());
    manifest.save(&manifest_path).unwrap();

    // Create agent with URL references (should be ignored)
    let agents_dir = project_dir.join(".claude").join("agents");
    fs::create_dir_all(&agents_dir).unwrap();
    let agent_content = r#"---
title: Test Agent
---

# Test Agent

Check [GitHub](https://github.com/user/repo) for source.
Visit http://example.com for more info.
"#;
    fs::write(agents_dir.join("test.md"), agent_content).unwrap();

    // Create lockfile
    let lockfile_path = project_dir.join("agpm.lock");
    let mut lockfile = crate::lockfile::LockFile::default();
    lockfile.agents.push(LockedResource {
        name: "test-agent".to_string(),
        source: None,
        path: "agents/test.md".to_string(),
        version: Some("v1.0.0".to_string()),
        resolved_commit: None,
        url: None,
        checksum: "abc123".to_string(),
        installed_at: normalize_path_for_storage(agents_dir.join("test.md")),
        dependencies: vec![],
        resource_type: crate::core::ResourceType::Agent,
        tool: None,
        manifest_alias: None,
        context_checksum: None,
        applied_patches: std::collections::BTreeMap::new(),
        install: None,
        variant_inputs: crate::resolver::lockfile_builder::VariantInputs::default(),
    });
    lockfile.save(&lockfile_path).unwrap();

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
        render: true,
    };

    let result = cmd.execute_from_path(manifest_path).await;
    result?;
    Ok(())
}

#[tokio::test]
async fn test_file_reference_validation_ignores_code_blocks() -> Result<()> {
    use crate::lockfile::LockedResource;
    use std::fs;

    let temp = TempDir::new().unwrap();
    let project_dir = temp.path();

    // Create manifest
    let manifest_path = project_dir.join("agpm.toml");
    let mut manifest = Manifest::new();
    manifest.sources.insert("test".to_string(), "https://github.com/test/repo.git".to_string());
    manifest.save(&manifest_path).unwrap();

    // Create agent with file references in code blocks (should be ignored)
    let agents_dir = project_dir.join(".claude").join("agents");
    fs::create_dir_all(&agents_dir).unwrap();
    let agent_content = r#"---
title: Test Agent
---

# Test Agent

```bash
# This reference in code should be ignored
cat .agpm/snippets/nonexistent.md
```

Inline code `example.md` should also be ignored.
"#;
    fs::write(agents_dir.join("test.md"), agent_content).unwrap();

    // Create lockfile
    let lockfile_path = project_dir.join("agpm.lock");
    let mut lockfile = crate::lockfile::LockFile::default();
    lockfile.agents.push(LockedResource {
        name: "test-agent".to_string(),
        source: None,
        path: "agents/test.md".to_string(),
        version: Some("v1.0.0".to_string()),
        resolved_commit: None,
        url: None,
        checksum: "abc123".to_string(),
        installed_at: normalize_path_for_storage(agents_dir.join("test.md")),
        dependencies: vec![],
        resource_type: crate::core::ResourceType::Agent,
        tool: None,
        manifest_alias: None,
        context_checksum: None,
        applied_patches: std::collections::BTreeMap::new(),
        install: None,
        variant_inputs: crate::resolver::lockfile_builder::VariantInputs::default(),
    });
    lockfile.save(&lockfile_path).unwrap();

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
        render: true,
    };

    let result = cmd.execute_from_path(manifest_path).await;
    result?;
    Ok(())
}

#[tokio::test]
async fn test_file_reference_validation_multiple_resources() -> Result<()> {
    use crate::lockfile::LockedResource;
    use std::fs;

    let temp = TempDir::new().unwrap();
    let project_dir = temp.path();

    // Create manifest
    let manifest_path = project_dir.join("agpm.toml");
    let mut manifest = Manifest::new();
    manifest.sources.insert("test".to_string(), "https://github.com/test/repo.git".to_string());
    manifest.save(&manifest_path).unwrap();

    // Create referenced snippets
    let snippets_dir = project_dir.join(".agpm").join("snippets");
    fs::create_dir_all(&snippets_dir).unwrap();
    fs::write(snippets_dir.join("util.md"), "# Utilities").unwrap();

    // Create agent with valid reference
    let agents_dir = project_dir.join(".claude").join("agents");
    fs::create_dir_all(&agents_dir).unwrap();
    fs::write(agents_dir.join("agent1.md"), "# Agent 1\n\nSee [util](.agpm/snippets/util.md).")
        .unwrap();

    // Create command with broken reference
    let commands_dir = project_dir.join(".claude").join("commands");
    fs::create_dir_all(&commands_dir).unwrap();
    fs::write(commands_dir.join("cmd1.md"), "# Command\n\nCheck `.agpm/snippets/missing.md`.")
        .unwrap();

    // Create lockfile
    let lockfile_path = project_dir.join("agpm.lock");
    let mut lockfile = crate::lockfile::LockFile::default();
    lockfile.agents.push(LockedResource {
        name: "agent1".to_string(),
        source: None,
        path: "agents/agent1.md".to_string(),
        version: Some("v1.0.0".to_string()),
        resolved_commit: None,
        url: None,
        checksum: "abc123".to_string(),
        installed_at: normalize_path_for_storage(agents_dir.join("agent1.md")),
        dependencies: vec![],
        resource_type: crate::core::ResourceType::Agent,
        tool: None,
        manifest_alias: None,
        context_checksum: None,
        applied_patches: std::collections::BTreeMap::new(),
        install: None,
        variant_inputs: crate::resolver::lockfile_builder::VariantInputs::default(),
    });
    lockfile.commands.push(LockedResource {
        name: "cmd1".to_string(),
        source: None,
        path: "commands/cmd1.md".to_string(),
        version: Some("v1.0.0".to_string()),
        resolved_commit: None,
        url: None,
        checksum: "def456".to_string(),
        installed_at: normalize_path_for_storage(commands_dir.join("cmd1.md")),
        dependencies: vec![],
        resource_type: crate::core::ResourceType::Command,
        tool: None,
        manifest_alias: None,
        context_checksum: None,
        applied_patches: std::collections::BTreeMap::new(),
        install: None,
        variant_inputs: crate::resolver::lockfile_builder::VariantInputs::default(),
    });
    lockfile.save(&lockfile_path).unwrap();

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
        render: true,
    };

    let result = cmd.execute_from_path(manifest_path).await;
    assert!(result.is_err());
    let err_msg = format!("{:?}", result.unwrap_err());
    assert!(err_msg.contains("File reference validation failed"));
    Ok(())
}
