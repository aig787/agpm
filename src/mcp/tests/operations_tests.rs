use crate::mcp::models::{AgpmMetadata, ClaudeSettings, McpConfig, McpServerConfig};
use crate::mcp::operations::{clean_mcp_servers, list_mcp_servers, merge_mcp_servers};
use anyhow::Result;
use serde_json::json;
use std::collections::HashMap;
use tempfile::tempdir;

use super::setup_project_root;

#[test]
fn test_clean_mcp_servers() {
    let temp = tempfile::TempDir::new().unwrap();
    setup_project_root(temp.path());

    let project_root = temp.path();
    let claude_dir = project_root.join(".claude");
    let agpm_dir = claude_dir.join("agpm");
    let mcp_servers_dir = agpm_dir.join("mcp-servers");
    let settings_path = claude_dir.join("settings.local.json");
    let mcp_config_path = project_root.join(".mcp.json");

    // Create directory structure
    std::fs::create_dir_all(&mcp_servers_dir).unwrap();

    // Create MCP server files
    let server1_path = mcp_servers_dir.join("server1.json");
    let server2_path = mcp_servers_dir.join("server2.json");
    let server_config = McpServerConfig {
        command: Some("test".to_string()),
        args: vec![],
        env: None,
        r#type: None,
        url: None,
        headers: None,
        agpm_metadata: Some(AgpmMetadata {
            managed: true,
            source: Some("test-source".to_string()),
            version: Some("v1.0.0".to_string()),
            installed_at: "2024-01-01T00:00:00Z".to_string(),
            dependency_name: Some("test-server".to_string()),
        }),
    };
    crate::utils::write_json_file(&server1_path, &server_config, true).unwrap();
    crate::utils::write_json_file(&server2_path, &server_config, true).unwrap();

    // Create settings with both AGPM-managed and user-managed servers
    let mut settings = ClaudeSettings::default();
    let mut servers = HashMap::new();

    // AGPM-managed server
    servers.insert(
        "agpm-server".to_string(),
        McpServerConfig {
            command: Some("agpm-cmd".to_string()),
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
                dependency_name: None,
            }),
        },
    );

    // User-managed server
    servers.insert(
        "user-server".to_string(),
        McpServerConfig {
            command: Some("user-cmd".to_string()),
            args: vec![],
            env: None,
            r#type: None,
            url: None,
            headers: None,
            agpm_metadata: None,
        },
    );

    settings.mcp_servers = Some(servers);
    settings.save(&settings_path).unwrap();

    // Create .mcp.json file with the same servers
    let mut mcp_config = McpConfig::default();
    mcp_config.mcp_servers.insert(
        "agpm-server".to_string(),
        McpServerConfig {
            command: Some("agpm-cmd".to_string()),
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
                dependency_name: None,
            }),
        },
    );
    mcp_config.mcp_servers.insert(
        "user-server".to_string(),
        McpServerConfig {
            command: Some("user-cmd".to_string()),
            args: vec![],
            env: None,
            r#type: None,
            url: None,
            headers: None,
            agpm_metadata: None,
        },
    );
    mcp_config.save(&mcp_config_path).unwrap();

    // Run clean_mcp_servers
    clean_mcp_servers(project_root).unwrap();

    // Verify MCP server files are deleted
    assert!(!server1_path.exists());
    assert!(!server2_path.exists());

    // Verify .mcp.json only contains user-managed servers
    let updated_mcp_config = McpConfig::load_or_default(&mcp_config_path).unwrap();
    assert_eq!(updated_mcp_config.mcp_servers.len(), 1);
    assert!(updated_mcp_config.mcp_servers.contains_key("user-server"));
    assert!(!updated_mcp_config.mcp_servers.contains_key("agpm-server"));
}

#[test]
fn test_clean_mcp_servers_no_servers() -> Result<()> {
    let temp = tempfile::TempDir::new().unwrap();
    let project_root = temp.path();

    // Run clean_mcp_servers on empty project
    let result = clean_mcp_servers(project_root);
    result?;
    Ok(())
}

#[test]
fn test_list_mcp_servers() -> Result<()> {
    let temp = tempfile::TempDir::new().unwrap();
    let project_root = temp.path();
    let claude_dir = project_root.join(".claude");
    let settings_path = claude_dir.join("settings.local.json");

    std::fs::create_dir_all(&claude_dir).unwrap();

    // Create settings with mixed servers
    let mut settings = ClaudeSettings::default();
    let mut servers = HashMap::new();

    servers.insert(
        "managed-server".to_string(),
        McpServerConfig {
            command: Some("managed".to_string()),
            args: vec![],
            env: None,
            r#type: None,
            url: None,
            headers: None,
            agpm_metadata: Some(AgpmMetadata {
                managed: true,
                source: Some("test".to_string()),
                version: Some("v2.0.0".to_string()),
                installed_at: "2024-01-01T00:00:00Z".to_string(),
                dependency_name: None,
            }),
        },
    );

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
    settings.save(&settings_path).unwrap();

    // Run list_mcp_servers - just verify it doesn't error
    let result = list_mcp_servers(project_root);
    result?;
    Ok(())
}

#[test]
fn test_list_mcp_servers_no_file() -> Result<()> {
    let temp = tempfile::TempDir::new().unwrap();
    let project_root = temp.path();

    // Run list_mcp_servers with no settings file
    let result = list_mcp_servers(project_root);
    result?;
    Ok(())
}

#[test]
fn test_list_mcp_servers_empty() -> Result<()> {
    let temp = tempfile::TempDir::new().unwrap();
    let project_root = temp.path();
    let claude_dir = project_root.join(".claude");
    let settings_path = claude_dir.join("settings.local.json");

    std::fs::create_dir_all(&claude_dir).unwrap();

    // Create settings with no servers
    let settings = ClaudeSettings::default();
    settings.save(&settings_path).unwrap();

    // Run list_mcp_servers
    let result = list_mcp_servers(project_root);
    result?;
    Ok(())
}

#[tokio::test]
async fn test_merge_mcp_servers_unchanged_detection() {
    let temp = tempdir().unwrap();
    setup_project_root(temp.path());
    let config_path = temp.path().join(".mcp.json");

    // Create initial config with a server
    let initial_config = json!({
        "mcpServers": {
            "test-server": {
                "command": "node",
                "args": ["server.js"],
                "_agpm": {
                    "managed": true,
                    "source": "test-source",
                    "version": "v1.0.0",
                    "installed_at": "2024-01-01T00:00:00Z"
                }
            }
        }
    });

    tokio::fs::write(&config_path, serde_json::to_string_pretty(&initial_config).unwrap())
        .await
        .unwrap();

    // Create "same" server configuration (only timestamp differs)
    let mut agpm_servers = HashMap::new();
    agpm_servers.insert(
        "test-server".to_string(),
        McpServerConfig {
            command: Some("node".to_string()),
            args: vec!["server.js".to_string()],
            env: None,
            r#type: None,
            url: None,
            headers: None,
            agpm_metadata: Some(AgpmMetadata {
                managed: true,
                source: Some("test-source".to_string()),
                version: Some("v1.0.0".to_string()),
                installed_at: "2024-01-02T00:00:00Z".to_string(), // Different timestamp
                dependency_name: None,
            }),
        },
    );

    // Merge should detect no changes (ignoring timestamps)
    let changed_count = merge_mcp_servers(&config_path, agpm_servers).await.unwrap();
    assert_eq!(changed_count, 0, "Should detect no changes when only timestamp differs");
}

#[tokio::test]
async fn test_merge_mcp_servers_actual_change() {
    let temp = tempdir().unwrap();
    setup_project_root(temp.path());
    let config_path = temp.path().join(".mcp.json");

    // Create initial config with a server
    let initial_config = json!({
        "mcpServers": {
            "test-server": {
                "command": "node",
                "args": ["server.js"],
                "_agpm": {
                    "managed": true,
                    "source": "test-source",
                    "version": "v1.0.0",
                    "installed_at": "2024-01-01T00:00:00Z"
                }
            }
        }
    });

    tokio::fs::write(&config_path, serde_json::to_string_pretty(&initial_config).unwrap())
        .await
        .unwrap();

    // Create modified server configuration
    let mut agpm_servers = HashMap::new();
    agpm_servers.insert(
        "test-server".to_string(),
        McpServerConfig {
            command: Some("python".to_string()), // Changed command
            args: vec!["server.py".to_string()],
            env: None,
            r#type: None,
            url: None,
            headers: None,
            agpm_metadata: Some(AgpmMetadata {
                managed: true,
                source: Some("test-source".to_string()),
                version: Some("v1.0.0".to_string()),
                installed_at: "2024-01-01T00:00:00Z".to_string(),
                dependency_name: None,
            }),
        },
    );

    // Merge should detect changes
    let changed_count = merge_mcp_servers(&config_path, agpm_servers).await.unwrap();
    assert_eq!(changed_count, 1, "Should detect changes when server configuration differs");
}

#[tokio::test]
async fn test_merge_mcp_servers_new_server() {
    let temp = tempdir().unwrap();
    setup_project_root(temp.path());
    let config_path = temp.path().join(".mcp.json");

    // Create empty initial config
    let initial_config = json!({
        "mcpServers": {}
    });

    tokio::fs::write(&config_path, serde_json::to_string_pretty(&initial_config).unwrap())
        .await
        .unwrap();

    // Add a new server
    let mut agpm_servers = HashMap::new();
    agpm_servers.insert(
        "new-server".to_string(),
        McpServerConfig {
            command: Some("node".to_string()),
            args: vec!["server.js".to_string()],
            env: None,
            r#type: None,
            url: None,
            headers: None,
            agpm_metadata: Some(AgpmMetadata {
                managed: true,
                source: Some("test-source".to_string()),
                version: Some("v1.0.0".to_string()),
                installed_at: "2024-01-01T00:00:00Z".to_string(),
                dependency_name: None,
            }),
        },
    );

    // Merge should detect new server as changed
    let changed_count = merge_mcp_servers(&config_path, agpm_servers).await.unwrap();
    assert_eq!(changed_count, 1, "Should detect new server as changed");
}

#[tokio::test]
async fn test_merge_mcp_servers_mixed_changes() {
    let temp = tempdir().unwrap();
    setup_project_root(temp.path());
    let config_path = temp.path().join(".mcp.json");

    // Create initial config with two servers
    let initial_config = json!({
        "mcpServers": {
            "unchanged-server": {
                "command": "node",
                "args": ["server.js"],
                "_agpm": {
                    "managed": true,
                    "source": "test-source",
                    "version": "v1.0.0",
                    "installed_at": "2024-01-01T00:00:00Z"
                }
            },
            "changed-server": {
                "command": "python",
                "args": ["server.py"],
                "_agpm": {
                    "managed": true,
                    "source": "test-source",
                    "version": "v1.0.0",
                    "installed_at": "2024-01-01T00:00:00Z"
                }
            }
        }
    });

    tokio::fs::write(&config_path, serde_json::to_string_pretty(&initial_config).unwrap())
        .await
        .unwrap();

    // Create server configurations (one unchanged, one changed, one new)
    let mut agpm_servers = HashMap::new();

    // Unchanged server (same config, different timestamp)
    agpm_servers.insert(
        "unchanged-server".to_string(),
        McpServerConfig {
            command: Some("node".to_string()),
            args: vec!["server.js".to_string()],
            env: None,
            r#type: None,
            url: None,
            headers: None,
            agpm_metadata: Some(AgpmMetadata {
                managed: true,
                source: Some("test-source".to_string()),
                version: Some("v1.0.0".to_string()),
                installed_at: "2024-01-02T00:00:00Z".to_string(), // Different timestamp only
                dependency_name: None,
            }),
        },
    );

    // Changed server (different command)
    agpm_servers.insert(
        "changed-server".to_string(),
        McpServerConfig {
            command: Some("ruby".to_string()), // Changed command
            args: vec!["server.rb".to_string()],
            env: None,
            r#type: None,
            url: None,
            headers: None,
            agpm_metadata: Some(AgpmMetadata {
                managed: true,
                source: Some("test-source".to_string()),
                version: Some("v1.0.0".to_string()),
                installed_at: "2024-01-01T00:00:00Z".to_string(),
                dependency_name: None,
            }),
        },
    );

    // New server
    agpm_servers.insert(
        "new-server".to_string(),
        McpServerConfig {
            command: Some("go".to_string()),
            args: vec!["server".to_string()],
            env: None,
            r#type: None,
            url: None,
            headers: None,
            agpm_metadata: Some(AgpmMetadata {
                managed: true,
                source: Some("test-source".to_string()),
                version: Some("v1.0.0".to_string()),
                installed_at: "2024-01-01T00:00:00Z".to_string(),
                dependency_name: None,
            }),
        },
    );

    // Merge should detect 2 changes (changed server + new server)
    let changed_count = merge_mcp_servers(&config_path, agpm_servers).await.unwrap();
    assert_eq!(changed_count, 2, "Should detect 2 changes: 1 modified server + 1 new server");
}

#[tokio::test]
async fn test_merge_mcp_servers_empty_updates() {
    let temp = tempdir().unwrap();
    setup_project_root(temp.path());
    let config_path = temp.path().join(".mcp.json");

    // Create initial config with servers
    let initial_config = json!({
        "mcpServers": {
            "existing-server": {
                "command": "node",
                "args": ["server.js"],
                "_agpm": {
                    "managed": true,
                    "source": "test-source",
                    "version": "v1.0.0",
                    "installed_at": "2024-01-01T00:00:00Z"
                }
            }
        }
    });

    tokio::fs::write(&config_path, serde_json::to_string_pretty(&initial_config).unwrap())
        .await
        .unwrap();

    // Empty updates should remove all managed servers
    let agpm_servers = HashMap::new();

    // Merge should detect removal as changes (0 changes to add, but servers are removed)
    let changed_count = merge_mcp_servers(&config_path, agpm_servers).await.unwrap();
    assert_eq!(changed_count, 0, "Should detect 0 changes when only removing servers");
}
