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
    /// Whether this hook is managed by CCPM (true) or manually configured (false)
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
/// A HashMap mapping hook names to their configurations. If the directory
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
/// use ccpm::hooks::load_hook_configs;
/// use std::path::Path;
///
/// # fn example() -> anyhow::Result<()> {
/// let hooks_dir = Path::new(".claude/ccpm/hooks");
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

// Re-export commonly used merge functions
pub use merge::{apply_hooks_to_settings, merge_hooks_advanced, MergeResult};

/// Install hooks from manifest to .claude/settings.local.json
///
/// This function:
/// 1. Loads hook JSON files from .claude/ccpm/hooks/
/// 2. Merges them into .claude/settings.local.json
/// 3. Preserves user-managed hooks
pub async fn install_hooks(
    manifest: &crate::manifest::Manifest,
    project_root: &Path,
) -> Result<Vec<crate::lockfile::LockedResource>> {
    if manifest.hooks.is_empty() {
        return Ok(Vec::new());
    }

    let claude_dir = project_root.join(".claude");
    let hooks_dir = project_root.join(&manifest.target.hooks);
    let settings_path = claude_dir.join("settings.local.json");

    // Ensure directories exist
    crate::utils::fs::ensure_dir(&hooks_dir)?;
    crate::utils::fs::ensure_dir(&claude_dir)?;

    // Load hook configurations from JSON files
    let hook_configs = load_hook_configs(&hooks_dir)?;

    // Build source info for hooks
    let mut source_info = HashMap::new();
    for (name, dep) in &manifest.hooks {
        match dep {
            crate::manifest::ResourceDependency::Detailed(detailed) => {
                if let Some(source) = &detailed.source {
                    let version = detailed
                        .version
                        .as_ref()
                        .or(detailed.branch.as_ref())
                        .or(detailed.rev.as_ref())
                        .cloned()
                        .unwrap_or_else(|| "latest".to_string());
                    source_info.insert(name.clone(), (source.clone(), version));
                }
            }
            crate::manifest::ResourceDependency::Simple(_) => {
                // Local dependencies don't have source info
                source_info.insert(name.clone(), ("local".to_string(), "latest".to_string()));
            }
        }
    }

    // Load existing settings
    let mut settings = crate::mcp::ClaudeSettings::load_or_default(&settings_path)?;

    // Merge hooks
    let merge_result = merge_hooks_advanced(settings.hooks.as_ref(), hook_configs, &source_info)?;

    // Apply merged hooks to settings
    apply_hooks_to_settings(&mut settings, merge_result.hooks)?;

    // Save updated settings
    settings.save(&settings_path)?;

    println!(
        "✓ Configured {} hook(s) in .claude/settings.local.json",
        manifest.hooks.len()
    );

    // Build locked entries for the lockfile
    let locked_hooks: Vec<crate::lockfile::LockedResource> = manifest
        .hooks
        .iter()
        .map(|(name, dep)| {
            let installed_path = manifest.target.hooks.clone() + "/" + name + ".json";
            match dep {
                crate::manifest::ResourceDependency::Detailed(detailed) => {
                    crate::lockfile::LockedResource {
                        name: name.clone(),
                        source: detailed.source.clone(),
                        url: None,
                        path: detailed.path.clone(),
                        version: detailed
                            .version
                            .clone()
                            .or(detailed.branch.clone())
                            .or(detailed.rev.clone()),
                        resolved_commit: None,
                        checksum: String::new(),
                        installed_at: installed_path,
                    }
                }
                crate::manifest::ResourceDependency::Simple(path) => {
                    crate::lockfile::LockedResource {
                        name: name.clone(),
                        source: None,
                        url: None,
                        path: path.clone(),
                        version: None,
                        resolved_commit: None,
                        checksum: String::new(),
                        installed_at: installed_path,
                    }
                }
            }
        })
        .collect();

    Ok(locked_hooks)
}

/// Validate a hook configuration for correctness and safety.
///
/// Performs comprehensive validation of a hook configuration including:
/// - Event list validation (must have at least one event)
/// - Regex pattern syntax validation for the matcher
/// - Hook type validation (only "command" type is supported)
/// - Script existence validation for CCPM-managed scripts
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
/// - Referenced script file doesn't exist (for CCPM-managed scripts)
///
/// # Examples
///
/// ```rust,no_run
/// use ccpm::hooks::{validate_hook_config, HookConfig, HookEvent};
/// use std::path::Path;
///
/// # fn example() -> anyhow::Result<()> {
/// let config = HookConfig {
///     events: vec![HookEvent::PreToolUse],
///     matcher: "Bash|Write".to_string(),
///     hook_type: "command".to_string(),
///     command: "echo 'validation'".to_string(),
///     timeout: Some(5000),
///     description: None,
/// };
///
/// let hook_file = Path::new(".claude/ccpm/hooks/test.json");
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
        // If script_path is the hook file (e.g., .claude/ccpm/hooks/test.json),
        // we need to go up to the project root:
        // test.json -> hooks/ -> ccpm/ -> .claude/ -> project_root
        script_path
            .parent() // hooks/
            .and_then(|p| p.parent()) // ccpm/
            .and_then(|p| p.parent()) // .claude/
            .and_then(|p| p.parent()) // project root
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
            matcher: "Bash|Write".to_string(),
            hook_type: "command".to_string(),
            command: ".claude/ccpm/scripts/security-check.sh".to_string(),
            timeout: Some(5000),
            description: Some("Security validation".to_string()),
        };

        let json = serde_json::to_string_pretty(&config).unwrap();
        let parsed: HookConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.events.len(), 2);
        assert_eq!(parsed.matcher, "Bash|Write");
        assert_eq!(parsed.timeout, Some(5000));
        assert_eq!(parsed.description, Some("Security validation".to_string()));
    }

    #[test]
    fn test_hook_config_minimal() {
        // Test minimal config without optional fields
        let config = HookConfig {
            events: vec![HookEvent::UserPromptSubmit],
            matcher: ".*".to_string(),
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
        let metadata = CcpmHookMetadata {
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
            ccpm_metadata: Some(metadata.clone()),
        };

        let json = serde_json::to_string(&command).unwrap();
        let parsed: HookCommand = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.hook_type, "command");
        assert_eq!(parsed.command, "test.sh");
        assert_eq!(parsed.timeout, Some(3000));
        assert!(parsed.ccpm_metadata.is_some());
        let meta = parsed.ccpm_metadata.unwrap();
        assert!(meta.managed);
        assert_eq!(meta.dependency_name, "test-hook");
    }

    #[test]
    fn test_matcher_group_serialization() {
        let command = HookCommand {
            hook_type: "command".to_string(),
            command: "test.sh".to_string(),
            timeout: None,
            ccpm_metadata: None,
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
            matcher: ".*".to_string(),
            hook_type: "command".to_string(),
            command: "test1.sh".to_string(),
            timeout: None,
            description: None,
        };

        let config2 = HookConfig {
            events: vec![HookEvent::PostToolUse],
            matcher: "Write".to_string(),
            hook_type: "command".to_string(),
            command: "test2.sh".to_string(),
            timeout: Some(1000),
            description: Some("Test hook 2".to_string()),
        };

        fs::write(
            hooks_dir.join("test-hook1.json"),
            serde_json::to_string(&config1).unwrap(),
        )
        .unwrap();
        fs::write(
            hooks_dir.join("test-hook2.json"),
            serde_json::to_string(&config2).unwrap(),
        )
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
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Failed to parse hook config"));
    }

    #[test]
    fn test_validate_hook_config_empty_events() {
        let temp = tempdir().unwrap();

        let config = HookConfig {
            events: vec![], // Empty events
            matcher: ".*".to_string(),
            hook_type: "command".to_string(),
            command: "test.sh".to_string(),
            timeout: None,
            description: None,
        };

        let result = validate_hook_config(&config, temp.path());
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("at least one event"));
    }

    #[test]
    fn test_validate_hook_config_invalid_regex() {
        let temp = tempdir().unwrap();

        let config = HookConfig {
            events: vec![HookEvent::PreToolUse],
            matcher: "[invalid regex".to_string(), // Invalid regex
            hook_type: "command".to_string(),
            command: "test.sh".to_string(),
            timeout: None,
            description: None,
        };

        let result = validate_hook_config(&config, temp.path());
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Invalid regex pattern"));
    }

    #[test]
    fn test_validate_hook_config_unsupported_type() {
        let temp = tempdir().unwrap();

        let config = HookConfig {
            events: vec![HookEvent::PreToolUse],
            matcher: ".*".to_string(),
            hook_type: "webhook".to_string(), // Unsupported type
            command: "test.sh".to_string(),
            timeout: None,
            description: None,
        };

        let result = validate_hook_config(&config, temp.path());
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Only 'command' hook type"));
    }

    #[test]
    fn test_validate_hook_config_script_exists() {
        let temp = tempdir().unwrap();

        // Create the expected directory structure with script
        let claude_dir = temp.path().join(".claude").join("ccpm");
        let scripts_dir = claude_dir.join("scripts");
        let hooks_dir = claude_dir.join("hooks");
        fs::create_dir_all(&scripts_dir).unwrap();
        fs::create_dir_all(&hooks_dir).unwrap();

        let script_path = scripts_dir.join("test.sh");
        fs::write(&script_path, "#!/bin/bash\necho test").unwrap();

        let config = HookConfig {
            events: vec![HookEvent::PreToolUse],
            matcher: ".*".to_string(),
            hook_type: "command".to_string(),
            command: ".claude/ccpm/scripts/test.sh".to_string(),
            timeout: None,
            description: None,
        };

        // The hook file would be at .claude/ccpm/hooks/test.json
        // validate_hook_config goes up 3 levels from the hook path to find the project root
        let hook_json_path = hooks_dir.join("test.json");
        let result = validate_hook_config(&config, &hook_json_path);

        // Since the script exists at the expected location, this should succeed
        assert!(
            result.is_ok(),
            "Expected validation to succeed, but got: {:?}",
            result
        );
    }

    #[test]
    fn test_validate_hook_config_script_not_exists() {
        let temp = tempdir().unwrap();

        let config = HookConfig {
            events: vec![HookEvent::PreToolUse],
            matcher: ".*".to_string(),
            hook_type: "command".to_string(),
            command: ".claude/ccpm/scripts/nonexistent.sh".to_string(),
            timeout: None,
            description: None,
        };

        // Pass the hook file path
        let hook_path = temp
            .path()
            .join(".claude")
            .join("ccpm")
            .join("hooks")
            .join("test.json");
        let result = validate_hook_config(&config, &hook_path);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("non-existent script"));
    }

    #[test]
    fn test_validate_hook_config_non_claude_path() {
        let temp = tempdir().unwrap();

        // Test with a command that doesn't start with .claude/ccpm/scripts/
        let config = HookConfig {
            events: vec![HookEvent::PreToolUse],
            matcher: ".*".to_string(),
            hook_type: "command".to_string(),
            command: "/usr/bin/echo".to_string(), // Absolute path not in .claude
            timeout: None,
            description: None,
        };

        let result = validate_hook_config(&config, temp.path());
        // Should pass - we don't validate non-.claude paths
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_install_hooks_empty_manifest() {
        let temp = tempdir().unwrap();
        let manifest = crate::manifest::Manifest::default();

        let result = install_hooks(&manifest, temp.path()).await.unwrap();
        assert_eq!(result.len(), 0);
    }

    #[tokio::test]
    async fn test_install_hooks_with_hooks() {
        let temp = tempdir().unwrap();
        let hooks_dir = temp.path().join(".claude/ccpm/hooks");
        fs::create_dir_all(&hooks_dir).unwrap();

        // Create a hook JSON file
        let hook_config = HookConfig {
            events: vec![HookEvent::PreToolUse],
            matcher: "Bash".to_string(),
            hook_type: "command".to_string(),
            command: "test.sh".to_string(),
            timeout: Some(5000),
            description: Some("Test hook".to_string()),
        };

        fs::write(
            hooks_dir.join("test-hook.json"),
            serde_json::to_string(&hook_config).unwrap(),
        )
        .unwrap();

        // Create a manifest with hooks
        let mut manifest = crate::manifest::Manifest::default();
        manifest.hooks.insert(
            "test-hook".to_string(),
            crate::manifest::ResourceDependency::Detailed(crate::manifest::DetailedDependency {
                source: Some("community".to_string()),
                path: "hooks/test-hook.json".to_string(),
                version: Some("v1.0.0".to_string()),
                branch: None,
                rev: None,
                command: None,
                args: None,
                target: None,
                filename: None,
            }),
        );
        manifest.target.hooks = ".claude/ccpm/hooks".to_string();

        let result = install_hooks(&manifest, temp.path()).await.unwrap();
        assert_eq!(result.len(), 1);

        // Check that settings.local.json was created
        let settings_path = temp.path().join(".claude/settings.local.json");
        assert!(settings_path.exists());

        // Verify the locked resource
        let locked = &result[0];
        assert_eq!(locked.name, "test-hook");
        assert_eq!(locked.source, Some("community".to_string()));
        assert_eq!(locked.version, Some("v1.0.0".to_string()));
    }

    #[tokio::test]
    async fn test_install_hooks_simple_dependency() {
        let temp = tempdir().unwrap();
        let hooks_dir = temp.path().join(".claude/ccpm/hooks");
        fs::create_dir_all(&hooks_dir).unwrap();

        // Create a hook JSON file
        let hook_config = HookConfig {
            events: vec![HookEvent::UserPromptSubmit],
            matcher: ".*".to_string(),
            hook_type: "command".to_string(),
            command: "echo 'prompt submitted'".to_string(),
            timeout: None,
            description: None,
        };

        fs::write(
            hooks_dir.join("simple-hook.json"),
            serde_json::to_string(&hook_config).unwrap(),
        )
        .unwrap();

        // Create a manifest with a simple dependency
        let mut manifest = crate::manifest::Manifest::default();
        manifest.hooks.insert(
            "simple-hook".to_string(),
            crate::manifest::ResourceDependency::Simple("/path/to/hook.json".to_string()),
        );
        manifest.target.hooks = ".claude/ccpm/hooks".to_string();

        let result = install_hooks(&manifest, temp.path()).await.unwrap();
        assert_eq!(result.len(), 1);

        // Verify the locked resource for simple dependency
        let locked = &result[0];
        assert_eq!(locked.name, "simple-hook");
        assert_eq!(locked.source, None);
        assert_eq!(locked.path, "/path/to/hook.json");
        assert_eq!(locked.version, None);
    }

    #[tokio::test]
    async fn test_install_hooks_with_branch() {
        let temp = tempdir().unwrap();
        let hooks_dir = temp.path().join(".claude/ccpm/hooks");
        fs::create_dir_all(&hooks_dir).unwrap();

        // Create a manifest with a branch-based dependency
        let mut manifest = crate::manifest::Manifest::default();
        manifest.hooks.insert(
            "branch-hook".to_string(),
            crate::manifest::ResourceDependency::Detailed(crate::manifest::DetailedDependency {
                source: Some("upstream".to_string()),
                path: "hooks/branch.json".to_string(),
                version: None,
                branch: Some("main".to_string()),
                rev: None,
                command: None,
                args: None,
                target: None,
                filename: None,
            }),
        );
        manifest.target.hooks = ".claude/ccpm/hooks".to_string();

        let result = install_hooks(&manifest, temp.path()).await.unwrap();
        assert_eq!(result.len(), 1);

        let locked = &result[0];
        assert_eq!(locked.version, Some("main".to_string()));
    }

    #[tokio::test]
    async fn test_install_hooks_with_rev() {
        let temp = tempdir().unwrap();
        let hooks_dir = temp.path().join(".claude/ccpm/hooks");
        fs::create_dir_all(&hooks_dir).unwrap();

        // Create a manifest with a rev-based dependency
        let mut manifest = crate::manifest::Manifest::default();
        manifest.hooks.insert(
            "rev-hook".to_string(),
            crate::manifest::ResourceDependency::Detailed(crate::manifest::DetailedDependency {
                source: Some("upstream".to_string()),
                path: "hooks/rev.json".to_string(),
                version: None,
                branch: None,
                rev: Some("abc123".to_string()),
                command: None,
                args: None,
                target: None,
                filename: None,
            }),
        );
        manifest.target.hooks = ".claude/ccpm/hooks".to_string();

        let result = install_hooks(&manifest, temp.path()).await.unwrap();
        assert_eq!(result.len(), 1);

        let locked = &result[0];
        assert_eq!(locked.version, Some("abc123".to_string()));
    }
}
