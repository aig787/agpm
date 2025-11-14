use super::converters;
use super::*;
use crate::lockfile::{LockedResource, LockedSource};
use crate::manifest::{DetailedDependency, ResourceDependency};

use anyhow::Result;
use tempfile::TempDir;

fn create_default_command() -> ListCommand {
    ListCommand {
        agents: false,
        snippets: false,
        commands: false,
        format: "table".to_string(),
        manifest: false,
        r#type: None,
        source: None,
        search: None,
        detailed: false,
        files: false,
        verbose: false,
        sort: None,
    }
}

fn create_test_manifest() -> crate::manifest::Manifest {
    let mut manifest = crate::manifest::Manifest::new();

    // Add sources
    manifest
        .sources
        .insert("official".to_string(), "https://github.com/example/official.git".to_string());
    manifest
        .sources
        .insert("community".to_string(), "https://github.com/example/community.git".to_string());

    // Add agents
    manifest.agents.insert(
        "code-reviewer".to_string(),
        ResourceDependency::Detailed(Box::new(DetailedDependency {
            source: Some("official".to_string()),
            path: "agents/reviewer.md".to_string(),
            version: Some("v1.0.0".to_string()),
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

    manifest.agents.insert(
        "local-helper".to_string(),
        ResourceDependency::Simple("../local/helper.md".to_string()),
    );

    // Add snippets
    manifest.snippets.insert(
        "utils".to_string(),
        ResourceDependency::Detailed(Box::new(DetailedDependency {
            source: Some("community".to_string()),
            path: "snippets/utils.md".to_string(),
            version: Some("v1.2.0".to_string()),
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

    manifest.snippets.insert(
        "local-snippet".to_string(),
        ResourceDependency::Simple("./snippets/local.md".to_string()),
    );

    manifest
}

fn create_test_lockfile() -> crate::lockfile::LockFile {
    let mut lockfile = crate::lockfile::LockFile::new();

    // Add sources
    lockfile.sources.push(LockedSource {
        name: "official".to_string(),
        url: "https://github.com/example/official.git".to_string(),
        fetched_at: "2024-01-01T00:00:00Z".to_string(),
    });

    lockfile.sources.push(LockedSource {
        name: "community".to_string(),
        url: "https://github.com/example/community.git".to_string(),
        fetched_at: "2024-01-01T00:00:00Z".to_string(),
    });

    // Add agents
    lockfile.agents.push(LockedResource {
        name: "code-reviewer".to_string(),
        source: Some("official".to_string()),
        url: Some("https://github.com/example/official.git".to_string()),
        path: "agents/reviewer.md".to_string(),
        version: Some("v1.0.0".to_string()),
        resolved_commit: Some("abc123def456".to_string()),
        checksum: "sha256:abc123".to_string(),
        installed_at: "agents/code-reviewer.md".to_string(),
        dependencies: vec![],
        resource_type: crate::core::ResourceType::Agent,

        tool: Some("claude-code".to_string()),
        manifest_alias: None,
        context_checksum: None,
        applied_patches: std::collections::BTreeMap::new(),
        install: None,
        variant_inputs: crate::resolver::lockfile_builder::VariantInputs::default(),
    });

    lockfile.agents.push(LockedResource {
        name: "local-helper".to_string(),
        source: None,
        url: None,
        path: "../local/helper.md".to_string(),
        version: None,
        resolved_commit: None,
        checksum: "sha256:def456".to_string(),
        installed_at: "agents/local-helper.md".to_string(),
        dependencies: vec![],
        resource_type: crate::core::ResourceType::Agent,

        tool: Some("claude-code".to_string()),
        manifest_alias: None,
        context_checksum: None,
        applied_patches: std::collections::BTreeMap::new(),
        install: None,
        variant_inputs: crate::resolver::lockfile_builder::VariantInputs::default(),
    });

    // Add snippets
    lockfile.snippets.push(LockedResource {
        name: "utils".to_string(),
        source: Some("community".to_string()),
        url: Some("https://github.com/example/community.git".to_string()),
        path: "snippets/utils.md".to_string(),
        version: Some("v1.2.0".to_string()),
        resolved_commit: Some("def456ghi789".to_string()),
        checksum: "sha256:ghi789".to_string(),
        installed_at: "snippets/utils.md".to_string(),
        dependencies: vec![],
        resource_type: crate::core::ResourceType::Snippet,

        tool: Some("claude-code".to_string()),
        manifest_alias: None,
        context_checksum: None,
        applied_patches: std::collections::BTreeMap::new(),
        install: None,
        variant_inputs: crate::resolver::lockfile_builder::VariantInputs::default(),
    });

    lockfile
}

#[tokio::test]
async fn test_list_no_manifest() {
    let temp = TempDir::new().unwrap();
    // Don't create agpm.toml
    let manifest_path = temp.path().join("agpm.toml");

    let cmd = create_default_command();

    // This should fail because there's no manifest
    let result = cmd.execute_from_path(manifest_path).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_list_empty_manifest() -> Result<()> {
    let temp = TempDir::new().unwrap();
    let manifest_path = temp.path().join("agpm.toml");

    // Create empty manifest
    let manifest = crate::manifest::Manifest::new();
    manifest.save(&manifest_path).unwrap();

    let cmd = ListCommand {
        manifest: true,
        ..create_default_command()
    };

    let result = cmd.execute_from_path(manifest_path).await;
    result?;
    Ok(())
}

#[tokio::test]
async fn test_list_from_manifest_with_resources() -> Result<()> {
    let temp = TempDir::new().unwrap();
    let manifest_path = temp.path().join("agpm.toml");

    // Create manifest with resources
    let manifest = create_test_manifest();
    manifest.save(&manifest_path).unwrap();

    let cmd = ListCommand {
        manifest: true,
        ..create_default_command()
    };

    let result = cmd.execute_from_path(manifest_path).await;
    result?;
    Ok(())
}

#[tokio::test]
async fn test_list_from_lockfile_no_lockfile() -> Result<()> {
    let temp = TempDir::new().unwrap();
    let manifest_path = temp.path().join("agpm.toml");

    // Create manifest but no lockfile
    let manifest = create_test_manifest();
    manifest.save(&manifest_path).unwrap();

    let cmd = create_default_command(); // manifest = false by default

    let result = cmd.execute_from_path(manifest_path).await;
    result?;
    Ok(())
}

#[tokio::test]
async fn test_list_from_lockfile_with_resources() -> Result<()> {
    let temp = TempDir::new().unwrap();
    let manifest_path = temp.path().join("agpm.toml");
    let lockfile_path = temp.path().join("agpm.lock");

    // Create both manifest and lockfile
    let manifest = create_test_manifest();
    manifest.save(&manifest_path).unwrap();

    let lockfile = create_test_lockfile();
    lockfile.save(&lockfile_path).unwrap();

    let cmd = create_default_command(); // manifest = false by default

    let result = cmd.execute_from_path(manifest_path).await;
    result?;
    Ok(())
}

#[test]
fn test_validate_arguments_valid_format() -> Result<()> {
    let valid_formats = ["table", "json", "yaml", "compact", "simple"];

    for format in valid_formats {
        let cmd = ListCommand {
            format: format.to_string(),
            ..create_default_command()
        };
        cmd.validate_arguments()?;
    }
    Ok(())
}

#[test]
fn test_validate_arguments_invalid_format() -> Result<()> {
    let cmd = ListCommand {
        format: "invalid".to_string(),
        ..create_default_command()
    };

    let result = cmd.validate_arguments();
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("Invalid format"));
    Ok(())
}

#[test]
fn test_validate_arguments_valid_type() -> Result<()> {
    let valid_types = ["agents", "snippets"];

    for type_name in valid_types {
        let cmd = ListCommand {
            r#type: Some(type_name.to_string()),
            ..create_default_command()
        };
        cmd.validate_arguments()?;
    }
    Ok(())
}

#[test]
fn test_validate_arguments_invalid_type() -> Result<()> {
    let cmd = ListCommand {
        r#type: Some("invalid".to_string()),
        ..create_default_command()
    };

    let result = cmd.validate_arguments();
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("Invalid type"));
    Ok(())
}

#[test]
fn test_validate_arguments_valid_sort() -> Result<()> {
    let valid_sorts = ["name", "version", "source", "type"];

    for sort in valid_sorts {
        let cmd = ListCommand {
            sort: Some(sort.to_string()),
            ..create_default_command()
        };
        cmd.validate_arguments()?;
    }
    Ok(())
}

#[test]
fn test_validate_arguments_invalid_sort() -> Result<()> {
    let cmd = ListCommand {
        sort: Some("invalid".to_string()),
        ..create_default_command()
    };

    let result = cmd.validate_arguments();
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("Invalid sort field"));
    Ok(())
}

#[test]
fn test_should_show_agents() -> Result<()> {
    // Show agents when no specific type filter
    let cmd = create_default_command();
    assert!(cmd.should_show_resource_type(crate::core::ResourceType::Agent));

    // Show only agents when agents flag is set
    let cmd = ListCommand {
        agents: true,
        ..create_default_command()
    };
    assert!(cmd.should_show_resource_type(crate::core::ResourceType::Agent));

    // Don't show agents when snippets flag is set
    let cmd = ListCommand {
        snippets: true,
        ..create_default_command()
    };
    assert!(!cmd.should_show_resource_type(crate::core::ResourceType::Agent));

    // Show agents when type is "agents"
    let cmd = ListCommand {
        r#type: Some("agents".to_string()),
        ..create_default_command()
    };
    assert!(cmd.should_show_resource_type(crate::core::ResourceType::Agent));

    // Don't show agents when type is "snippets"
    let cmd = ListCommand {
        r#type: Some("snippets".to_string()),
        ..create_default_command()
    };
    assert!(!cmd.should_show_resource_type(crate::core::ResourceType::Agent));
    Ok(())
}

#[test]
fn test_should_show_snippets() -> Result<()> {
    // Show snippets when no specific type filter
    let cmd = create_default_command();
    assert!(cmd.should_show_resource_type(crate::core::ResourceType::Snippet));

    // Don't show snippets when agents flag is set
    let cmd = ListCommand {
        agents: true,
        ..create_default_command()
    };
    assert!(!cmd.should_show_resource_type(crate::core::ResourceType::Snippet));

    // Show only snippets when snippets flag is set
    let cmd = ListCommand {
        snippets: true,
        ..create_default_command()
    };
    assert!(cmd.should_show_resource_type(crate::core::ResourceType::Snippet));

    // Don't show snippets when type is "agents"
    let cmd = ListCommand {
        r#type: Some("agents".to_string()),
        ..create_default_command()
    };
    assert!(!cmd.should_show_resource_type(crate::core::ResourceType::Snippet));

    // Show snippets when type is "snippets"
    let cmd = ListCommand {
        r#type: Some("snippets".to_string()),
        ..create_default_command()
    };
    assert!(cmd.should_show_resource_type(crate::core::ResourceType::Snippet));
    Ok(())
}

#[test]
fn test_should_show_commands() -> Result<()> {
    // Show commands when no specific type filter
    let cmd = create_default_command();
    assert!(cmd.should_show_resource_type(crate::core::ResourceType::Command));

    // Don't show commands when agents flag is set
    let cmd = ListCommand {
        agents: true,
        ..create_default_command()
    };
    assert!(!cmd.should_show_resource_type(crate::core::ResourceType::Command));

    // Don't show commands when snippets flag is set
    let cmd = ListCommand {
        snippets: true,
        ..create_default_command()
    };
    assert!(!cmd.should_show_resource_type(crate::core::ResourceType::Command));

    // Show only commands when commands flag is set
    let cmd = ListCommand {
        commands: true,
        ..create_default_command()
    };
    assert!(cmd.should_show_resource_type(crate::core::ResourceType::Command));

    // Don't show other types when commands flag is set
    assert!(!cmd.should_show_resource_type(crate::core::ResourceType::Agent));
    assert!(!cmd.should_show_resource_type(crate::core::ResourceType::Snippet));

    // Show commands when type is "commands" or "command"
    let cmd = ListCommand {
        r#type: Some("commands".to_string()),
        ..create_default_command()
    };
    assert!(cmd.should_show_resource_type(crate::core::ResourceType::Command));

    let cmd = ListCommand {
        r#type: Some("command".to_string()),
        ..create_default_command()
    };
    assert!(cmd.should_show_resource_type(crate::core::ResourceType::Command));
    Ok(())
}

#[test]
fn test_mutually_exclusive_type_filters() -> Result<()> {
    // Test that only one type shows when flags are set individually
    let cmd = ListCommand {
        agents: true,
        ..create_default_command()
    };
    assert!(cmd.should_show_resource_type(crate::core::ResourceType::Agent));
    assert!(!cmd.should_show_resource_type(crate::core::ResourceType::Snippet));
    assert!(!cmd.should_show_resource_type(crate::core::ResourceType::Command));

    let cmd = ListCommand {
        snippets: true,
        ..create_default_command()
    };
    assert!(!cmd.should_show_resource_type(crate::core::ResourceType::Agent));
    assert!(cmd.should_show_resource_type(crate::core::ResourceType::Snippet));
    assert!(!cmd.should_show_resource_type(crate::core::ResourceType::Command));

    let cmd = ListCommand {
        commands: true,
        ..create_default_command()
    };
    assert!(!cmd.should_show_resource_type(crate::core::ResourceType::Agent));
    assert!(!cmd.should_show_resource_type(crate::core::ResourceType::Snippet));
    assert!(cmd.should_show_resource_type(crate::core::ResourceType::Command));
    Ok(())
}

#[test]
fn test_matches_filters_source() -> Result<()> {
    let cmd = ListCommand {
        source: Some("official".to_string()),
        ..create_default_command()
    };

    let dep_with_source = ResourceDependency::Detailed(Box::new(DetailedDependency {
        source: Some("official".to_string()),
        path: "agents/test.md".to_string(),
        version: Some("v1.0.0".to_string()),
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
    }));

    let dep_with_different_source = ResourceDependency::Detailed(Box::new(DetailedDependency {
        source: Some("community".to_string()),
        path: "agents/test.md".to_string(),
        version: Some("v1.0.0".to_string()),
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
    }));

    let dep_without_source = ResourceDependency::Simple("local/file.md".to_string());

    assert!(cmd.matches_filters("test", Some(&dep_with_source), "agent"));
    assert!(!cmd.matches_filters("test", Some(&dep_with_different_source), "agent"));
    assert!(!cmd.matches_filters("test", Some(&dep_without_source), "agent"));
    Ok(())
}

#[test]
fn test_matches_filters_search() -> Result<()> {
    let cmd = ListCommand {
        search: Some("code".to_string()),
        ..create_default_command()
    };

    assert!(cmd.matches_filters("code-reviewer", None, "agent"));
    assert!(cmd.matches_filters("my-code-helper", None, "agent"));
    assert!(!cmd.matches_filters("utils", None, "agent"));
    Ok(())
}

#[test]
fn test_matches_lockfile_filters_source() -> Result<()> {
    let cmd = ListCommand {
        source: Some("official".to_string()),
        ..create_default_command()
    };

    let entry_with_source = LockedResource {
        name: "test".to_string(),
        source: Some("official".to_string()),
        url: None,
        path: "test.md".to_string(),
        version: None,
        resolved_commit: None,
        checksum: "abc123".to_string(),
        installed_at: "test.md".to_string(),
        dependencies: vec![],
        resource_type: crate::core::ResourceType::Agent,

        tool: Some("claude-code".to_string()),
        manifest_alias: None,
        context_checksum: None,
        applied_patches: std::collections::BTreeMap::new(),
        install: None,
        variant_inputs: crate::resolver::lockfile_builder::VariantInputs::default(),
    };

    let entry_with_different_source = LockedResource {
        name: "test".to_string(),
        source: Some("community".to_string()),
        url: None,
        path: "test.md".to_string(),
        version: None,
        resolved_commit: None,
        checksum: "abc123".to_string(),
        installed_at: "test.md".to_string(),
        dependencies: vec![],
        resource_type: crate::core::ResourceType::Agent,

        tool: Some("claude-code".to_string()),
        manifest_alias: None,
        context_checksum: None,
        applied_patches: std::collections::BTreeMap::new(),
        install: None,
        variant_inputs: crate::resolver::lockfile_builder::VariantInputs::default(),
    };

    let entry_without_source = LockedResource {
        name: "test".to_string(),
        source: None,
        url: None,
        path: "test.md".to_string(),
        version: None,
        resolved_commit: None,
        checksum: "abc123".to_string(),
        installed_at: "test.md".to_string(),
        dependencies: vec![],
        resource_type: crate::core::ResourceType::Agent,

        tool: Some("claude-code".to_string()),
        manifest_alias: None,
        context_checksum: None,
        applied_patches: std::collections::BTreeMap::new(),
        install: None,
        variant_inputs: crate::resolver::lockfile_builder::VariantInputs::default(),
    };

    assert!(cmd.matches_lockfile_filters("test", &entry_with_source, "agent"));
    assert!(!cmd.matches_lockfile_filters("test", &entry_with_different_source, "agent"));
    assert!(!cmd.matches_lockfile_filters("test", &entry_without_source, "agent"));
    Ok(())
}

#[test]
fn test_matches_lockfile_filters_search() -> Result<()> {
    let cmd = ListCommand {
        search: Some("code".to_string()),
        ..create_default_command()
    };

    let entry = LockedResource {
        name: "test".to_string(),
        source: None,
        url: None,
        path: "test.md".to_string(),
        version: None,
        resolved_commit: None,
        checksum: "abc123".to_string(),
        installed_at: "test.md".to_string(),
        dependencies: vec![],
        resource_type: crate::core::ResourceType::Agent,

        tool: Some("claude-code".to_string()),
        manifest_alias: None,
        context_checksum: None,
        applied_patches: std::collections::BTreeMap::new(),
        install: None,
        variant_inputs: crate::resolver::lockfile_builder::VariantInputs::default(),
    };

    assert!(cmd.matches_lockfile_filters("code-reviewer", &entry, "agent"));
    assert!(cmd.matches_lockfile_filters("my-code-helper", &entry, "agent"));
    assert!(!cmd.matches_lockfile_filters("utils", &entry, "agent"));
    Ok(())
}

#[test]
fn test_sort_items() -> Result<()> {
    let cmd = ListCommand {
        sort: Some("name".to_string()),
        ..create_default_command()
    };

    let mut items = vec![
        ListItem {
            name: "zebra".to_string(),
            source: None,
            version: None,
            path: None,
            resource_type: "agent".to_string(),
            installed_at: None,
            checksum: None,
            resolved_commit: None,
            tool: Some("claude-code".to_string()),
            applied_patches: std::collections::BTreeMap::new(),
        },
        ListItem {
            name: "alpha".to_string(),
            source: None,
            version: None,
            path: None,
            resource_type: "agent".to_string(),
            installed_at: None,
            checksum: None,
            resolved_commit: None,
            tool: Some("claude-code".to_string()),
            applied_patches: std::collections::BTreeMap::new(),
        },
    ];

    cmd.sort_items(&mut items);
    assert_eq!(items[0].name, "alpha");
    assert_eq!(items[1].name, "zebra");
    Ok(())
}

#[test]
fn test_sort_items_by_version() -> Result<()> {
    let cmd = ListCommand {
        sort: Some("version".to_string()),
        ..create_default_command()
    };

    let mut items = vec![
        ListItem {
            name: "test1".to_string(),
            source: None,
            version: Some("v2.0.0".to_string()),
            path: None,
            resource_type: "agent".to_string(),
            installed_at: None,
            checksum: None,
            resolved_commit: None,
            tool: Some("claude-code".to_string()),
            applied_patches: std::collections::BTreeMap::new(),
        },
        ListItem {
            name: "test2".to_string(),
            source: None,
            version: Some("v1.0.0".to_string()),
            path: None,
            resource_type: "agent".to_string(),
            installed_at: None,
            checksum: None,
            resolved_commit: None,
            tool: Some("claude-code".to_string()),
            applied_patches: std::collections::BTreeMap::new(),
        },
    ];

    cmd.sort_items(&mut items);
    assert_eq!(items[0].version, Some("v1.0.0".to_string()));
    assert_eq!(items[1].version, Some("v2.0.0".to_string()));
    Ok(())
}

#[test]
fn test_sort_items_by_source() -> Result<()> {
    let cmd = ListCommand {
        sort: Some("source".to_string()),
        ..create_default_command()
    };

    let mut items = vec![
        ListItem {
            name: "test1".to_string(),
            source: Some("zebra".to_string()),
            version: None,
            path: None,
            resource_type: "agent".to_string(),
            installed_at: None,
            checksum: None,
            resolved_commit: None,
            tool: Some("claude-code".to_string()),
            applied_patches: std::collections::BTreeMap::new(),
        },
        ListItem {
            name: "test2".to_string(),
            source: Some("alpha".to_string()),
            version: None,
            path: None,
            resource_type: "agent".to_string(),
            installed_at: None,
            checksum: None,
            resolved_commit: None,
            tool: Some("claude-code".to_string()),
            applied_patches: std::collections::BTreeMap::new(),
        },
        ListItem {
            name: "test3".to_string(),
            source: None, // Should be treated as "local"
            version: None,
            path: None,
            resource_type: "agent".to_string(),
            installed_at: None,
            checksum: None,
            resolved_commit: None,
            tool: Some("claude-code".to_string()),
            applied_patches: std::collections::BTreeMap::new(),
        },
    ];

    cmd.sort_items(&mut items);
    assert_eq!(items[0].source, Some("alpha".to_string()));
    assert_eq!(items[1].source, None); // "local" comes before "zebra"
    assert_eq!(items[2].source, Some("zebra".to_string()));
    Ok(())
}

#[test]
fn test_sort_items_by_type() -> Result<()> {
    let cmd = ListCommand {
        sort: Some("type".to_string()),
        ..create_default_command()
    };

    let mut items = vec![
        ListItem {
            name: "test1".to_string(),
            source: None,
            version: None,
            path: None,
            resource_type: "snippet".to_string(),
            installed_at: None,
            checksum: None,
            resolved_commit: None,
            tool: Some("agpm".to_string()),
            applied_patches: std::collections::BTreeMap::new(),
        },
        ListItem {
            name: "test2".to_string(),
            source: None,
            version: None,
            path: None,
            resource_type: "agent".to_string(),
            installed_at: None,
            checksum: None,
            resolved_commit: None,
            tool: Some("claude-code".to_string()),
            applied_patches: std::collections::BTreeMap::new(),
        },
    ];

    cmd.sort_items(&mut items);
    assert_eq!(items[0].resource_type, "agent");
    assert_eq!(items[1].resource_type, "snippet");
    Ok(())
}

#[test]
fn test_lockentry_to_listitem() -> Result<()> {
    let _cmd = create_default_command();

    let lock_entry = LockedResource {
        name: "test-agent".to_string(),
        source: Some("official".to_string()),
        url: Some("https://example.com/repo.git".to_string()),
        path: "agents/test.md".to_string(),
        version: Some("v1.0.0".to_string()),
        resolved_commit: Some("abc123".to_string()),
        checksum: "sha256:def456".to_string(),
        installed_at: "agents/test-agent.md".to_string(),
        dependencies: vec![],
        resource_type: crate::core::ResourceType::Agent,

        tool: Some("claude-code".to_string()),
        manifest_alias: None,
        context_checksum: None,
        applied_patches: std::collections::BTreeMap::new(),
        install: None,
        variant_inputs: crate::resolver::lockfile_builder::VariantInputs::default(),
    };

    let list_item = converters::lockentry_to_listitem(&lock_entry, "agent");

    assert_eq!(list_item.name, "test-agent");
    assert_eq!(list_item.source, Some("official".to_string()));
    assert_eq!(list_item.version, Some("v1.0.0".to_string()));
    assert_eq!(list_item.path, Some("agents/test.md".to_string()));
    assert_eq!(list_item.resource_type, "agent");
    assert_eq!(list_item.installed_at, Some("agents/test-agent.md".to_string()));
    assert_eq!(list_item.checksum, Some("sha256:def456".to_string()));
    assert_eq!(list_item.resolved_commit, Some("abc123".to_string()));
    Ok(())
}

#[tokio::test]
async fn test_list_with_json_format() -> Result<()> {
    let temp = TempDir::new().unwrap();
    let manifest_path = temp.path().join("agpm.toml");

    // Create manifest with resources
    let manifest = create_test_manifest();
    manifest.save(&manifest_path).unwrap();

    let cmd = ListCommand {
        format: "json".to_string(),
        manifest: true,
        ..create_default_command()
    };

    let result = cmd.execute_from_path(manifest_path).await;
    result?;
    Ok(())
}

#[tokio::test]
async fn test_list_with_yaml_format() -> Result<()> {
    let temp = TempDir::new().unwrap();
    let manifest_path = temp.path().join("agpm.toml");

    // Create manifest with resources
    let manifest = create_test_manifest();
    manifest.save(&manifest_path).unwrap();

    let cmd = ListCommand {
        format: "yaml".to_string(),
        manifest: true,
        ..create_default_command()
    };

    let result = cmd.execute_from_path(manifest_path).await;
    result?;
    Ok(())
}

#[tokio::test]
async fn test_list_with_compact_format() -> Result<()> {
    let temp = TempDir::new().unwrap();
    let manifest_path = temp.path().join("agpm.toml");

    // Create manifest with resources
    let manifest = create_test_manifest();
    manifest.save(&manifest_path).unwrap();

    let cmd = ListCommand {
        format: "compact".to_string(),
        manifest: true,
        ..create_default_command()
    };

    let result = cmd.execute_from_path(manifest_path).await;
    result?;
    Ok(())
}

#[tokio::test]
async fn test_list_with_simple_format() -> Result<()> {
    let temp = TempDir::new().unwrap();
    let manifest_path = temp.path().join("agpm.toml");

    // Create manifest with resources
    let manifest = create_test_manifest();
    manifest.save(&manifest_path).unwrap();

    let cmd = ListCommand {
        format: "simple".to_string(),
        manifest: true,
        ..create_default_command()
    };

    let result = cmd.execute_from_path(manifest_path).await;
    result?;
    Ok(())
}

#[tokio::test]
async fn test_list_filter_by_agents_only() -> Result<()> {
    let temp = TempDir::new().unwrap();
    let manifest_path = temp.path().join("agpm.toml");

    // Create manifest with both agents and snippets
    let manifest = create_test_manifest();
    manifest.save(&manifest_path).unwrap();

    let cmd = ListCommand {
        agents: true,
        manifest: true,
        ..create_default_command()
    };

    let result = cmd.execute_from_path(manifest_path).await;
    result?;
    Ok(())
}

#[tokio::test]
async fn test_list_filter_by_snippets_only() -> Result<()> {
    let temp = TempDir::new().unwrap();
    let manifest_path = temp.path().join("agpm.toml");

    // Create manifest with both agents and snippets
    let manifest = create_test_manifest();
    manifest.save(&manifest_path).unwrap();

    let cmd = ListCommand {
        snippets: true,
        manifest: true,
        ..create_default_command()
    };

    let result = cmd.execute_from_path(manifest_path).await;
    result?;
    Ok(())
}

#[tokio::test]
async fn test_list_filter_by_type() -> Result<()> {
    let temp = TempDir::new().unwrap();
    let manifest_path = temp.path().join("agpm.toml");

    // Create manifest with both agents and snippets
    let manifest = create_test_manifest();
    manifest.save(&manifest_path).unwrap();

    // Test filtering by agents
    let cmd = ListCommand {
        r#type: Some("agents".to_string()),
        manifest: true,
        ..create_default_command()
    };

    let result = cmd.execute_from_path(manifest_path.clone()).await;
    result?;

    // Test filtering by snippets
    let cmd = ListCommand {
        r#type: Some("snippets".to_string()),
        manifest: true,
        ..create_default_command()
    };

    let result = cmd.execute_from_path(manifest_path).await;
    result?;
    Ok(())
}

#[tokio::test]
async fn test_list_filter_by_source() -> Result<()> {
    let temp = TempDir::new().unwrap();
    let manifest_path = temp.path().join("agpm.toml");

    // Create manifest with resources from different sources
    let manifest = create_test_manifest();
    manifest.save(&manifest_path).unwrap();

    let cmd = ListCommand {
        source: Some("official".to_string()),
        manifest: true,
        ..create_default_command()
    };

    let result = cmd.execute_from_path(manifest_path).await;
    result?;
    Ok(())
}

#[tokio::test]
async fn test_list_search_by_pattern() -> Result<()> {
    let temp = TempDir::new().unwrap();
    let manifest_path = temp.path().join("agpm.toml");

    // Create manifest with resources
    let manifest = create_test_manifest();
    manifest.save(&manifest_path).unwrap();

    let cmd = ListCommand {
        search: Some("code".to_string()),
        manifest: true,
        ..create_default_command()
    };

    let result = cmd.execute_from_path(manifest_path).await;
    result?;
    Ok(())
}

#[tokio::test]
async fn test_list_with_detailed_flag() -> Result<()> {
    let temp = TempDir::new().unwrap();
    let manifest_path = temp.path().join("agpm.toml");

    // Create manifest with resources
    let manifest = create_test_manifest();
    manifest.save(&manifest_path).unwrap();

    let cmd = ListCommand {
        detailed: true,
        manifest: true,
        ..create_default_command()
    };

    let result = cmd.execute_from_path(manifest_path).await;
    result?;
    Ok(())
}

#[tokio::test]
async fn test_list_with_files_flag() -> Result<()> {
    let temp = TempDir::new().unwrap();
    let manifest_path = temp.path().join("agpm.toml");

    // Create manifest with resources
    let manifest = create_test_manifest();
    manifest.save(&manifest_path).unwrap();

    let cmd = ListCommand {
        files: true,
        manifest: true,
        ..create_default_command()
    };

    let result = cmd.execute_from_path(manifest_path).await;
    result?;
    Ok(())
}

#[tokio::test]
async fn test_list_with_verbose_flag() -> Result<()> {
    let temp = TempDir::new().unwrap();
    let manifest_path = temp.path().join("agpm.toml");

    // Create manifest with resources
    let manifest = create_test_manifest();
    manifest.save(&manifest_path).unwrap();

    let cmd = ListCommand {
        verbose: true,
        manifest: true,
        ..create_default_command()
    };

    let result = cmd.execute_from_path(manifest_path).await;
    result?;
    Ok(())
}

#[tokio::test]
async fn test_list_with_sort() -> Result<()> {
    let temp = TempDir::new().unwrap();
    let manifest_path = temp.path().join("agpm.toml");

    // Create manifest with resources
    let manifest = create_test_manifest();
    manifest.save(&manifest_path).unwrap();

    let cmd = ListCommand {
        sort: Some("name".to_string()),
        manifest: true,
        ..create_default_command()
    };

    let result = cmd.execute_from_path(manifest_path).await;
    result?;
    Ok(())
}

#[tokio::test]
async fn test_list_empty_lockfile_json_output() -> Result<()> {
    let temp = TempDir::new().unwrap();
    let manifest_path = temp.path().join("agpm.toml");

    // Create manifest but no lockfile
    let manifest = create_test_manifest();
    manifest.save(&manifest_path).unwrap();

    let cmd = ListCommand {
        format: "json".to_string(),
        manifest: false, // Use lockfile mode
        ..create_default_command()
    };

    let result = cmd.execute_from_path(manifest_path).await;
    result?;
    Ok(())
}
