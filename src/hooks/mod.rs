//! Hook configuration management for CCPM
//!
//! This module handles Claude Code hook configurations, including:
//! - Installing hook JSON files to `.claude/ccpm/hooks/`
//! - Merging hook configurations into `settings.local.json`
//! - Managing hook lifecycle and dependencies

pub mod merge;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

/// Hook event types supported by Claude Code
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum HookEvent {
    #[serde(rename = "PreToolUse")]
    PreToolUse,
    #[serde(rename = "PostToolUse")]
    PostToolUse,
    #[serde(rename = "UserPromptSubmit")]
    UserPromptSubmit,
    #[serde(rename = "UserPromptReceive")]
    UserPromptReceive,
    #[serde(rename = "AssistantResponseReceive")]
    AssistantResponseReceive,
}

/// Hook configuration as stored in JSON files
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookConfig {
    /// Events this hook should trigger on
    pub events: Vec<HookEvent>,
    /// Regex matcher pattern for tools or commands
    pub matcher: String,
    /// Type of hook (usually "command")
    #[serde(rename = "type")]
    pub hook_type: String,
    /// Command to execute
    pub command: String,
    /// Timeout in milliseconds
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout: Option<u32>,
    /// Description of what this hook does
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// A single hook command within a matcher group
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookCommand {
    /// Type of hook (usually "command")
    #[serde(rename = "type")]
    pub hook_type: String,
    /// Command to execute
    pub command: String,
    /// Timeout in milliseconds
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout: Option<u32>,
    /// CCPM metadata for tracking
    #[serde(rename = "_ccpm", skip_serializing_if = "Option::is_none")]
    pub ccpm_metadata: Option<CcpmHookMetadata>,
}

/// Metadata for CCPM-managed hooks
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CcpmHookMetadata {
    pub managed: bool,
    pub dependency_name: String,
    pub source: String,
    pub version: String,
    pub installed_at: String,
}

/// A matcher group containing multiple hooks
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatcherGroup {
    pub matcher: String,
    pub hooks: Vec<HookCommand>,
}

/// Load hook configurations from a directory
pub fn load_hook_configs(hooks_dir: &Path) -> Result<HashMap<String, HookConfig>> {
    let mut configs = HashMap::new();

    if !hooks_dir.exists() {
        return Ok(configs);
    }

    for entry in std::fs::read_dir(hooks_dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.extension().and_then(|s| s.to_str()) == Some("json") {
            let name = path
                .file_stem()
                .and_then(|s| s.to_str())
                .ok_or_else(|| anyhow::anyhow!("Invalid hook filename"))?
                .to_string();

            let content = std::fs::read_to_string(&path)
                .with_context(|| format!("Failed to read hook file: {}", path.display()))?;

            let config: HookConfig = serde_json::from_str(&content)
                .with_context(|| format!("Failed to parse hook config: {}", path.display()))?;

            configs.insert(name, config);
        }
    }

    Ok(configs)
}

// Re-export commonly used merge functions
pub use merge::{apply_hooks_to_settings, merge_hooks_advanced, MergeResult};

/// Validate a hook configuration
pub fn validate_hook_config(config: &HookConfig, script_path: &Path) -> Result<()> {
    // Validate events
    if config.events.is_empty() {
        return Err(anyhow::anyhow!("Hook must specify at least one event"));
    }

    // Validate matcher regex
    regex::Regex::new(&config.matcher)
        .with_context(|| format!("Invalid regex pattern in matcher: {}", config.matcher))?;

    // Validate hook type
    if config.hook_type != "command" {
        return Err(anyhow::anyhow!(
            "Only 'command' hook type is currently supported"
        ));
    }

    // Validate that the referenced script exists
    let script_full_path = if config.command.starts_with(".claude/ccpm/scripts/") {
        script_path
            .parent()
            .and_then(|p| p.parent())
            .and_then(|p| p.parent())
            .map(|p| p.join(&config.command))
    } else {
        None
    };

    if let Some(path) = script_full_path {
        if !path.exists() {
            return Err(anyhow::anyhow!(
                "Hook references non-existent script: {}",
                config.command
            ));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_hook_config_serialization() {
        let config = HookConfig {
            events: vec![HookEvent::PreToolUse],
            matcher: "Bash|Write".to_string(),
            hook_type: "command".to_string(),
            command: ".claude/ccpm/scripts/security-check.sh".to_string(),
            timeout: Some(5000),
            description: Some("Security validation".to_string()),
        };

        let json = serde_json::to_string_pretty(&config).unwrap();
        let parsed: HookConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.events.len(), 1);
        assert_eq!(parsed.matcher, "Bash|Write");
        assert_eq!(parsed.timeout, Some(5000));
    }

    #[test]
    fn test_load_hook_configs() {
        let temp = tempdir().unwrap();
        let hooks_dir = temp.path().join("hooks");
        std::fs::create_dir(&hooks_dir).unwrap();

        let config = HookConfig {
            events: vec![HookEvent::PreToolUse],
            matcher: ".*".to_string(),
            hook_type: "command".to_string(),
            command: "test.sh".to_string(),
            timeout: None,
            description: None,
        };

        let config_path = hooks_dir.join("test-hook.json");
        std::fs::write(&config_path, serde_json::to_string(&config).unwrap()).unwrap();

        let configs = load_hook_configs(&hooks_dir).unwrap();
        assert_eq!(configs.len(), 1);
        assert!(configs.contains_key("test-hook"));
    }

    #[test]
    fn test_validate_hook_config() {
        let temp = tempdir().unwrap();

        let config = HookConfig {
            events: vec![HookEvent::PreToolUse],
            matcher: "Bash|Write".to_string(),
            hook_type: "command".to_string(),
            command: ".claude/ccpm/scripts/test.sh".to_string(),
            timeout: None,
            description: None,
        };

        // Should pass basic validation (script existence check will fail but that's ok for this test)
        let result = validate_hook_config(&config, temp.path());
        assert!(
            result.is_ok()
                || result
                    .unwrap_err()
                    .to_string()
                    .contains("non-existent script")
        );

        // Test invalid regex
        let bad_config = HookConfig {
            events: vec![HookEvent::PreToolUse],
            matcher: "[invalid regex".to_string(),
            hook_type: "command".to_string(),
            command: "test.sh".to_string(),
            timeout: None,
            description: None,
        };

        assert!(validate_hook_config(&bad_config, temp.path()).is_err());
    }
}
