use crate::mcp::models::{AgpmMetadata, ClaudeSettings, McpServerConfig};
use std::collections::HashMap;
use std::fs;
use tempfile::tempdir;

use super::setup_project_root;

#[test]
fn test_claude_settings_load_save() {
    let temp = tempdir().unwrap();
    let settings_path = temp.path().join("settings.local.json");

    let mut settings = ClaudeSettings::default();
    let mut servers = HashMap::new();
    servers.insert(
        "test-server".to_string(),
        McpServerConfig {
            command: Some("node".to_string()),
            args: vec!["server.js".to_string()],
            env: None,
            r#type: None,
            url: None,
            headers: None,
            agpm_metadata: None,
        },
    );
    settings.mcp_servers = Some(servers);

    settings.save(&settings_path).unwrap();

    let loaded = ClaudeSettings::load_or_default(&settings_path).unwrap();
    assert!(loaded.mcp_servers.is_some());
    let servers = loaded.mcp_servers.unwrap();
    assert_eq!(servers.len(), 1);
    assert!(servers.contains_key("test-server"));
}

#[test]
fn test_claude_settings_load_nonexistent_file() {
    let temp = tempdir().unwrap();
    let settings_path = temp.path().join("nonexistent.json");

    let settings = ClaudeSettings::load_or_default(&settings_path).unwrap();
    assert!(settings.mcp_servers.is_none());
    assert!(settings.permissions.is_none());
    assert!(settings.other.is_empty());
}

#[test]
fn test_claude_settings_load_invalid_json() {
    let temp = tempdir().unwrap();
    let settings_path = temp.path().join("invalid.json");
    fs::write(&settings_path, "invalid json {").unwrap();

    let result = ClaudeSettings::load_or_default(&settings_path);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("Failed to parse"));
}

#[test]
fn test_claude_settings_save_creates_backup() {
    let temp = tempdir().unwrap();
    setup_project_root(temp.path());

    let settings_path = temp.path().join("settings.local.json");
    let backup_path =
        temp.path().join(".agpm").join("backups").join("claude-code").join("settings.local.json");

    // Create initial file
    fs::write(&settings_path, r#"{"test": "value"}"#).unwrap();

    let settings = ClaudeSettings::default();
    settings.save(&settings_path).unwrap();

    // Backup should be created in .agpm/backups/claude-code directory
    assert!(backup_path.exists());
    let backup_content = fs::read_to_string(backup_path).unwrap();
    assert_eq!(backup_content, r#"{"test": "value"}"#);
}

#[test]
fn test_claude_settings_update_mcp_servers_empty_dir() {
    let temp = tempdir().unwrap();
    let nonexistent_dir = temp.path().join("nonexistent");

    let mut settings = ClaudeSettings::default();
    // Should not error on nonexistent directory
    settings.update_mcp_servers(&nonexistent_dir).unwrap();
}

#[test]
fn test_update_mcp_servers_from_directory() {
    let temp = tempdir().unwrap();
    let mcp_servers_dir = temp.path().join("mcp-servers");
    std::fs::create_dir(&mcp_servers_dir).unwrap();

    // Create a server config file
    let server_config = McpServerConfig {
        command: Some("managed".to_string()),
        args: vec![],
        env: None,
        r#type: None,
        url: None,
        headers: None,
        agpm_metadata: Some(AgpmMetadata {
            managed: true,
            source: Some("test".to_string()),
            version: Some("v1.0.0".to_string()),
            installed_at: "2024-01-01T00:00:00Z".to_string(),
            dependency_name: Some("agpm-server".to_string()),
        }),
    };
    let config_path = mcp_servers_dir.join("agpm-server.json");
    let json = serde_json::to_string_pretty(&server_config).unwrap();
    std::fs::write(&config_path, json).unwrap();

    // Add a user-managed server to settings
    let mut settings = ClaudeSettings::default();
    let mut servers = HashMap::new();
    servers.insert(
        "user-server".to_string(),
        McpServerConfig {
            command: Some("custom".to_string()),
            args: vec![],
            env: None,
            r#type: None,
            url: None,
            headers: None,
            agpm_metadata: None,
        },
    );
    settings.mcp_servers = Some(servers);

    // Update from directory
    settings.update_mcp_servers(&mcp_servers_dir).unwrap();

    // Both servers should be present
    assert!(settings.mcp_servers.is_some());
    let servers = settings.mcp_servers.as_ref().unwrap();
    assert!(servers.contains_key("user-server"));
    assert!(servers.contains_key("agpm-server"));
    assert_eq!(servers.len(), 2);
}

#[test]
fn test_update_mcp_servers_replaces_old_managed() {
    let temp = tempdir().unwrap();
    let mcp_servers_dir = temp.path().join("mcp-servers");
    std::fs::create_dir(&mcp_servers_dir).unwrap();

    // Start with existing managed and user servers
    let mut settings = ClaudeSettings::default();
    let mut servers = HashMap::new();

    // User-managed server (should be preserved)
    servers.insert(
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

    // Old AGPM-managed server (should be removed)
    servers.insert(
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
                source: Some("old-source".to_string()),
                version: Some("v0.1.0".to_string()),
                installed_at: "2023-01-01T00:00:00Z".to_string(),
                dependency_name: Some("old-managed".to_string()),
            }),
        },
    );

    settings.mcp_servers = Some(servers);

    // Create new managed server config file
    let server_config = McpServerConfig {
        command: Some("new-managed".to_string()),
        args: vec![],
        env: None,
        r#type: None,
        url: None,
        headers: None,
        agpm_metadata: Some(AgpmMetadata {
            managed: true,
            source: Some("new-source".to_string()),
            version: Some("v1.0.0".to_string()),
            installed_at: "2024-01-01T00:00:00Z".to_string(),
            dependency_name: Some("new-managed".to_string()),
        }),
    };
    let config_path = mcp_servers_dir.join("new-managed.json");
    let json = serde_json::to_string_pretty(&server_config).unwrap();
    std::fs::write(&config_path, json).unwrap();

    // Update from directory
    settings.update_mcp_servers(&mcp_servers_dir).unwrap();

    let servers = settings.mcp_servers.as_ref().unwrap();
    assert!(servers.contains_key("user-server")); // User server preserved
    assert!(servers.contains_key("new-managed")); // New managed server added
    assert!(!servers.contains_key("old-managed")); // Old managed server removed
    assert_eq!(servers.len(), 2);
}

#[test]
fn test_update_mcp_servers_invalid_json_file() {
    let temp = tempdir().unwrap();
    let mcp_servers_dir = temp.path().join("mcp-servers");
    std::fs::create_dir(&mcp_servers_dir).unwrap();

    // Create invalid JSON file
    let invalid_path = mcp_servers_dir.join("invalid.json");
    std::fs::write(&invalid_path, "invalid json {").unwrap();

    let mut settings = ClaudeSettings::default();
    let result = settings.update_mcp_servers(&mcp_servers_dir);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("Failed to parse"));
}

#[test]
fn test_update_mcp_servers_ignores_non_json_files() {
    let temp = tempdir().unwrap();
    let mcp_servers_dir = temp.path().join("mcp-servers");
    std::fs::create_dir(&mcp_servers_dir).unwrap();

    // Create non-JSON file
    let txt_path = mcp_servers_dir.join("readme.txt");
    std::fs::write(&txt_path, "This is not a JSON file").unwrap();

    // Create valid JSON file
    let server_config = McpServerConfig {
        command: Some("test".to_string()),
        args: vec![],
        env: None,
        r#type: None,
        url: None,
        headers: None,
        agpm_metadata: None,
    };
    let json_path = mcp_servers_dir.join("valid.json");
    let json = serde_json::to_string_pretty(&server_config).unwrap();
    std::fs::write(&json_path, json).unwrap();

    let mut settings = ClaudeSettings::default();
    settings.update_mcp_servers(&mcp_servers_dir).unwrap();

    let servers = settings.mcp_servers.as_ref().unwrap();
    assert!(servers.contains_key("valid"));
    assert_eq!(servers.len(), 1);
}

#[test]
fn test_settings_preserves_other_fields() {
    let temp = tempdir().unwrap();
    setup_project_root(temp.path());

    let settings_path = temp.path().join("settings.local.json");

    // Create a settings file with various fields
    let json = r#"{
        "permissions": {
            "allow": ["Bash(ls)"],
            "deny": []
        },
        "customField": "value",
        "mcpServers": {
            "test": {
                "command": "node",
                "args": []
            }
        }
    }"#;
    std::fs::write(&settings_path, json).unwrap();

    // Load and save
    let settings = ClaudeSettings::load_or_default(&settings_path).unwrap();
    assert!(settings.permissions.is_some());
    assert!(settings.mcp_servers.is_some());
    assert!(settings.other.contains_key("customField"));

    settings.save(&settings_path).unwrap();

    // Reload and verify all fields preserved
    let reloaded = ClaudeSettings::load_or_default(&settings_path).unwrap();
    assert!(reloaded.permissions.is_some());
    assert!(reloaded.mcp_servers.is_some());
    assert!(reloaded.other.contains_key("customField"));
}

#[test]
fn test_claude_settings_save_backup() {
    let temp = tempfile::TempDir::new().unwrap();
    setup_project_root(temp.path());

    let settings_path = temp.path().join("settings.local.json");
    let backup_path =
        temp.path().join(".agpm").join("backups").join("claude-code").join("settings.local.json");

    // Create initial settings
    let settings1 = ClaudeSettings::default();
    settings1.save(&settings_path).unwrap();
    assert!(settings_path.exists());
    assert!(!backup_path.exists());

    // Save again to trigger backup
    let settings2 = ClaudeSettings {
        hooks: Some(serde_json::json!({"test": "hook"})),
        ..Default::default()
    };
    settings2.save(&settings_path).unwrap();

    // Verify backup was created in agpm directory
    assert!(backup_path.exists());

    // Verify backup contains original content
    let backup_content: ClaudeSettings = crate::utils::read_json_file(&backup_path).unwrap();
    assert!(backup_content.hooks.is_none());

    // Verify main file has new content
    let main_content: ClaudeSettings = crate::utils::read_json_file(&settings_path).unwrap();
    assert!(main_content.hooks.is_some());
}

#[test]
fn test_backup_fails_without_project_root() {
    // Test that backup creation fails gracefully when no agpm.toml exists
    let temp = tempfile::TempDir::new().unwrap();
    // Deliberately NOT calling setup_project_root here

    let settings_path = temp.path().join("settings.local.json");

    // Create initial file
    fs::write(&settings_path, r#"{"test": "value"}"#).unwrap();

    let settings = ClaudeSettings::default();
    let result = settings.save(&settings_path);

    // Should fail with helpful error message about missing project root
    assert!(result.is_err());
    let error_msg = result.unwrap_err().to_string();
    assert!(error_msg.contains("Failed to find project root") || error_msg.contains("agpm.toml"));
}

#[test]
fn test_update_mcp_servers_preserves_user_servers() {
    let temp = tempfile::TempDir::new().unwrap();
    let agpm_dir = temp.path().join(".claude").join("agpm");
    let mcp_servers_dir = agpm_dir.join("mcp-servers");
    std::fs::create_dir_all(&mcp_servers_dir).unwrap();

    // Create server config files
    let server1 = McpServerConfig {
        command: Some("server1".to_string()),
        args: vec!["arg1".to_string()],
        env: None,
        r#type: None,
        url: None,
        headers: None,
        agpm_metadata: Some(AgpmMetadata {
            managed: true,
            source: Some("source1".to_string()),
            version: Some("v1.0.0".to_string()),
            installed_at: "2024-01-01T00:00:00Z".to_string(),
            dependency_name: None,
        }),
    };
    crate::utils::write_json_file(&mcp_servers_dir.join("server1.json"), &server1, true).unwrap();

    // Create settings with existing user server
    let mut settings = ClaudeSettings::default();
    let mut servers = HashMap::new();
    servers.insert(
        "user-server".to_string(),
        McpServerConfig {
            command: Some("user".to_string()),
            args: vec![],
            env: None,
            r#type: None,
            url: None,
            headers: None,
            agpm_metadata: None,
        },
    );
    settings.mcp_servers = Some(servers);

    // Update from directory
    settings.update_mcp_servers(&mcp_servers_dir).unwrap();

    // Verify both servers are present
    let servers = settings.mcp_servers.as_ref().unwrap();
    assert_eq!(servers.len(), 2);
    assert!(servers.contains_key("user-server"));
    assert!(servers.contains_key("server1"));

    // Verify server1 config matches
    let server1_config = servers.get("server1").unwrap();
    assert_eq!(server1_config.command, Some("server1".to_string()));
    assert_eq!(server1_config.args, vec!["arg1"]);
}

#[test]
fn test_update_mcp_servers_nonexistent_dir() {
    let temp = tempfile::TempDir::new().unwrap();
    let nonexistent_dir = temp.path().join("nonexistent");

    let mut settings = ClaudeSettings::default();
    let result = settings.update_mcp_servers(&nonexistent_dir);
    assert!(result.is_ok());
}
