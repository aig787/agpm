//! Hook configuration management for AGPM
//!
//! This module handles Claude Code hook configurations, including:
//! - Installing hook JSON files to `.claude/agpm/hooks/`
//! - Converting them to Claude Code format in `settings.local.json`
//! - Managing hook lifecycle and dependencies

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

/// Hook event types supported by Claude Code
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum HookEvent {
    /// Triggered before a tool is executed by Claude
    #[serde(rename = "PreToolUse")]
    PreToolUse,
    /// Triggered after a tool has been executed by Claude
    #[serde(rename = "PostToolUse")]
    PostToolUse,
    /// Triggered when Claude needs permission or input is idle
    #[serde(rename = "Notification")]
    Notification,
    /// Triggered when the user submits a prompt
    #[serde(rename = "UserPromptSubmit")]
    UserPromptSubmit,
    /// Triggered when main Claude Code agent finishes responding
    #[serde(rename = "Stop")]
    Stop,
    /// Triggered when a subagent (Task tool) finishes responding
    #[serde(rename = "SubagentStop")]
    SubagentStop,
    /// Triggered before compact operation
    #[serde(rename = "PreCompact")]
    PreCompact,
    /// Triggered when starting/resuming a session
    #[serde(rename = "SessionStart")]
    SessionStart,
    /// Triggered when session ends
    #[serde(rename = "SessionEnd")]
    SessionEnd,
    /// Unknown or future hook event type
    #[serde(untagged)]
    Other(String),
}

/// Hook configuration as stored in JSON files
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookConfig {
    /// Events this hook should trigger on
    pub events: Vec<HookEvent>,
    /// Regex matcher pattern for tools or commands (optional, only needed for tool-triggered events)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub matcher: Option<String>,
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
    /// AGPM metadata for tracking
    #[serde(rename = "_agpm", skip_serializing_if = "Option::is_none")]
    pub agpm_metadata: Option<AgpmHookMetadata>,
}

/// Metadata for AGPM-managed hooks
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgpmHookMetadata {
    /// Whether this hook is managed by AGPM (true) or manually configured (false)
    pub managed: bool,
    /// Name of the dependency that installed this hook
    pub dependency_name: String,
    /// Source repository name where this hook originated
    pub source: String,
    /// Version constraint or resolved version of the hook dependency
    pub version: String,
    /// ISO 8601 timestamp when this hook was installed
    pub installed_at: String,
}

/// A matcher group containing multiple hooks with the same regex pattern.
///
/// In Claude Code's settings.local.json, hooks are organized into matcher groups
/// where multiple hook commands can share the same regex pattern for tool matching.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatcherGroup {
    /// Regex pattern that determines which tools this group applies to
    pub matcher: String,
    /// List of hook commands to execute when the matcher pattern matches
    pub hooks: Vec<HookCommand>,
}

/// Load hook configurations from a directory containing JSON files.
///
/// Scans the specified directory for `.json` files and parses each one as a
/// [`HookConfig`]. The filename (without extension) becomes the hook name in
/// the returned map.
///
/// # Arguments
///
/// * `hooks_dir` - Directory path containing hook JSON configuration files
///
/// # Returns
///
/// A `HashMap` mapping hook names to their configurations. If the directory
/// doesn't exist, returns an empty map.
///
/// # Errors
///
/// Returns an error if:
/// - Directory reading fails due to permissions or I/O errors
/// - Any JSON file cannot be read or parsed
/// - A filename is invalid or cannot be converted to a string
///
/// # Examples
///
/// ```rust,no_run
/// use agpm::hooks::load_hook_configs;
/// use std::path::Path;
///
/// # fn example() -> anyhow::Result<()> {
/// let hooks_dir = Path::new(".claude/agpm/hooks");
/// let configs = load_hook_configs(hooks_dir)?;
///
/// for (name, config) in configs {
///     println!("Loaded hook '{}' with {} events", name, config.events.len());
/// }
/// # Ok(())
/// # }
/// ```
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

/// Convert AGPM hook configs to Claude Code format
///
/// Transforms hooks from the AGPM format to the format expected by Claude Code.
/// Groups hooks by event type and handles optional matchers correctly.
fn convert_to_claude_format(
    hook_configs: HashMap<String, HookConfig>,
) -> Result<serde_json::Value> {
    use serde_json::{Map, Value, json};

    let mut events_map: Map<String, Value> = Map::new();

    for (_name, config) in hook_configs {
        for event in &config.events {
            let event_name = event_to_string(event);

            // Create the hook object in Claude format
            let hook_obj = json!({
                "type": config.hook_type,
                "command": config.command,
                "timeout": config.timeout
            });

            // Get or create the event array
            let event_array = events_map.entry(event_name).or_insert_with(|| json!([]));
            let event_vec = event_array.as_array_mut().unwrap();

            if let Some(ref matcher) = config.matcher {
                // Tool-triggered event with matcher
                // Find existing matcher group or create new one
                let mut found_group = false;
                for group in event_vec.iter_mut() {
                    if let Some(group_matcher) = group.get("matcher").and_then(|m| m.as_str())
                        && group_matcher == matcher
                    {
                        // Add to existing matcher group
                        if let Some(hooks_array) =
                            group.get_mut("hooks").and_then(|h| h.as_array_mut())
                        {
                            hooks_array.push(hook_obj.clone());
                            found_group = true;
                            break;
                        }
                    }
                }

                if !found_group {
                    // Create new matcher group
                    event_vec.push(json!({
                        "matcher": matcher,
                        "hooks": [hook_obj]
                    }));
                }
            } else {
                // Session event without matcher - add to first group or create new one
                if let Some(first_group) = event_vec.first_mut() {
                    // Add to existing group if it has no matcher
                    if first_group.as_object().unwrap().contains_key("matcher") {
                        // Create new group for session events
                        event_vec.push(json!({
                            "hooks": [hook_obj]
                        }));
                    } else if let Some(hooks_array) =
                        first_group.get_mut("hooks").and_then(|h| h.as_array_mut())
                    {
                        // Check for duplicates before adding
                        let hook_exists = hooks_array.iter().any(|existing_hook| {
                            existing_hook.get("command") == hook_obj.get("command")
                                && existing_hook.get("type") == hook_obj.get("type")
                        });
                        if !hook_exists {
                            hooks_array.push(hook_obj);
                        }
                    }
                } else {
                    // Create first group for session events
                    event_vec.push(json!({
                        "hooks": [hook_obj]
                    }));
                }
            }
        }
    }

    Ok(Value::Object(events_map))
}

/// Convert event enum to string
fn event_to_string(event: &HookEvent) -> String {
    match event {
        HookEvent::PreToolUse => "PreToolUse".to_string(),
        HookEvent::PostToolUse => "PostToolUse".to_string(),
        HookEvent::Notification => "Notification".to_string(),
        HookEvent::UserPromptSubmit => "UserPromptSubmit".to_string(),
        HookEvent::Stop => "Stop".to_string(),
        HookEvent::SubagentStop => "SubagentStop".to_string(),
        HookEvent::PreCompact => "PreCompact".to_string(),
        HookEvent::SessionStart => "SessionStart".to_string(),
        HookEvent::SessionEnd => "SessionEnd".to_string(),
        HookEvent::Other(event_name) => event_name.clone(),
    }
}

/// Configure hooks from source files into .claude/settings.local.json
///
/// This function:
/// 1. Reads hook JSON files directly from source locations (no file copying)
/// 2. Converts them to Claude Code format
/// 3. Updates .claude/settings.local.json with proper event-based structure
/// 4. Can be called from both `add` and `install` commands
pub async fn install_hooks(
    lockfile: &crate::lockfile::LockFile,
    project_root: &Path,
    cache: &crate::cache::Cache,
) -> Result<()> {
    if lockfile.hooks.is_empty() {
        return Ok(());
    }

    let claude_dir = project_root.join(".claude");
    let settings_path = claude_dir.join("settings.local.json");

    // Ensure directory exists
    crate::utils::fs::ensure_dir(&claude_dir)?;

    // Load hook configurations directly from source files
    let mut hook_configs = HashMap::new();

    for entry in &lockfile.hooks {
        // Get the source file path
        let source_path = if let Some(source_name) = &entry.source {
            let url = entry
                .url
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("Hook {} has no URL", entry.name))?;

            // Check if this is a local directory source
            let is_local_source = entry.resolved_commit.as_deref().is_none_or(str::is_empty);

            if is_local_source {
                // Local directory source - use URL as path directly
                std::path::PathBuf::from(url).join(&entry.path)
            } else {
                // Git-based source - get worktree
                let sha = entry.resolved_commit.as_deref().ok_or_else(|| {
                    anyhow::anyhow!("Hook {} missing resolved commit SHA", entry.name)
                })?;

                let worktree = cache
                    .get_or_create_worktree_for_sha(source_name, url, sha, Some(&entry.name))
                    .await?;
                worktree.join(&entry.path)
            }
        } else {
            // Local file - resolve relative to project root
            let candidate = Path::new(&entry.path);
            if candidate.is_absolute() {
                candidate.to_path_buf()
            } else {
                project_root.join(candidate)
            }
        };

        // Read and parse the hook configuration
        let content = tokio::fs::read_to_string(&source_path)
            .await
            .with_context(|| format!("Failed to read hook file: {}", source_path.display()))?;

        let config: HookConfig = serde_json::from_str(&content)
            .with_context(|| format!("Failed to parse hook config: {}", source_path.display()))?;

        hook_configs.insert(entry.name.clone(), config);
    }

    // Load existing settings
    let mut settings = crate::mcp::ClaudeSettings::load_or_default(&settings_path)?;

    // Convert hooks to Claude Code format
    let claude_hooks = convert_to_claude_format(hook_configs)?;

    // Compare with existing hooks to detect changes
    let hooks_changed = match &settings.hooks {
        Some(existing_hooks) => existing_hooks != &claude_hooks,
        None => claude_hooks.as_object().is_none_or(|obj| !obj.is_empty()),
    };

    if hooks_changed {
        // Count actual configured hooks (after deduplication)
        let configured_count = claude_hooks.as_object().map_or(0, |events| {
            events
                .values()
                .filter_map(|event_groups| event_groups.as_array())
                .map(|groups| {
                    groups
                        .iter()
                        .filter_map(|group| group.get("hooks")?.as_array())
                        .map(std::vec::Vec::len)
                        .sum::<usize>()
                })
                .sum::<usize>()
        });

        // Update settings with hooks (replaces existing hooks completely)
        settings.hooks = Some(claude_hooks);

        // Save updated settings
        settings.save(&settings_path)?;

        if configured_count > 0 {
            println!("✓ Configured {configured_count} hook(s) in .claude/settings.local.json");
        }
    }

    Ok(())
}

/// Validate a hook configuration for correctness and safety.
///
/// Performs comprehensive validation of a hook configuration including:
/// - Event list validation (must have at least one event)
/// - Regex pattern syntax validation for the matcher
/// - Hook type validation (only "command" type is supported)
/// - Script existence validation for AGPM-managed scripts
///
/// # Arguments
///
/// * `config` - The hook configuration to validate
/// * `script_path` - Path to the hook file (used to resolve relative script paths)
///
/// # Returns
///
/// Returns `Ok(())` if the configuration is valid.
///
/// # Errors
///
/// Returns an error if:
/// - No events are specified
/// - The matcher regex pattern is invalid
/// - Unsupported hook type is used (only "command" is supported)
/// - Referenced script file doesn't exist (for AGPM-managed scripts)
///
/// # Examples
///
/// ```rust,no_run
/// use agpm::hooks::{validate_hook_config, HookConfig, HookEvent};
/// use std::path::Path;
///
/// # fn example() -> anyhow::Result<()> {
/// let config = HookConfig {
///     events: vec![HookEvent::PreToolUse],
///     matcher: Some("Bash|Write".to_string()),
///     hook_type: "command".to_string(),
///     command: "echo 'validation'".to_string(),
///     timeout: Some(5000),
///     description: None,
/// };
///
/// let hook_file = Path::new(".claude/agpm/hooks/test.json");
/// validate_hook_config(&config, hook_file)?;
/// println!("Hook configuration is valid!");
/// # Ok(())
/// # }
/// ```
pub fn validate_hook_config(config: &HookConfig, script_path: &Path) -> Result<()> {
    // Validate events
    if config.events.is_empty() {
        return Err(anyhow::anyhow!("Hook must specify at least one event"));
    }

    // Validate matcher regex if present
    if let Some(ref matcher) = config.matcher {
        regex::Regex::new(matcher)
            .with_context(|| format!("Invalid regex pattern in matcher: {matcher}"))?;
    }

    // Validate hook type
    if config.hook_type != "command" {
        return Err(anyhow::anyhow!("Only 'command' hook type is currently supported"));
    }

    // Validate that the referenced script exists
    let script_full_path = if config.command.starts_with(".claude/agpm/scripts/") {
        // If script_path is the hook file (e.g., .claude/agpm/hooks/test.json),
        // we need to go up to the project root:
        // test.json -> hooks/ -> agpm/ -> .claude/ -> project_root
        script_path
            .parent() // hooks/
            .and_then(|p| p.parent()) // agpm/
            .and_then(|p| p.parent()) // .claude/
            .and_then(|p| p.parent()) // project root
            .map(|p| p.join(&config.command))
    } else {
        None
    };

    if let Some(path) = script_full_path
        && !path.exists()
    {
        return Err(anyhow::anyhow!("Hook references non-existent script: {}", config.command));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_hook_event_serialization() {
        // Test all hook event types serialize correctly
        let events = vec![
            (HookEvent::PreToolUse, r#""PreToolUse""#),
            (HookEvent::PostToolUse, r#""PostToolUse""#),
            (HookEvent::Notification, r#""Notification""#),
            (HookEvent::UserPromptSubmit, r#""UserPromptSubmit""#),
            (HookEvent::Stop, r#""Stop""#),
            (HookEvent::SubagentStop, r#""SubagentStop""#),
            (HookEvent::PreCompact, r#""PreCompact""#),
            (HookEvent::SessionStart, r#""SessionStart""#),
            (HookEvent::SessionEnd, r#""SessionEnd""#),
            (HookEvent::Other("CustomEvent".to_string()), r#""CustomEvent""#),
        ];

        for (event, expected) in events {
            let json = serde_json::to_string(&event).unwrap();
            assert_eq!(json, expected);
            let parsed: HookEvent = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed, event);
        }
    }

    #[test]
    fn test_hook_config_serialization() {
        let config = HookConfig {
            events: vec![HookEvent::PreToolUse, HookEvent::PostToolUse],
            matcher: Some("Bash|Write".to_string()),
            hook_type: "command".to_string(),
            command: ".claude/agpm/scripts/security-check.sh".to_string(),
            timeout: Some(5000),
            description: Some("Security validation".to_string()),
        };

        let json = serde_json::to_string_pretty(&config).unwrap();
        let parsed: HookConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.events.len(), 2);
        assert_eq!(parsed.matcher, Some("Bash|Write".to_string()));
        assert_eq!(parsed.timeout, Some(5000));
        assert_eq!(parsed.description, Some("Security validation".to_string()));
    }

    #[test]
    fn test_hook_config_minimal() {
        // Test minimal config without optional fields
        let config = HookConfig {
            events: vec![HookEvent::UserPromptSubmit],
            matcher: Some(".*".to_string()),
            hook_type: "command".to_string(),
            command: "echo 'test'".to_string(),
            timeout: None,
            description: None,
        };

        let json = serde_json::to_string(&config).unwrap();
        assert!(!json.contains("timeout"));
        assert!(!json.contains("description"));
    }

    #[test]
    fn test_hook_command_serialization() {
        let metadata = AgpmHookMetadata {
            managed: true,
            dependency_name: "test-hook".to_string(),
            source: "community".to_string(),
            version: "v1.0.0".to_string(),
            installed_at: "2024-01-01T00:00:00Z".to_string(),
        };

        let command = HookCommand {
            hook_type: "command".to_string(),
            command: "test.sh".to_string(),
            timeout: Some(3000),
            agpm_metadata: Some(metadata.clone()),
        };

        let json = serde_json::to_string(&command).unwrap();
        let parsed: HookCommand = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.hook_type, "command");
        assert_eq!(parsed.command, "test.sh");
        assert_eq!(parsed.timeout, Some(3000));
        assert!(parsed.agpm_metadata.is_some());
        let meta = parsed.agpm_metadata.unwrap();
        assert!(meta.managed);
        assert_eq!(meta.dependency_name, "test-hook");
    }

    #[test]
    fn test_matcher_group_serialization() {
        let command = HookCommand {
            hook_type: "command".to_string(),
            command: "test.sh".to_string(),
            timeout: None,
            agpm_metadata: None,
        };

        let group = MatcherGroup {
            matcher: "Bash.*".to_string(),
            hooks: vec![command.clone(), command.clone()],
        };

        let json = serde_json::to_string(&group).unwrap();
        let parsed: MatcherGroup = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.matcher, "Bash.*");
        assert_eq!(parsed.hooks.len(), 2);
    }

    #[test]
    fn test_load_hook_configs() {
        let temp = tempdir().unwrap();
        let hooks_dir = temp.path().join("hooks");
        std::fs::create_dir(&hooks_dir).unwrap();

        // Create multiple hook configs
        let config1 = HookConfig {
            events: vec![HookEvent::PreToolUse],
            matcher: Some(".*".to_string()),
            hook_type: "command".to_string(),
            command: "test1.sh".to_string(),
            timeout: None,
            description: None,
        };

        let config2 = HookConfig {
            events: vec![HookEvent::PostToolUse],
            matcher: Some("Write".to_string()),
            hook_type: "command".to_string(),
            command: "test2.sh".to_string(),
            timeout: Some(1000),
            description: Some("Test hook 2".to_string()),
        };

        fs::write(hooks_dir.join("test-hook1.json"), serde_json::to_string(&config1).unwrap())
            .unwrap();
        fs::write(hooks_dir.join("test-hook2.json"), serde_json::to_string(&config2).unwrap())
            .unwrap();

        // Also create a non-JSON file that should be ignored
        fs::write(hooks_dir.join("readme.txt"), "This is not a hook").unwrap();

        let configs = load_hook_configs(&hooks_dir).unwrap();
        assert_eq!(configs.len(), 2);
        assert!(configs.contains_key("test-hook1"));
        assert!(configs.contains_key("test-hook2"));

        let hook1 = &configs["test-hook1"];
        assert_eq!(hook1.events.len(), 1);
        assert_eq!(hook1.command, "test1.sh");

        let hook2 = &configs["test-hook2"];
        assert_eq!(hook2.timeout, Some(1000));
    }

    #[test]
    fn test_load_hook_configs_empty_dir() {
        let temp = tempdir().unwrap();
        let hooks_dir = temp.path().join("empty_hooks");
        // Don't create the directory

        let configs = load_hook_configs(&hooks_dir).unwrap();
        assert_eq!(configs.len(), 0);
    }

    #[test]
    fn test_load_hook_configs_invalid_json() {
        let temp = tempdir().unwrap();
        let hooks_dir = temp.path().join("hooks");
        fs::create_dir(&hooks_dir).unwrap();

        // Write invalid JSON
        fs::write(hooks_dir.join("invalid.json"), "{ not valid json").unwrap();

        let result = load_hook_configs(&hooks_dir);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Failed to parse hook config"));
    }

    #[test]
    fn test_validate_hook_config_empty_events() {
        let temp = tempdir().unwrap();

        let config = HookConfig {
            events: vec![], // Empty events
            matcher: Some(".*".to_string()),
            hook_type: "command".to_string(),
            command: "test.sh".to_string(),
            timeout: None,
            description: None,
        };

        let result = validate_hook_config(&config, temp.path());
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("at least one event"));
    }

    #[test]
    fn test_validate_hook_config_invalid_regex() {
        let temp = tempdir().unwrap();

        let config = HookConfig {
            events: vec![HookEvent::PreToolUse],
            matcher: Some("[invalid regex".to_string()), // Invalid regex
            hook_type: "command".to_string(),
            command: "test.sh".to_string(),
            timeout: None,
            description: None,
        };

        let result = validate_hook_config(&config, temp.path());
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Invalid regex pattern"));
    }

    #[test]
    fn test_validate_hook_config_unsupported_type() {
        let temp = tempdir().unwrap();

        let config = HookConfig {
            events: vec![HookEvent::PreToolUse],
            matcher: Some(".*".to_string()),
            hook_type: "webhook".to_string(), // Unsupported type
            command: "test.sh".to_string(),
            timeout: None,
            description: None,
        };

        let result = validate_hook_config(&config, temp.path());
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Only 'command' hook type"));
    }

    #[test]
    fn test_validate_hook_config_script_exists() {
        let temp = tempdir().unwrap();

        // Create the expected directory structure with script
        let claude_dir = temp.path().join(".claude").join("agpm");
        let scripts_dir = claude_dir.join("scripts");
        let hooks_dir = claude_dir.join("hooks");
        fs::create_dir_all(&scripts_dir).unwrap();
        fs::create_dir_all(&hooks_dir).unwrap();

        let script_path = scripts_dir.join("test.sh");
        fs::write(&script_path, "#!/bin/bash\necho test").unwrap();

        let config = HookConfig {
            events: vec![HookEvent::PreToolUse],
            matcher: Some(".*".to_string()),
            hook_type: "command".to_string(),
            command: ".claude/agpm/scripts/test.sh".to_string(),
            timeout: None,
            description: None,
        };

        // The hook file would be at .claude/agpm/hooks/test.json
        // validate_hook_config goes up 3 levels from the hook path to find the project root
        let hook_json_path = hooks_dir.join("test.json");
        let result = validate_hook_config(&config, &hook_json_path);

        // Since the script exists at the expected location, this should succeed
        assert!(result.is_ok(), "Expected validation to succeed, but got: {:?}", result);
    }

    #[test]
    fn test_validate_hook_config_script_not_exists() {
        let temp = tempdir().unwrap();

        let config = HookConfig {
            events: vec![HookEvent::PreToolUse],
            matcher: Some(".*".to_string()),
            hook_type: "command".to_string(),
            command: ".claude/agpm/scripts/nonexistent.sh".to_string(),
            timeout: None,
            description: None,
        };

        // Pass the hook file path
        let hook_path = temp.path().join(".claude").join("agpm").join("hooks").join("test.json");
        let result = validate_hook_config(&config, &hook_path);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("non-existent script"));
    }

    #[test]
    fn test_validate_hook_config_non_claude_path() {
        let temp = tempdir().unwrap();

        // Test with a command that doesn't start with .claude/agpm/scripts/
        let config = HookConfig {
            events: vec![HookEvent::PreToolUse],
            matcher: Some(".*".to_string()),
            hook_type: "command".to_string(),
            command: "/usr/bin/echo".to_string(), // Absolute path not in .claude
            timeout: None,
            description: None,
        };

        let result = validate_hook_config(&config, temp.path());
        // Should pass - we don't validate non-.claude paths
        assert!(result.is_ok());
    }

    #[test]
    fn test_convert_to_claude_format_session_start() {
        // Test SessionStart hook without matcher
        let mut hook_configs = HashMap::new();
        hook_configs.insert(
            "session-hook".to_string(),
            HookConfig {
                events: vec![HookEvent::SessionStart],
                matcher: None, // No matcher for session events
                hook_type: "command".to_string(),
                command: "echo 'session started'".to_string(),
                timeout: Some(1000),
                description: Some("Session start hook".to_string()),
            },
        );

        let result = convert_to_claude_format(hook_configs).unwrap();
        let expected = serde_json::json!({
            "SessionStart": [
                {
                    "hooks": [
                        {
                            "type": "command",
                            "command": "echo 'session started'",
                            "timeout": 1000
                        }
                    ]
                }
            ]
        });

        assert_eq!(result, expected);
    }

    #[test]
    fn test_convert_to_claude_format_with_matcher() {
        // Test PreToolUse hook with matcher
        let mut hook_configs = HashMap::new();
        hook_configs.insert(
            "tool-hook".to_string(),
            HookConfig {
                events: vec![HookEvent::PreToolUse],
                matcher: Some("Bash|Write".to_string()),
                hook_type: "command".to_string(),
                command: "echo 'before tool use'".to_string(),
                timeout: None,
                description: None,
            },
        );

        let result = convert_to_claude_format(hook_configs).unwrap();
        let expected = serde_json::json!({
            "PreToolUse": [
                {
                    "matcher": "Bash|Write",
                    "hooks": [
                        {
                            "type": "command",
                            "command": "echo 'before tool use'",
                            "timeout": null
                        }
                    ]
                }
            ]
        });

        assert_eq!(result, expected);
    }

    #[test]
    fn test_convert_to_claude_format_multiple_events() {
        // Test hook with multiple events
        let mut hook_configs = HashMap::new();
        hook_configs.insert(
            "multi-event-hook".to_string(),
            HookConfig {
                events: vec![HookEvent::PreToolUse, HookEvent::PostToolUse],
                matcher: Some(".*".to_string()),
                hook_type: "command".to_string(),
                command: "echo 'tool event'".to_string(),
                timeout: Some(5000),
                description: None,
            },
        );

        let result = convert_to_claude_format(hook_configs).unwrap();

        // Should appear in both events
        assert!(result.get("PreToolUse").is_some());
        assert!(result.get("PostToolUse").is_some());

        let pre_tool = result.get("PreToolUse").unwrap().as_array().unwrap();
        let post_tool = result.get("PostToolUse").unwrap().as_array().unwrap();

        assert_eq!(pre_tool.len(), 1);
        assert_eq!(post_tool.len(), 1);

        // Both should have the matcher
        assert_eq!(pre_tool[0].get("matcher").unwrap().as_str().unwrap(), ".*");
        assert_eq!(post_tool[0].get("matcher").unwrap().as_str().unwrap(), ".*");
    }

    #[test]
    fn test_convert_to_claude_format_deduplication() {
        // Test deduplication of identical session hooks
        let mut hook_configs = HashMap::new();

        // Add two identical SessionStart hooks
        hook_configs.insert(
            "hook1".to_string(),
            HookConfig {
                events: vec![HookEvent::SessionStart],
                matcher: None,
                hook_type: "command".to_string(),
                command: "agpm update".to_string(),
                timeout: None,
                description: None,
            },
        );
        hook_configs.insert(
            "hook2".to_string(),
            HookConfig {
                events: vec![HookEvent::SessionStart],
                matcher: None,
                hook_type: "command".to_string(),
                command: "agpm update".to_string(), // Same command
                timeout: None,
                description: None,
            },
        );

        let result = convert_to_claude_format(hook_configs).unwrap();
        let session_start = result.get("SessionStart").unwrap().as_array().unwrap();

        // Should have only one group
        assert_eq!(session_start.len(), 1);

        // That group should have only one hook (deduplicated)
        let hooks = session_start[0].get("hooks").unwrap().as_array().unwrap();
        assert_eq!(hooks.len(), 1);
        assert_eq!(hooks[0].get("command").unwrap().as_str().unwrap(), "agpm update");
    }

    #[test]
    fn test_convert_to_claude_format_different_matchers() {
        // Test hooks with different matchers should be in separate groups
        let mut hook_configs = HashMap::new();

        hook_configs.insert(
            "bash-hook".to_string(),
            HookConfig {
                events: vec![HookEvent::PreToolUse],
                matcher: Some("Bash".to_string()),
                hook_type: "command".to_string(),
                command: "echo 'bash tool'".to_string(),
                timeout: None,
                description: None,
            },
        );
        hook_configs.insert(
            "write-hook".to_string(),
            HookConfig {
                events: vec![HookEvent::PreToolUse],
                matcher: Some("Write".to_string()),
                hook_type: "command".to_string(),
                command: "echo 'write tool'".to_string(),
                timeout: None,
                description: None,
            },
        );

        let result = convert_to_claude_format(hook_configs).unwrap();
        let pre_tool = result.get("PreToolUse").unwrap().as_array().unwrap();

        // Should have two separate groups
        assert_eq!(pre_tool.len(), 2);

        // Find the groups by matcher
        let bash_group = pre_tool
            .iter()
            .find(|g| g.get("matcher").and_then(|m| m.as_str()) == Some("Bash"))
            .unwrap();
        let write_group = pre_tool
            .iter()
            .find(|g| g.get("matcher").and_then(|m| m.as_str()) == Some("Write"))
            .unwrap();

        assert!(bash_group.get("hooks").unwrap().as_array().unwrap().len() == 1);
        assert!(write_group.get("hooks").unwrap().as_array().unwrap().len() == 1);
    }

    #[test]
    fn test_convert_to_claude_format_same_matcher() {
        // Test hooks with same matcher should be in same group
        let mut hook_configs = HashMap::new();

        hook_configs.insert(
            "hook1".to_string(),
            HookConfig {
                events: vec![HookEvent::PreToolUse],
                matcher: Some("Bash".to_string()),
                hook_type: "command".to_string(),
                command: "echo 'first'".to_string(),
                timeout: None,
                description: None,
            },
        );
        hook_configs.insert(
            "hook2".to_string(),
            HookConfig {
                events: vec![HookEvent::PreToolUse],
                matcher: Some("Bash".to_string()), // Same matcher
                hook_type: "command".to_string(),
                command: "echo 'second'".to_string(),
                timeout: None,
                description: None,
            },
        );

        let result = convert_to_claude_format(hook_configs).unwrap();
        let pre_tool = result.get("PreToolUse").unwrap().as_array().unwrap();

        // Should have only one group
        assert_eq!(pre_tool.len(), 1);

        // That group should have both hooks
        let hooks = pre_tool[0].get("hooks").unwrap().as_array().unwrap();
        assert_eq!(hooks.len(), 2);
        assert_eq!(pre_tool[0].get("matcher").unwrap().as_str().unwrap(), "Bash");
    }

    #[test]
    fn test_convert_to_claude_format_empty() {
        // Test empty hook configs
        let hook_configs = HashMap::new();
        let result = convert_to_claude_format(hook_configs).unwrap();

        assert_eq!(result.as_object().unwrap().len(), 0);
    }

    #[test]
    fn test_convert_to_claude_format_other_event() {
        // Test unknown/future event type
        let mut hook_configs = HashMap::new();
        hook_configs.insert(
            "future-hook".to_string(),
            HookConfig {
                events: vec![HookEvent::Other("FutureEvent".to_string())],
                matcher: None,
                hook_type: "command".to_string(),
                command: "echo 'future event'".to_string(),
                timeout: None,
                description: None,
            },
        );

        let result = convert_to_claude_format(hook_configs).unwrap();
        let expected = serde_json::json!({
            "FutureEvent": [
                {
                    "hooks": [
                        {
                            "type": "command",
                            "command": "echo 'future event'",
                            "timeout": null
                        }
                    ]
                }
            ]
        });

        assert_eq!(result, expected);
    }

    #[test]
    fn test_hook_event_other_serialization() {
        // Test that Other variant serializes/deserializes correctly
        let other_event = HookEvent::Other("CustomEvent".to_string());
        let json = serde_json::to_string(&other_event).unwrap();
        assert_eq!(json, r#""CustomEvent""#);

        let parsed: HookEvent = serde_json::from_str(&json).unwrap();
        if let HookEvent::Other(event_name) = parsed {
            assert_eq!(event_name, "CustomEvent");
        } else {
            panic!("Expected Other variant");
        }
    }
}
