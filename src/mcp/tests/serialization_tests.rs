use crate::mcp::models::{AgpmMetadata, ClaudeSettings, McpConfig, McpServerConfig};
use anyhow::Result;
use serde_json::json;
use std::collections::HashMap;

#[test]
fn test_claude_settings_serialization() -> Result<()> {
    // Add various fields
    let mut settings = ClaudeSettings {
        permissions: Some(json!({"allow": ["test"], "deny": []})),
        ..Default::default()
    };

    let mut servers = HashMap::new();
    servers.insert(
        "test".to_string(),
        McpServerConfig {
            command: Some("test-cmd".to_string()),
            args: vec!["arg1".to_string()],
            env: Some({
                let mut env = HashMap::new();
                env.insert("VAR".to_string(), json!("value"));
                env
            }),
            r#type: None,
            url: None,
            headers: None,
            agpm_metadata: Some(AgpmMetadata {
                managed: true,
                source: Some("test-source".to_string()),
                version: Some("v1.0.0".to_string()),
                installed_at: "2024-01-01T00:00:00Z".to_string(),
                dependency_name: Some("test".to_string()),
            }),
        },
    );
    settings.mcp_servers = Some(servers);

    settings.other.insert("custom".to_string(), json!("value"));

    // Serialize and deserialize
    let json = serde_json::to_string(&settings)?;
    let deserialized: ClaudeSettings = serde_json::from_str(&json)?;

    assert_eq!(deserialized.permissions, settings.permissions);
    assert_eq!(deserialized.mcp_servers.as_ref().expect("mcp_servers should be present").len(), 1);
    assert_eq!(deserialized.other.get("custom"), settings.other.get("custom"));
    Ok(())
}

#[test]
fn test_mcp_config_serialization() -> Result<()> {
    let mut config = McpConfig::default();

    config.mcp_servers.insert(
        "test".to_string(),
        McpServerConfig {
            command: Some("test-cmd".to_string()),
            args: vec!["arg1".to_string(), "arg2".to_string()],
            env: Some({
                let mut env = HashMap::new();
                env.insert("TEST_VAR".to_string(), json!("test_value"));
                env
            }),
            r#type: None,
            url: None,
            headers: None,
            agpm_metadata: Some(AgpmMetadata {
                managed: true,
                source: Some("github.com/test/repo".to_string()),
                version: Some("v2.0.0".to_string()),
                installed_at: "2024-01-01T12:00:00Z".to_string(),
                dependency_name: Some("test-dep".to_string()),
            }),
        },
    );

    // Serialize and deserialize
    let json = serde_json::to_string_pretty(&config)?;
    let deserialized: McpConfig = serde_json::from_str(&json)?;

    assert_eq!(deserialized.mcp_servers.len(), 1);
    let server = &deserialized.mcp_servers["test"];
    assert_eq!(server.command, Some("test-cmd".to_string()));
    assert_eq!(server.args.len(), 2);
    assert!(server.env.is_some());
    assert!(server.agpm_metadata.is_some());

    let metadata = server.agpm_metadata.as_ref().expect("agpm_metadata should be present");
    assert!(metadata.managed);
    assert_eq!(metadata.source, Some("github.com/test/repo".to_string()));
    assert_eq!(metadata.version, Some("v2.0.0".to_string()));
    Ok(())
}

#[test]
fn test_mcp_server_config_minimal_serialization() -> Result<()> {
    let config = McpServerConfig {
        command: Some("minimal".to_string()),
        args: vec![],
        env: None,
        r#type: None,
        url: None,
        headers: None,
        agpm_metadata: None,
    };

    let json = serde_json::to_string(&config)?;
    let deserialized: McpServerConfig = serde_json::from_str(&json)?;

    assert_eq!(deserialized.command, Some("minimal".to_string()));
    assert!(deserialized.args.is_empty());
    assert!(deserialized.env.is_none());
    assert!(deserialized.agpm_metadata.is_none());

    // Check that empty args are skipped in serialization
    assert!(!json.contains(r#""args":[]"#));
    Ok(())
}

#[test]
fn test_agpm_metadata_serialization() -> Result<()> {
    let metadata = AgpmMetadata {
        managed: true,
        source: Some("test-source".to_string()),
        version: None,
        installed_at: "2024-01-01T00:00:00Z".to_string(),
        dependency_name: Some("test-dep".to_string()),
    };

    let json = serde_json::to_string(&metadata)?;
    let deserialized: AgpmMetadata = serde_json::from_str(&json)?;

    assert!(deserialized.managed);
    assert_eq!(deserialized.source, Some("test-source".to_string()));
    assert_eq!(deserialized.version, None);
    assert_eq!(deserialized.installed_at, "2024-01-01T00:00:00Z");
    assert_eq!(deserialized.dependency_name, Some("test-dep".to_string()));

    // Check that None version is skipped in serialization
    assert!(!json.contains(r#""version""#));
    Ok(())
}
