use crate::mcp::models::{AgpmMetadata, McpConfig, McpServerConfig};
use serde_json::json;
use std::collections::HashMap;
use std::fs;
use tempfile::tempdir;

use super::setup_project_root;

#[test]
fn test_mcp_config_load_save() {
    let temp = tempdir().unwrap();
    let config_path = temp.path().join("mcp.json");

    let mut config = McpConfig::default();
    config.mcp_servers.insert(
        "test-server".to_string(),
        McpServerConfig {
            command: Some("node".to_string()),
            args: vec!["server.js".to_string()],
            env: Some({
                let mut env = HashMap::new();
                env.insert("NODE_ENV".to_string(), json!("production"));
                env
            }),
            r#type: None,
            url: None,
            headers: None,
            agpm_metadata: None,
        },
    );

    config.save(&config_path).unwrap();

    let loaded = McpConfig::load_or_default(&config_path).unwrap();
    assert!(loaded.mcp_servers.contains_key("test-server"));
    let server = &loaded.mcp_servers["test-server"];
    assert_eq!(server.command, Some("node".to_string()));
    assert_eq!(server.args, vec!["server.js"]);
    assert!(server.env.is_some());
}

#[test]
fn test_mcp_config_load_nonexistent() {
    let temp = tempdir().unwrap();
    let config_path = temp.path().join("nonexistent.json");

    let config = McpConfig::load_or_default(&config_path).unwrap();
    assert!(config.mcp_servers.is_empty());
}

#[test]
fn test_mcp_config_load_invalid_json() {
    let temp = tempdir().unwrap();
    let config_path = temp.path().join("invalid.json");
    fs::write(&config_path, "invalid json {").unwrap();

    let result = McpConfig::load_or_default(&config_path);
    assert!(result.is_err());
}

#[test]
fn test_mcp_config_save_creates_backup() {
    let temp = tempdir().unwrap();
    setup_project_root(temp.path());

    let config_path = temp.path().join("mcp.json");
    let backup_path =
        temp.path().join(".agpm").join("backups").join("claude-code").join("mcp.json");

    // Create initial file
    fs::write(&config_path, r#"{"mcpServers": {"old": {"command": "old"}}}"#).unwrap();

    let config = McpConfig::default();
    config.save(&config_path).unwrap();

    // Backup should be created in .agpm/backups/claude-code directory
    assert!(backup_path.exists());
    let backup_content = fs::read_to_string(backup_path).unwrap();
    assert!(backup_content.contains("old"));
}

#[test]
fn test_mcp_config_update_managed_servers() {
    let mut config = McpConfig::default();

    // Add user-managed server
    config.mcp_servers.insert(
        "user-server".to_string(),
        McpServerConfig {
            command: Some("user-command".to_string()),
            args: vec![],
            env: None,
            r#type: None,
            url: None,
            headers: None,
            agpm_metadata: None,
        },
    );

    // Add old AGPM-managed server
    config.mcp_servers.insert(
        "old-managed".to_string(),
        McpServerConfig {
            command: Some("old-command".to_string()),
            args: vec![],
            env: None,
            r#type: None,
            url: None,
            headers: None,
            agpm_metadata: Some(AgpmMetadata {
                managed: true,
                source: None,
                version: None,
                installed_at: "old-time".to_string(),
                dependency_name: None,
            }),
        },
    );

    // Update with new managed servers
    let mut updates = HashMap::new();
    updates.insert(
        "new-managed".to_string(),
        McpServerConfig {
            command: Some("new-command".to_string()),
            args: vec![],
            env: None,
            r#type: None,
            url: None,
            headers: None,
            agpm_metadata: Some(AgpmMetadata {
                managed: true,
                source: None,
                version: None,
                installed_at: "new-time".to_string(),
                dependency_name: None,
            }),
        },
    );

    config.update_managed_servers(updates).unwrap();

    // User server should be preserved, old managed should be removed, new managed added
    assert!(config.mcp_servers.contains_key("user-server"));
    assert!(config.mcp_servers.contains_key("new-managed"));
    assert!(!config.mcp_servers.contains_key("old-managed"));
    assert_eq!(config.mcp_servers.len(), 2);
}

#[test]
fn test_mcp_config_update_managed_servers_preserves_updating_servers() {
    let mut config = McpConfig::default();

    // Add AGPM-managed server that will be updated
    config.mcp_servers.insert(
        "updating-server".to_string(),
        McpServerConfig {
            command: Some("old-command".to_string()),
            args: vec![],
            env: None,
            r#type: None,
            url: None,
            headers: None,
            agpm_metadata: Some(AgpmMetadata {
                managed: true,
                source: None,
                version: Some("v1.0.0".to_string()),
                installed_at: "old-time".to_string(),
                dependency_name: None,
            }),
        },
    );

    // Update with new version of the same server
    let mut updates = HashMap::new();
    updates.insert(
        "updating-server".to_string(),
        McpServerConfig {
            command: Some("new-command".to_string()),
            args: vec![],
            env: None,
            r#type: None,
            url: None,
            headers: None,
            agpm_metadata: Some(AgpmMetadata {
                managed: true,
                source: None,
                version: Some("v2.0.0".to_string()),
                installed_at: "new-time".to_string(),
                dependency_name: None,
            }),
        },
    );

    config.update_managed_servers(updates).unwrap();

    assert!(config.mcp_servers.contains_key("updating-server"));
    let server = &config.mcp_servers["updating-server"];
    assert_eq!(server.command, Some("new-command".to_string()));
    assert_eq!(server.agpm_metadata.as_ref().unwrap().version, Some("v2.0.0".to_string()));
}

#[test]
fn test_mcp_config_check_conflicts() {
    let mut config = McpConfig::default();

    // Add user-managed server
    config.mcp_servers.insert(
        "user-server".to_string(),
        McpServerConfig {
            command: Some("user-command".to_string()),
            args: vec![],
            env: None,
            r#type: None,
            url: None,
            headers: None,
            agpm_metadata: None,
        },
    );

    // Add AGPM-managed server
    config.mcp_servers.insert(
        "managed-server".to_string(),
        McpServerConfig {
            command: Some("managed-command".to_string()),
            args: vec![],
            env: None,
            r#type: None,
            url: None,
            headers: None,
            agpm_metadata: Some(AgpmMetadata {
                managed: true,
                source: None,
                version: None,
                installed_at: "time".to_string(),
                dependency_name: None,
            }),
        },
    );

    let mut new_servers = HashMap::new();
    new_servers.insert(
        "user-server".to_string(), // This conflicts
        McpServerConfig {
            command: Some("new-command".to_string()),
            args: vec![],
            env: None,
            r#type: None,
            url: None,
            headers: None,
            agpm_metadata: Some(AgpmMetadata {
                managed: true,
                source: None,
                version: None,
                installed_at: "time".to_string(),
                dependency_name: None,
            }),
        },
    );
    new_servers.insert(
        "managed-server".to_string(), // This doesn't conflict (already managed)
        McpServerConfig {
            command: Some("updated-command".to_string()),
            args: vec![],
            env: None,
            r#type: None,
            url: None,
            headers: None,
            agpm_metadata: Some(AgpmMetadata {
                managed: true,
                source: None,
                version: None,
                installed_at: "time".to_string(),
                dependency_name: None,
            }),
        },
    );
    new_servers.insert(
        "new-server".to_string(), // This doesn't conflict (new)
        McpServerConfig {
            command: Some("new-command".to_string()),
            args: vec![],
            env: None,
            r#type: None,
            url: None,
            headers: None,
            agpm_metadata: Some(AgpmMetadata {
                managed: true,
                source: None,
                version: None,
                installed_at: "time".to_string(),
                dependency_name: None,
            }),
        },
    );

    let conflicts = config.check_conflicts(&new_servers);
    assert_eq!(conflicts, vec!["user-server"]);
}

#[test]
fn test_mcp_config_check_conflicts_unmanaged_metadata() {
    let mut config = McpConfig::default();

    // Add server with metadata but managed=false
    config.mcp_servers.insert(
        "unmanaged-server".to_string(),
        McpServerConfig {
            command: Some("user-command".to_string()),
            args: vec![],
            env: None,
            r#type: None,
            url: None,
            headers: None,
            agpm_metadata: Some(AgpmMetadata {
                managed: false,
                source: None,
                version: None,
                installed_at: "time".to_string(),
                dependency_name: None,
            }),
        },
    );

    let mut new_servers = HashMap::new();
    new_servers.insert(
        "unmanaged-server".to_string(),
        McpServerConfig {
            command: Some("new-command".to_string()),
            args: vec![],
            env: None,
            r#type: None,
            url: None,
            headers: None,
            agpm_metadata: Some(AgpmMetadata {
                managed: true,
                source: None,
                version: None,
                installed_at: "time".to_string(),
                dependency_name: None,
            }),
        },
    );

    let conflicts = config.check_conflicts(&new_servers);
    assert_eq!(conflicts, vec!["unmanaged-server"]);
}

#[test]
fn test_mcp_config_remove_all_managed() {
    let mut config = McpConfig::default();

    // Add mixed servers
    config.mcp_servers.insert(
        "user-server".to_string(),
        McpServerConfig {
            command: Some("user-command".to_string()),
            args: vec![],
            env: None,
            r#type: None,
            url: None,
            headers: None,
            agpm_metadata: None,
        },
    );

    config.mcp_servers.insert(
        "managed-server".to_string(),
        McpServerConfig {
            command: Some("managed-command".to_string()),
            args: vec![],
            env: None,
            r#type: None,
            url: None,
            headers: None,
            agpm_metadata: Some(AgpmMetadata {
                managed: true,
                source: None,
                version: None,
                installed_at: "time".to_string(),
                dependency_name: None,
            }),
        },
    );

    config.mcp_servers.insert(
        "unmanaged-with-metadata".to_string(),
        McpServerConfig {
            command: Some("unmanaged-command".to_string()),
            args: vec![],
            env: None,
            r#type: None,
            url: None,
            headers: None,
            agpm_metadata: Some(AgpmMetadata {
                managed: false,
                source: None,
                version: None,
                installed_at: "time".to_string(),
                dependency_name: None,
            }),
        },
    );

    config.remove_all_managed();

    assert!(config.mcp_servers.contains_key("user-server"));
    assert!(config.mcp_servers.contains_key("unmanaged-with-metadata"));
    assert!(!config.mcp_servers.contains_key("managed-server"));
    assert_eq!(config.mcp_servers.len(), 2);
}

#[test]
fn test_mcp_config_get_managed_servers() {
    let mut config = McpConfig::default();

    // Add mixed servers
    config.mcp_servers.insert(
        "user-server".to_string(),
        McpServerConfig {
            command: Some("user-command".to_string()),
            args: vec![],
            env: None,
            r#type: None,
            url: None,
            headers: None,
            agpm_metadata: None,
        },
    );

    config.mcp_servers.insert(
        "managed-server1".to_string(),
        McpServerConfig {
            command: Some("managed-command1".to_string()),
            args: vec![],
            env: None,
            r#type: None,
            url: None,
            headers: None,
            agpm_metadata: Some(AgpmMetadata {
                managed: true,
                source: None,
                version: None,
                installed_at: "time".to_string(),
                dependency_name: None,
            }),
        },
    );

    config.mcp_servers.insert(
        "managed-server2".to_string(),
        McpServerConfig {
            command: Some("managed-command2".to_string()),
            args: vec![],
            env: None,
            r#type: None,
            url: None,
            headers: None,
            agpm_metadata: Some(AgpmMetadata {
                managed: true,
                source: Some("source".to_string()),
                version: Some("v1.0.0".to_string()),
                installed_at: "time".to_string(),
                dependency_name: Some("dep".to_string()),
            }),
        },
    );

    let managed = config.get_managed_servers();
    assert_eq!(managed.len(), 2);
    assert!(managed.contains_key("managed-server1"));
    assert!(managed.contains_key("managed-server2"));
    assert!(!managed.contains_key("user-server"));
}

#[test]
fn test_mcp_config_save_backup() {
    let temp = tempfile::TempDir::new().unwrap();
    setup_project_root(temp.path());

    let config_path = temp.path().join(".mcp.json");
    let backup_path =
        temp.path().join(".agpm").join("backups").join("claude-code").join(".mcp.json");

    // Create initial config
    let config1 = McpConfig::default();
    config1.save(&config_path).unwrap();
    assert!(config_path.exists());
    assert!(!backup_path.exists());

    // Save again with changes to trigger backup
    let mut config2 = McpConfig::default();
    config2.mcp_servers.insert(
        "test".to_string(),
        McpServerConfig {
            command: Some("test-cmd".to_string()),
            args: vec![],
            env: None,
            r#type: None,
            url: None,
            headers: None,
            agpm_metadata: None,
        },
    );
    config2.save(&config_path).unwrap();

    // Verify backup was created in .agpm/backups directory
    assert!(backup_path.exists());

    // Verify backup contains original content
    let backup_content: McpConfig = crate::utils::read_json_file(&backup_path).unwrap();
    assert!(backup_content.mcp_servers.is_empty());

    // Verify main file has new content
    let main_content: McpConfig = crate::utils::read_json_file(&config_path).unwrap();
    assert_eq!(main_content.mcp_servers.len(), 1);
}

#[test]
fn test_mcp_config_handles_extra_fields() {
    // McpConfig doesn't preserve other fields, but it should parse files with extra fields
    let json_str = r#"{
        "mcpServers": {
            "test": {
                "command": "test",
                "args": []
            }
        },
        "customField": "value",
        "anotherField": {
            "nested": true
        }
    }"#;

    let temp = tempdir().unwrap();
    let config_path = temp.path().join(".mcp.json");
    std::fs::write(&config_path, json_str).unwrap();

    // Should parse successfully ignoring extra fields
    let config = McpConfig::load_or_default(&config_path).unwrap();
    assert!(config.mcp_servers.contains_key("test"));
    assert_eq!(config.mcp_servers.len(), 1);
}
