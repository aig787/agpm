use anyhow::Result;
use serde_json::Value;
use std::fs;

mod common;
use common::TestProject;

#[tokio::test]
async fn test_hooks_install_and_format() -> Result<()> {
    let project = TestProject::new()?;

    // Create hook source repository
    let source_repo = project.create_source_repo("hooks")?;

    // Create hooks directory and add hooks directly (not using add_resource since it adds .md)
    let hooks_dir = source_repo.path.join("hooks");
    fs::create_dir_all(&hooks_dir)?;

    // Create SessionStart hook (no matcher)
    let session_hook = serde_json::json!({
        "events": ["SessionStart"],
        "type": "command",
        "command": "echo 'Session started'",
        "description": "Session start hook"
    });
    fs::write(
        hooks_dir.join("session-start.json"),
        serde_json::to_string_pretty(&session_hook)?,
    )?;

    // Create PreToolUse hook (with matcher)
    let pre_tool_hook = serde_json::json!({
        "events": ["PreToolUse"],
        "matcher": "Bash|Write",
        "type": "command",
        "command": "echo 'Before tool use'",
        "timeout": 5000,
        "description": "Pre-tool use hook"
    });
    fs::write(
        hooks_dir.join("pre-tool-use.json"),
        serde_json::to_string_pretty(&pre_tool_hook)?,
    )?;

    source_repo.commit_all("Add test hooks")?;
    let source_url = source_repo.bare_file_url(project.sources_path())?;

    // Create manifest with both hooks
    let manifest_content = format!(
        r#"
[sources]
hooks = "{}"

[hooks]
session-hook = {{ source = "hooks", path = "hooks/session-start.json" }}
tool-hook = {{ source = "hooks", path = "hooks/pre-tool-use.json" }}
"#,
        source_url
    );
    project.write_manifest(&manifest_content)?;

    // Run install
    let output = project.run_ccpm(&["install"])?;
    output.assert_success();
    output.assert_stdout_contains("✓ Configured 2 hook(s)");

    // Check settings.local.json has correct format
    let settings_path = project.project_path().join(".claude/settings.local.json");
    let settings_content = fs::read_to_string(&settings_path)?;
    let settings: Value = serde_json::from_str(&settings_content)?;

    // Verify structure
    let hooks = settings.get("hooks").expect("Should have hooks section");

    // Test SessionStart hook (no matcher)
    let session_start = hooks
        .get("SessionStart")
        .expect("Should have SessionStart")
        .as_array()
        .unwrap();
    assert_eq!(session_start.len(), 1);
    assert!(
        session_start[0].get("matcher").is_none(),
        "SessionStart should not have matcher"
    );

    let session_commands = session_start[0].get("hooks").unwrap().as_array().unwrap();
    assert_eq!(session_commands.len(), 1);
    assert_eq!(
        session_commands[0]
            .get("command")
            .unwrap()
            .as_str()
            .unwrap(),
        "echo 'Session started'"
    );

    // Test PreToolUse hook (with matcher)
    let pre_tool_use = hooks
        .get("PreToolUse")
        .expect("Should have PreToolUse")
        .as_array()
        .unwrap();
    assert_eq!(pre_tool_use.len(), 1);
    assert_eq!(
        pre_tool_use[0].get("matcher").unwrap().as_str().unwrap(),
        "Bash|Write"
    );

    let pre_tool_commands = pre_tool_use[0].get("hooks").unwrap().as_array().unwrap();
    assert_eq!(pre_tool_commands.len(), 1);
    assert_eq!(
        pre_tool_commands[0]
            .get("command")
            .unwrap()
            .as_str()
            .unwrap(),
        "echo 'Before tool use'"
    );
    assert_eq!(
        pre_tool_commands[0]
            .get("timeout")
            .unwrap()
            .as_u64()
            .unwrap(),
        5000
    );

    Ok(())
}

#[tokio::test]
async fn test_hooks_deduplication() -> Result<()> {
    let project = TestProject::new()?;

    // Create hook source repository
    let source_repo = project.create_source_repo("hooks")?;

    // Create hooks directory and add identical hooks
    let hooks_dir = source_repo.path.join("hooks");
    fs::create_dir_all(&hooks_dir)?;

    // Create identical SessionStart hook
    let session_hook = serde_json::json!({
        "events": ["SessionStart"],
        "type": "command",
        "command": "ccpm update",
        "description": "Update CCPM"
    });
    fs::write(
        hooks_dir.join("hook1.json"),
        serde_json::to_string_pretty(&session_hook)?,
    )?;
    fs::write(
        hooks_dir.join("hook2.json"),
        serde_json::to_string_pretty(&session_hook)?,
    )?;

    source_repo.commit_all("Add duplicate hooks")?;
    let source_url = source_repo.bare_file_url(project.sources_path())?;

    // Create manifest with both identical hooks
    let manifest_content = format!(
        r#"
[sources]
hooks = "{}"

[hooks]
first-hook = {{ source = "hooks", path = "hooks/hook1.json" }}
second-hook = {{ source = "hooks", path = "hooks/hook2.json" }}
"#,
        source_url
    );
    project.write_manifest(&manifest_content)?;

    // Run install
    let output = project.run_ccpm(&["install"])?;
    output.assert_success();
    output.assert_stdout_contains("✓ Configured 1 hook(s)"); // Deduplicated count

    // Check that hooks are deduplicated
    let settings_path = project.project_path().join(".claude/settings.local.json");
    let settings_content = fs::read_to_string(&settings_path)?;
    let settings: Value = serde_json::from_str(&settings_content)?;

    let hooks = settings.get("hooks").unwrap();
    let session_start = hooks.get("SessionStart").unwrap().as_array().unwrap();

    // Should have only one group
    assert_eq!(session_start.len(), 1);

    // That group should have only one hook (deduplicated)
    let hook_commands = session_start[0].get("hooks").unwrap().as_array().unwrap();
    assert_eq!(
        hook_commands.len(),
        1,
        "Identical hooks should be deduplicated"
    );
    assert_eq!(
        hook_commands[0].get("command").unwrap().as_str().unwrap(),
        "ccpm update"
    );

    Ok(())
}

#[tokio::test]
async fn test_hooks_unknown_event_type() -> Result<()> {
    let project = TestProject::new()?;

    // Create hook source repository
    let source_repo = project.create_source_repo("hooks")?;

    // Create hooks directory and add hook with unknown event type
    let hooks_dir = source_repo.path.join("hooks");
    fs::create_dir_all(&hooks_dir)?;

    // Create hook with unknown/future event type
    let future_hook = serde_json::json!({
        "events": ["FutureEvent"],
        "type": "command",
        "command": "echo 'future event'",
        "description": "Testing future event type"
    });
    fs::write(
        hooks_dir.join("future-hook.json"),
        serde_json::to_string_pretty(&future_hook)?,
    )?;

    source_repo.commit_all("Add future hook")?;
    let source_url = source_repo.bare_file_url(project.sources_path())?;

    // Create manifest with unknown event hook
    let manifest_content = format!(
        r#"
[sources]
hooks = "{}"

[hooks]
future-hook = {{ source = "hooks", path = "hooks/future-hook.json" }}
"#,
        source_url
    );
    project.write_manifest(&manifest_content)?;

    // Run install
    let output = project.run_ccpm(&["install"])?;
    output.assert_success();
    output.assert_stdout_contains("✓ Configured 1 hook(s)");

    // Check settings.local.json has the unknown event type
    let settings_path = project.project_path().join(".claude/settings.local.json");
    let settings_content = fs::read_to_string(&settings_path)?;
    let settings: Value = serde_json::from_str(&settings_content)?;

    let hooks = settings.get("hooks").expect("Should have hooks section");

    // Should have the FutureEvent
    let future_event = hooks
        .get("FutureEvent")
        .expect("Should have FutureEvent")
        .as_array()
        .unwrap();
    assert_eq!(future_event.len(), 1);

    // Should have no matcher for this event type
    assert!(
        future_event[0].get("matcher").is_none(),
        "FutureEvent should not have matcher"
    );

    let commands = future_event[0].get("hooks").unwrap().as_array().unwrap();
    assert_eq!(commands.len(), 1);
    assert_eq!(
        commands[0].get("command").unwrap().as_str().unwrap(),
        "echo 'future event'"
    );

    Ok(())
}

#[tokio::test]
async fn test_hooks_empty_no_message() -> Result<()> {
    let project = TestProject::new()?;

    // Create manifest with no hooks
    let manifest_content = r#"
[sources]
# No sources

[hooks]
# No hooks
"#;
    project.write_manifest(manifest_content)?;

    // Run install
    let output = project.run_ccpm(&["install"])?;
    output.assert_success();

    // Should NOT contain any hook configuration message
    assert!(!output.stdout.contains("Configured"));
    assert!(!output.stdout.contains("hook"));

    // Check that settings file either doesn't exist or has no hooks
    let settings_path = project.project_path().join(".claude/settings.local.json");
    if settings_path.exists() {
        let settings_content = fs::read_to_string(&settings_path)?;
        let settings: Value = serde_json::from_str(&settings_content)?;

        // Should either have no hooks section or empty hooks
        if let Some(hooks) = settings.get("hooks") {
            let hooks_obj = hooks.as_object().unwrap();
            assert!(hooks_obj.is_empty(), "Hooks section should be empty");
        }
    }

    Ok(())
}

#[tokio::test]
async fn test_hooks_no_change_no_message() -> Result<()> {
    let project = TestProject::new()?;

    // Create hook source repository
    let source_repo = project.create_source_repo("hooks")?;

    // Create hooks directory and add hook
    let hooks_dir = source_repo.path.join("hooks");
    fs::create_dir_all(&hooks_dir)?;

    // Create SessionStart hook
    let session_hook = serde_json::json!({
        "events": ["SessionStart"],
        "type": "command",
        "command": "echo 'test hook'",
        "description": "Test hook"
    });
    fs::write(
        hooks_dir.join("session-start.json"),
        serde_json::to_string_pretty(&session_hook)?,
    )?;

    source_repo.commit_all("Add test hook")?;
    let source_url = source_repo.bare_file_url(project.sources_path())?;

    // Create manifest
    let manifest_content = format!(
        r#"
[sources]
hooks = "{}"

[hooks]
session-hook = {{ source = "hooks", path = "hooks/session-start.json" }}
"#,
        source_url
    );
    project.write_manifest(&manifest_content)?;

    // First install - should configure hooks and show message
    let output1 = project.run_ccpm(&["install"])?;
    output1.assert_success();
    output1.assert_stdout_contains("✓ Configured 1 hook(s)");

    // Second install with same hooks - should NOT show message (no changes)
    let output2 = project.run_ccpm(&["install"])?;
    output2.assert_success();
    assert!(!output2.stdout.contains("Configured"));
    assert!(!output2.stdout.contains("hook"));

    Ok(())
}
