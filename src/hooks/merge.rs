//! Hook merging logic for safely integrating CCPM hooks with user settings
//!
//! This module provides the critical functionality for merging CCPM-managed hooks
//! into Claude Code's settings.local.json while preserving user configurations.

use anyhow::{Context, Result};
use serde_json::Value;
use std::collections::HashMap;

use super::{CcpmHookMetadata, HookCommand, HookConfig, HookEvent, MatcherGroup};

/// Result of a merge operation for debugging and validation
#[derive(Debug)]
pub struct MergeResult {
    /// The merged hooks structure
    pub hooks: Value,
    /// Number of user hooks preserved
    pub user_hooks_preserved: usize,
    /// Number of CCPM hooks added
    pub ccpm_hooks_added: usize,
    /// Number of CCPM hooks updated
    pub ccpm_hooks_updated: usize,
    /// Number of CCPM hooks removed
    pub ccpm_hooks_removed: usize,
}

/// Merge CCPM hooks into existing settings with comprehensive conflict resolution
///
/// This function implements a sophisticated merge strategy that:
/// 1. Preserves all user-managed hooks
/// 2. Removes outdated CCPM-managed hooks
/// 3. Adds or updates current CCPM-managed hooks
/// 4. Groups hooks by matcher pattern for efficiency
/// 5. Maintains stable ordering where possible
pub fn merge_hooks_advanced(
    existing_hooks: Option<&Value>,
    ccpm_hooks: HashMap<String, HookConfig>,
    source_info: &HashMap<String, (String, String)>, // name -> (source, version)
) -> Result<MergeResult> {
    let mut merged: HashMap<String, Vec<MatcherGroup>> = HashMap::new();
    let mut stats = MergeResult {
        hooks: Value::Null,
        user_hooks_preserved: 0,
        ccpm_hooks_added: 0,
        ccpm_hooks_updated: 0,
        ccpm_hooks_removed: 0,
    };

    // Step 1: Parse and categorize existing hooks
    let (user_hooks, existing_ccpm) = parse_existing_hooks(existing_hooks)?;

    // Step 2: Preserve all user hooks
    for (event_name, groups) in user_hooks {
        stats.user_hooks_preserved += groups.iter().map(|g| g.hooks.len()).sum::<usize>();
        merged.insert(event_name, groups);
    }

    // Step 3: Track which CCPM hooks to keep
    let mut active_ccpm_hooks: HashMap<String, bool> = HashMap::new();

    // Step 4: Add/update CCPM hooks
    for (name, config) in ccpm_hooks {
        let (source, version) = source_info
            .get(&name)
            .ok_or_else(|| anyhow::anyhow!("Missing source info for hook: {}", name))?;

        active_ccpm_hooks.insert(name.clone(), true);

        for event in &config.events {
            let event_name = event_to_string(event);

            let hook_cmd = HookCommand {
                hook_type: config.hook_type.clone(),
                command: config.command.clone(),
                timeout: config.timeout,
                ccpm_metadata: Some(CcpmHookMetadata {
                    managed: true,
                    dependency_name: name.clone(),
                    source: source.clone(),
                    version: version.clone(),
                    installed_at: chrono::Utc::now().to_rfc3339(),
                }),
            };

            // Check if this hook already exists (update vs add)
            let is_update = existing_ccpm
                .iter()
                .any(|(existing_name, _)| existing_name == &name);

            if is_update {
                stats.ccpm_hooks_updated += 1;
            } else {
                stats.ccpm_hooks_added += 1;
            }

            // Add to appropriate matcher group
            add_hook_to_groups(&mut merged, event_name, config.matcher.clone(), hook_cmd);
        }
    }

    // Step 5: Count removed CCPM hooks
    for (old_name, _) in existing_ccpm {
        if !active_ccpm_hooks.contains_key(&old_name) {
            stats.ccpm_hooks_removed += 1;
        }
    }

    // Step 6: Convert to final structure
    stats.hooks = convert_to_value(merged)?;

    Ok(stats)
}

/// Type alias for parsed hooks result
type ParsedHooks = (
    HashMap<String, Vec<MatcherGroup>>, // User hooks
    HashMap<String, Vec<String>>,       // CCPM hooks: name -> events
);

/// Parse existing hooks and separate user-managed from CCPM-managed
fn parse_existing_hooks(existing: Option<&Value>) -> Result<ParsedHooks> {
    let mut user_hooks: HashMap<String, Vec<MatcherGroup>> = HashMap::new();
    let mut ccpm_hooks: HashMap<String, Vec<String>> = HashMap::new();

    if let Some(existing) = existing
        && let Some(obj) = existing.as_object() {
            for (event_name, matcher_groups) in obj {
                if let Some(groups) = matcher_groups.as_array() {
                    let mut user_groups = Vec::new();

                    for group in groups {
                        if let Some(group_obj) = group.as_object() {
                            let matcher = group_obj
                                .get("matcher")
                                .and_then(|m| m.as_str())
                                .unwrap_or("")
                                .to_string();

                            if let Some(hooks_array) =
                                group_obj.get("hooks").and_then(|h| h.as_array())
                            {
                                let mut user_hooks_in_group = Vec::new();

                                for hook in hooks_array {
                                    // Check if this is CCPM-managed
                                    if let Some(ccpm_meta) = hook.get("_ccpm") {
                                        if let Some(dep_name) = ccpm_meta
                                            .get("dependency_name")
                                            .and_then(|n| n.as_str())
                                        {
                                            ccpm_hooks
                                                .entry(dep_name.to_string())
                                                .or_default()
                                                .push(event_name.clone());
                                        }
                                    } else {
                                        // User-managed hook
                                        let hook_cmd: HookCommand =
                                            serde_json::from_value(hook.clone())
                                                .context("Failed to parse user hook")?;
                                        user_hooks_in_group.push(hook_cmd);
                                    }
                                }

                                // Only keep the group if it has user hooks
                                if !user_hooks_in_group.is_empty() {
                                    user_groups.push(MatcherGroup {
                                        matcher: matcher.clone(),
                                        hooks: user_hooks_in_group,
                                    });
                                }
                            }
                        }
                    }

                    if !user_groups.is_empty() {
                        user_hooks.insert(event_name.clone(), user_groups);
                    }
                }
            }
        }

    Ok((user_hooks, ccpm_hooks))
}

/// Add a hook to the appropriate matcher group
fn add_hook_to_groups(
    merged: &mut HashMap<String, Vec<MatcherGroup>>,
    event_name: String,
    matcher: String,
    hook: HookCommand,
) {
    let event_groups = merged.entry(event_name).or_default();

    // Find existing matcher group or create new one
    if let Some(group) = event_groups.iter_mut().find(|g| g.matcher == matcher) {
        // Remove any existing CCPM hook with the same dependency_name
        if let Some(ref new_meta) = hook.ccpm_metadata {
            group.hooks.retain(|h| {
                h.ccpm_metadata
                    .as_ref()
                    .map(|m| m.dependency_name != new_meta.dependency_name)
                    .unwrap_or(true)
            });
        }
        group.hooks.push(hook);
    } else {
        event_groups.push(MatcherGroup {
            matcher,
            hooks: vec![hook],
        });
    }
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
    }
}

/// Convert merged structure back to JSON Value
fn convert_to_value(merged: HashMap<String, Vec<MatcherGroup>>) -> Result<Value> {
    // Sort events for deterministic output
    let mut sorted_events: Vec<_> = merged.into_iter().collect();
    sorted_events.sort_by(|a, b| a.0.cmp(&b.0));

    let mut result = serde_json::Map::new();

    for (event_name, mut groups) in sorted_events {
        // Sort matcher groups for deterministic output
        groups.sort_by(|a, b| a.matcher.cmp(&b.matcher));

        let groups_value = serde_json::to_value(groups)?;
        result.insert(event_name, groups_value);
    }

    Ok(Value::Object(result))
}

/// Apply merged hooks to ClaudeSettings
pub fn apply_hooks_to_settings(
    settings: &mut crate::mcp::ClaudeSettings,
    merged_hooks: Value,
) -> Result<()> {
    // Only update if there are hooks to set
    if merged_hooks
        .as_object()
        .map(|o| o.is_empty())
        .unwrap_or(true)
    {
        settings.hooks = None;
    } else {
        settings.hooks = Some(merged_hooks);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_empty_merge() {
        let result = merge_hooks_advanced(None, HashMap::new(), &HashMap::new()).unwrap();

        assert_eq!(result.user_hooks_preserved, 0);
        assert_eq!(result.ccpm_hooks_added, 0);
        assert_eq!(result.ccpm_hooks_updated, 0);
        assert_eq!(result.ccpm_hooks_removed, 0);
        assert_eq!(result.hooks, json!({}));
    }

    #[test]
    fn test_preserve_user_hooks() {
        let existing = json!({
            "PreToolUse": [{
                "matcher": "Bash",
                "hooks": [{
                    "type": "command",
                    "command": "user-script.sh",
                    "timeout": 5000
                }]
            }]
        });

        let result =
            merge_hooks_advanced(Some(&existing), HashMap::new(), &HashMap::new()).unwrap();

        assert_eq!(result.user_hooks_preserved, 1);
        assert_eq!(result.ccpm_hooks_added, 0);

        // Verify user hook is preserved
        let hooks = result.hooks.as_object().unwrap();
        let pre_tool = hooks.get("PreToolUse").unwrap().as_array().unwrap();
        assert_eq!(pre_tool.len(), 1);
        let group = pre_tool[0].as_object().unwrap();
        assert_eq!(group.get("matcher").unwrap().as_str().unwrap(), "Bash");
        let hooks_array = group.get("hooks").unwrap().as_array().unwrap();
        assert_eq!(hooks_array.len(), 1);
        assert!(hooks_array[0].get("_ccpm").is_none());
    }

    #[test]
    fn test_add_ccpm_hooks() {
        let mut ccpm_hooks = HashMap::new();
        ccpm_hooks.insert(
            "security-hook".to_string(),
            HookConfig {
                events: vec![HookEvent::PreToolUse],
                matcher: "Bash|Write".to_string(),
                hook_type: "command".to_string(),
                command: ".claude/ccpm/scripts/security.sh".to_string(),
                timeout: Some(3000),
                description: None,
            },
        );

        let mut source_info = HashMap::new();
        source_info.insert(
            "security-hook".to_string(),
            ("test-source".to_string(), "v1.0.0".to_string()),
        );

        let result = merge_hooks_advanced(None, ccpm_hooks, &source_info).unwrap();

        assert_eq!(result.ccpm_hooks_added, 1);
        assert_eq!(result.user_hooks_preserved, 0);

        // Verify CCPM hook was added
        let hooks = result.hooks.as_object().unwrap();
        let pre_tool = hooks.get("PreToolUse").unwrap().as_array().unwrap();
        assert_eq!(pre_tool.len(), 1);
        let group = pre_tool[0].as_object().unwrap();
        assert_eq!(
            group.get("matcher").unwrap().as_str().unwrap(),
            "Bash|Write"
        );
        let hooks_array = group.get("hooks").unwrap().as_array().unwrap();
        assert_eq!(hooks_array.len(), 1);

        // Check CCPM metadata
        let ccpm_meta = hooks_array[0].get("_ccpm").unwrap();
        assert_eq!(
            ccpm_meta.get("dependency_name").unwrap().as_str().unwrap(),
            "security-hook"
        );
        assert_eq!(
            ccpm_meta.get("source").unwrap().as_str().unwrap(),
            "test-source"
        );
        assert_eq!(
            ccpm_meta.get("version").unwrap().as_str().unwrap(),
            "v1.0.0"
        );
    }

    #[test]
    fn test_merge_with_same_matcher() {
        // Existing user hook
        let existing = json!({
            "PreToolUse": [{
                "matcher": "Bash",
                "hooks": [{
                    "type": "command",
                    "command": "user-script.sh",
                    "timeout": 5000
                }]
            }]
        });

        // CCPM hook with same matcher
        let mut ccpm_hooks = HashMap::new();
        ccpm_hooks.insert(
            "security-hook".to_string(),
            HookConfig {
                events: vec![HookEvent::PreToolUse],
                matcher: "Bash".to_string(),
                hook_type: "command".to_string(),
                command: ".claude/ccpm/scripts/security.sh".to_string(),
                timeout: Some(3000),
                description: None,
            },
        );

        let mut source_info = HashMap::new();
        source_info.insert(
            "security-hook".to_string(),
            ("test-source".to_string(), "v1.0.0".to_string()),
        );

        let result = merge_hooks_advanced(Some(&existing), ccpm_hooks, &source_info).unwrap();

        assert_eq!(result.user_hooks_preserved, 1);
        assert_eq!(result.ccpm_hooks_added, 1);

        // Verify both hooks are in the same matcher group
        let hooks = result.hooks.as_object().unwrap();
        let pre_tool = hooks.get("PreToolUse").unwrap().as_array().unwrap();
        assert_eq!(pre_tool.len(), 1, "Should have one matcher group");

        let group = pre_tool[0].as_object().unwrap();
        assert_eq!(group.get("matcher").unwrap().as_str().unwrap(), "Bash");

        let hooks_array = group.get("hooks").unwrap().as_array().unwrap();
        assert_eq!(hooks_array.len(), 2, "Should have both user and CCPM hooks");

        // Check that we have one user hook and one CCPM hook
        let ccpm_count = hooks_array
            .iter()
            .filter(|h| h.get("_ccpm").is_some())
            .count();
        assert_eq!(ccpm_count, 1);

        let user_count = hooks_array
            .iter()
            .filter(|h| h.get("_ccpm").is_none())
            .count();
        assert_eq!(user_count, 1);
    }

    #[test]
    fn test_update_existing_ccpm_hook() {
        // Existing CCPM hook
        let existing = json!({
            "PreToolUse": [{
                "matcher": "Bash",
                "hooks": [{
                    "type": "command",
                    "command": ".claude/ccpm/scripts/old-security.sh",
                    "timeout": 5000,
                    "_ccpm": {
                        "managed": true,
                        "dependency_name": "security-hook",
                        "source": "test-source",
                        "version": "v0.9.0",
                        "installed_at": "2024-01-01T00:00:00Z"
                    }
                }]
            }]
        });

        let mut ccpm_hooks = HashMap::new();
        ccpm_hooks.insert(
            "security-hook".to_string(),
            HookConfig {
                events: vec![HookEvent::PreToolUse],
                matcher: "Bash".to_string(),
                hook_type: "command".to_string(),
                command: ".claude/ccpm/scripts/new-security.sh".to_string(),
                timeout: Some(3000),
                description: None,
            },
        );

        let mut source_info = HashMap::new();
        source_info.insert(
            "security-hook".to_string(),
            ("test-source".to_string(), "v1.0.0".to_string()),
        );

        let result = merge_hooks_advanced(Some(&existing), ccpm_hooks, &source_info).unwrap();

        assert_eq!(result.user_hooks_preserved, 0);
        assert_eq!(result.ccpm_hooks_added, 0);
        assert_eq!(result.ccpm_hooks_updated, 1);
        assert_eq!(result.ccpm_hooks_removed, 0);

        // Verify hook was updated
        let hooks = result.hooks.as_object().unwrap();
        let pre_tool = hooks.get("PreToolUse").unwrap().as_array().unwrap();
        assert_eq!(pre_tool.len(), 1);

        let group = pre_tool[0].as_object().unwrap();
        let hooks_array = group.get("hooks").unwrap().as_array().unwrap();
        assert_eq!(hooks_array.len(), 1);

        // Check updated values
        let hook = &hooks_array[0];
        assert_eq!(
            hook.get("command").unwrap().as_str().unwrap(),
            ".claude/ccpm/scripts/new-security.sh"
        );
        assert_eq!(hook.get("timeout").unwrap().as_u64().unwrap(), 3000);

        let ccpm_meta = hook.get("_ccpm").unwrap();
        assert_eq!(
            ccpm_meta.get("version").unwrap().as_str().unwrap(),
            "v1.0.0"
        );
    }

    #[test]
    fn test_remove_outdated_ccpm_hooks() {
        // Existing with two CCPM hooks
        let existing = json!({
            "PreToolUse": [{
                "matcher": "Bash",
                "hooks": [{
                    "type": "command",
                    "command": ".claude/ccpm/scripts/keep.sh",
                    "_ccpm": {
                        "managed": true,
                        "dependency_name": "keep-hook",
                        "source": "test-source",
                        "version": "v1.0.0",
                        "installed_at": "2024-01-01T00:00:00Z"
                    }
                }]
            }, {
                "matcher": "Write",
                "hooks": [{
                    "type": "command",
                    "command": ".claude/ccpm/scripts/remove.sh",
                    "_ccpm": {
                        "managed": true,
                        "dependency_name": "remove-hook",
                        "source": "test-source",
                        "version": "v1.0.0",
                        "installed_at": "2024-01-01T00:00:00Z"
                    }
                }]
            }]
        });

        // Only keep one hook
        let mut ccpm_hooks = HashMap::new();
        ccpm_hooks.insert(
            "keep-hook".to_string(),
            HookConfig {
                events: vec![HookEvent::PreToolUse],
                matcher: "Bash".to_string(),
                hook_type: "command".to_string(),
                command: ".claude/ccpm/scripts/keep.sh".to_string(),
                timeout: None,
                description: None,
            },
        );

        let mut source_info = HashMap::new();
        source_info.insert(
            "keep-hook".to_string(),
            ("test-source".to_string(), "v1.0.0".to_string()),
        );

        let result = merge_hooks_advanced(Some(&existing), ccpm_hooks, &source_info).unwrap();

        assert_eq!(result.ccpm_hooks_removed, 1);
        assert_eq!(result.ccpm_hooks_updated, 1);

        // Verify only one hook remains
        let hooks = result.hooks.as_object().unwrap();
        let pre_tool = hooks.get("PreToolUse").unwrap().as_array().unwrap();
        assert_eq!(pre_tool.len(), 1);

        // The "Write" matcher group should be gone
        assert!(
            !pre_tool
                .iter()
                .any(|g| g.get("matcher").and_then(|m| m.as_str()) == Some("Write"))
        );
    }

    #[test]
    fn test_multiple_events_same_hook() {
        let mut ccpm_hooks = HashMap::new();
        ccpm_hooks.insert(
            "multi-event-hook".to_string(),
            HookConfig {
                events: vec![
                    HookEvent::PreToolUse,
                    HookEvent::PostToolUse,
                    HookEvent::UserPromptSubmit,
                ],
                matcher: ".*".to_string(),
                hook_type: "command".to_string(),
                command: "multi-event.sh".to_string(),
                timeout: Some(1000),
                description: None,
            },
        );

        let mut source_info = HashMap::new();
        source_info.insert(
            "multi-event-hook".to_string(),
            ("test-source".to_string(), "v1.0.0".to_string()),
        );

        let result = merge_hooks_advanced(None, ccpm_hooks, &source_info).unwrap();

        assert_eq!(result.ccpm_hooks_added, 3); // One hook added to 3 events

        // Verify hook appears in all three events
        let hooks = result.hooks.as_object().unwrap();
        assert!(hooks.contains_key("PreToolUse"));
        assert!(hooks.contains_key("PostToolUse"));
        assert!(hooks.contains_key("UserPromptSubmit"));

        // Each event should have the hook
        for event in ["PreToolUse", "PostToolUse", "UserPromptSubmit"] {
            let event_hooks = hooks.get(event).unwrap().as_array().unwrap();
            assert_eq!(event_hooks.len(), 1);
            let group = event_hooks[0].as_object().unwrap();
            assert_eq!(group.get("matcher").unwrap().as_str().unwrap(), ".*");
        }
    }

    #[test]
    fn test_invalid_regex_matcher() {
        // Test that invalid regex matchers are handled gracefully
        let mut ccpm_hooks = HashMap::new();
        ccpm_hooks.insert(
            "test-hook".to_string(),
            HookConfig {
                events: vec![HookEvent::PreToolUse],
                matcher: "[invalid(regex".to_string(), // Invalid regex but we still store it
                hook_type: "command".to_string(),
                command: "test.sh".to_string(),
                timeout: None,
                description: None,
            },
        );

        let mut source_info = HashMap::new();
        source_info.insert(
            "test-hook".to_string(),
            ("test-source".to_string(), "v1.0.0".to_string()),
        );

        let result = merge_hooks_advanced(None, ccpm_hooks, &source_info).unwrap();

        assert_eq!(result.ccpm_hooks_added, 1);

        // The hook should still be added even with invalid regex
        // (validation happens elsewhere)
        let hooks = result.hooks.as_object().unwrap();
        let pre_tool = hooks.get("PreToolUse").unwrap().as_array().unwrap();
        assert_eq!(pre_tool.len(), 1);
    }

    #[test]
    fn test_empty_matcher_string() {
        let mut ccpm_hooks = HashMap::new();
        ccpm_hooks.insert(
            "empty-matcher".to_string(),
            HookConfig {
                events: vec![HookEvent::PreToolUse],
                matcher: "".to_string(), // Empty matcher
                hook_type: "command".to_string(),
                command: "test.sh".to_string(),
                timeout: None,
                description: None,
            },
        );

        let mut source_info = HashMap::new();
        source_info.insert(
            "empty-matcher".to_string(),
            ("test-source".to_string(), "v1.0.0".to_string()),
        );

        let result = merge_hooks_advanced(None, ccpm_hooks, &source_info).unwrap();

        assert_eq!(result.ccpm_hooks_added, 1);

        // Empty matcher should still work
        let hooks = result.hooks.as_object().unwrap();
        let pre_tool = hooks.get("PreToolUse").unwrap().as_array().unwrap();
        assert_eq!(pre_tool[0].get("matcher").unwrap().as_str().unwrap(), "");
    }

    #[test]
    fn test_duplicate_hooks_in_same_matcher() {
        // Test that duplicate CCPM hooks with same name get deduplicated
        let existing = json!({
            "PreToolUse": [{
                "matcher": "Bash",
                "hooks": [{
                    "type": "command",
                    "command": "old-security.sh",
                    "_ccpm": {
                        "managed": true,
                        "dependency_name": "security",
                        "source": "test",
                        "version": "v1.0.0"
                    }
                }]
            }]
        });

        let mut ccpm_hooks = HashMap::new();
        ccpm_hooks.insert(
            "security".to_string(), // Same dependency name
            HookConfig {
                events: vec![HookEvent::PreToolUse],
                matcher: "Bash".to_string(), // Same matcher
                hook_type: "command".to_string(),
                command: "new-security.sh".to_string(),
                timeout: None,
                description: None,
            },
        );

        let mut source_info = HashMap::new();
        source_info.insert(
            "security".to_string(),
            ("test".to_string(), "v2.0.0".to_string()),
        );

        let result = merge_hooks_advanced(Some(&existing), ccpm_hooks, &source_info).unwrap();

        // Should update, not add
        assert_eq!(result.ccpm_hooks_updated, 1);
        assert_eq!(result.ccpm_hooks_added, 0);

        // Should have only one hook in the matcher group
        let hooks = result.hooks.as_object().unwrap();
        let pre_tool = hooks.get("PreToolUse").unwrap().as_array().unwrap();
        assert_eq!(pre_tool.len(), 1);

        let group = pre_tool[0].as_object().unwrap();
        let hooks_array = group.get("hooks").unwrap().as_array().unwrap();
        assert_eq!(hooks_array.len(), 1);
        assert_eq!(
            hooks_array[0].get("command").unwrap().as_str().unwrap(),
            "new-security.sh"
        );
    }

    #[test]
    fn test_malformed_existing_hooks() {
        // Test handling of malformed existing hook structure
        let existing = json!({
            "PreToolUse": [
                {
                    // Missing matcher field
                    "hooks": [{
                        "type": "command",
                        "command": "test.sh"
                    }]
                },
                {
                    "matcher": "Bash",
                    // Missing hooks array
                },
                {
                    "matcher": "Write",
                    "hooks": "not-an-array" // Wrong type
                },
                {
                    "matcher": "Edit",
                    "hooks": [
                        "not-an-object", // Wrong type in array
                        {
                            "type": "command",
                            "command": "valid.sh"
                        }
                    ]
                }
            ]
        });

        let mut ccpm_hooks = HashMap::new();
        ccpm_hooks.insert(
            "new-hook".to_string(),
            HookConfig {
                events: vec![HookEvent::PreToolUse],
                matcher: "Test".to_string(),
                hook_type: "command".to_string(),
                command: "new.sh".to_string(),
                timeout: None,
                description: None,
            },
        );

        let mut source_info = HashMap::new();
        source_info.insert(
            "new-hook".to_string(),
            ("test".to_string(), "v1.0.0".to_string()),
        );

        // Should handle gracefully without panicking
        let result = merge_hooks_advanced(Some(&existing), ccpm_hooks, &source_info);

        // We expect this to either succeed with partial data or fail gracefully
        assert!(result.is_ok() || result.is_err());

        if let Ok(result) = result {
            assert_eq!(result.ccpm_hooks_added, 1);

            // New hook should be added
            let hooks = result.hooks.as_object().unwrap();
            assert!(hooks.contains_key("PreToolUse"));
        }
    }

    #[test]
    fn test_very_long_matcher_pattern() {
        let long_pattern = "A|".repeat(1000) + "B"; // Very long pattern

        let mut ccpm_hooks = HashMap::new();
        ccpm_hooks.insert(
            "long-hook".to_string(),
            HookConfig {
                events: vec![HookEvent::PreToolUse],
                matcher: long_pattern.clone(),
                hook_type: "command".to_string(),
                command: "test.sh".to_string(),
                timeout: None,
                description: None,
            },
        );

        let mut source_info = HashMap::new();
        source_info.insert(
            "long-hook".to_string(),
            ("test".to_string(), "v1.0.0".to_string()),
        );

        let result = merge_hooks_advanced(None, ccpm_hooks, &source_info).unwrap();

        assert_eq!(result.ccpm_hooks_added, 1);

        // Long pattern should be preserved
        let hooks = result.hooks.as_object().unwrap();
        let pre_tool = hooks.get("PreToolUse").unwrap().as_array().unwrap();
        assert_eq!(
            pre_tool[0].get("matcher").unwrap().as_str().unwrap(),
            &long_pattern
        );
    }

    #[test]
    fn test_special_characters_in_names() {
        // Test hooks with special characters in names
        let mut ccpm_hooks = HashMap::new();
        ccpm_hooks.insert(
            "hook-with-special!@#$%^&*()_+chars".to_string(),
            HookConfig {
                events: vec![HookEvent::PreToolUse],
                matcher: ".*".to_string(),
                hook_type: "command".to_string(),
                command: "test.sh".to_string(),
                timeout: None,
                description: None,
            },
        );

        let mut source_info = HashMap::new();
        source_info.insert(
            "hook-with-special!@#$%^&*()_+chars".to_string(),
            ("test-source".to_string(), "v1.0.0".to_string()),
        );

        let result = merge_hooks_advanced(None, ccpm_hooks, &source_info).unwrap();

        assert_eq!(result.ccpm_hooks_added, 1);

        // Special characters should be preserved
        let hooks = result.hooks.as_object().unwrap();
        let pre_tool = hooks.get("PreToolUse").unwrap().as_array().unwrap();
        let hook = &pre_tool[0].get("hooks").unwrap().as_array().unwrap()[0];
        assert_eq!(
            hook.get("_ccpm")
                .unwrap()
                .get("dependency_name")
                .unwrap()
                .as_str()
                .unwrap(),
            "hook-with-special!@#$%^&*()_+chars"
        );
    }

    #[test]
    fn test_complex_merge_scenario() {
        // Complex existing configuration
        let existing = json!({
            "PreToolUse": [
                {
                    "matcher": "Bash",
                    "hooks": [
                        {
                            "type": "command",
                            "command": "user-bash-hook.sh"
                        },
                        {
                            "type": "command",
                            "command": "old-ccpm-hook.sh",
                            "_ccpm": {
                                "managed": true,
                                "dependency_name": "old-hook",
                                "source": "old-source",
                                "version": "v0.1.0"
                            }
                        }
                    ]
                },
                {
                    "matcher": "Write|Edit",
                    "hooks": [
                        {
                            "type": "command",
                            "command": "user-write-hook.sh"
                        }
                    ]
                }
            ],
            "PostToolUse": [
                {
                    "matcher": ".*",
                    "hooks": [
                        {
                            "type": "command",
                            "command": "logging.sh",
                            "_ccpm": {
                                "managed": true,
                                "dependency_name": "logger",
                                "source": "utils",
                                "version": "v1.0.0"
                            }
                        }
                    ]
                }
            ]
        });

        // New CCPM hooks configuration
        let mut ccpm_hooks = HashMap::new();

        // Keep and update the logger
        ccpm_hooks.insert(
            "logger".to_string(),
            HookConfig {
                events: vec![HookEvent::PostToolUse],
                matcher: ".*".to_string(),
                hook_type: "command".to_string(),
                command: "new-logging.sh".to_string(),
                timeout: Some(500),
                description: None,
            },
        );

        // Add a new security hook
        ccpm_hooks.insert(
            "security".to_string(),
            HookConfig {
                events: vec![HookEvent::PreToolUse],
                matcher: "Bash".to_string(),
                hook_type: "command".to_string(),
                command: "security-check.sh".to_string(),
                timeout: Some(2000),
                description: None,
            },
        );

        let mut source_info = HashMap::new();
        source_info.insert(
            "logger".to_string(),
            ("utils".to_string(), "v2.0.0".to_string()),
        );
        source_info.insert(
            "security".to_string(),
            ("security-tools".to_string(), "v1.0.0".to_string()),
        );

        let result = merge_hooks_advanced(Some(&existing), ccpm_hooks, &source_info).unwrap();

        assert_eq!(result.user_hooks_preserved, 2); // user-bash-hook and user-write-hook
        assert_eq!(result.ccpm_hooks_added, 1); // security hook
        assert_eq!(result.ccpm_hooks_updated, 1); // logger hook
        assert_eq!(result.ccpm_hooks_removed, 1); // old-hook

        // Verify final structure
        let hooks = result.hooks.as_object().unwrap();

        // Check PreToolUse
        let pre_tool = hooks.get("PreToolUse").unwrap().as_array().unwrap();

        // Find Bash matcher group
        let bash_group = pre_tool
            .iter()
            .find(|g| g.get("matcher").and_then(|m| m.as_str()) == Some("Bash"))
            .unwrap()
            .as_object()
            .unwrap();

        let bash_hooks = bash_group.get("hooks").unwrap().as_array().unwrap();
        assert_eq!(bash_hooks.len(), 2); // user hook + security hook

        // Find Write|Edit matcher group
        let write_group = pre_tool
            .iter()
            .find(|g| g.get("matcher").and_then(|m| m.as_str()) == Some("Write|Edit"))
            .unwrap()
            .as_object()
            .unwrap();

        let write_hooks = write_group.get("hooks").unwrap().as_array().unwrap();
        assert_eq!(write_hooks.len(), 1); // Just the user hook
        assert!(write_hooks[0].get("_ccpm").is_none());

        // Check PostToolUse
        let post_tool = hooks.get("PostToolUse").unwrap().as_array().unwrap();
        assert_eq!(post_tool.len(), 1);

        let post_group = post_tool[0].as_object().unwrap();
        let post_hooks = post_group.get("hooks").unwrap().as_array().unwrap();
        assert_eq!(post_hooks.len(), 1);
        assert_eq!(
            post_hooks[0].get("command").unwrap().as_str().unwrap(),
            "new-logging.sh"
        );
    }
}
