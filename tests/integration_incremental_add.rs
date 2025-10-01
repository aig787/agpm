//! Integration tests for incremental dependency addition with `ccpm add dep`.
//!
//! These tests verify that transitive dependency relationships are properly maintained
//! when dependencies are added incrementally to a project manifest.

use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

/// Helper to create a test project with manifest and resource files
fn setup_test_project() -> TempDir {
    let temp_dir = TempDir::new().unwrap();
    let project_dir = temp_dir.path();

    // Create manifest
    let manifest_content = r#"[sources]

[target]
agents = ".claude/agents"
snippets = ".claude/ccpm/snippets"
commands = ".claude/commands"
mcp-servers = ".claude/ccpm/mcp-servers"
scripts = ".claude/ccpm/scripts"
hooks = ".claude/ccpm/hooks"
gitignore = true
"#;
    fs::write(project_dir.join("ccpm.toml"), manifest_content).unwrap();

    // Create command file with transitive dependencies
    let command_content = r#"---
title: Test Command
description: A test command with dependencies
dependencies:
  agents:
    - path: test-agent.md
  snippets:
    - path: test-snippet.md
---

# Test Command

This command depends on an agent and a snippet.
"#;
    fs::write(project_dir.join("test-command.md"), command_content).unwrap();

    // Create agent file with its own transitive dependency
    let agent_content = r#"---
title: Test Agent
description: A test agent with dependencies
dependencies:
  snippets:
    - path: helper-snippet.md
---

# Test Agent

This agent depends on a helper snippet.
"#;
    fs::write(project_dir.join("test-agent.md"), agent_content).unwrap();

    // Create snippet files
    fs::write(
        project_dir.join("test-snippet.md"),
        "# Test Snippet\n\nA test snippet.",
    )
    .unwrap();
    fs::write(
        project_dir.join("helper-snippet.md"),
        "# Helper Snippet\n\nA helper snippet.",
    )
    .unwrap();

    temp_dir
}

/// Helper to read and parse lockfile dependencies for a resource
fn get_lockfile_dependencies(lockfile_path: &PathBuf, resource_name: &str) -> Vec<String> {
    let lockfile_content = fs::read_to_string(lockfile_path).unwrap();
    let lockfile: toml::Value = toml::from_str(&lockfile_content).unwrap();

    // Search in all resource type arrays
    for resource_type in &[
        "agents",
        "snippets",
        "commands",
        "scripts",
        "hooks",
        "mcp_servers",
    ] {
        if let Some(resources) = lockfile.get(resource_type).and_then(|v| v.as_array()) {
            for resource in resources {
                if let Some(name) = resource.get("name").and_then(|v| v.as_str())
                    && name == resource_name
                {
                    if let Some(deps) = resource.get("dependencies").and_then(|v| v.as_array()) {
                        return deps
                            .iter()
                            .filter_map(|v| v.as_str().map(String::from))
                            .collect();
                    }
                    return Vec::new();
                }
            }
        }
    }

    Vec::new()
}

#[test]
fn test_incremental_add_preserves_transitive_dependencies() {
    let temp_dir = setup_test_project();
    let project_dir = temp_dir.path();
    let lockfile_path = project_dir.join("ccpm.lock");

    // Step 1: Add command dependency (should discover transitive deps)
    let mut cmd = Command::cargo_bin("ccpm").unwrap();
    cmd.current_dir(project_dir)
        .arg("add")
        .arg("dep")
        .arg("command")
        .arg("test-command.md")
        .arg("--name")
        .arg("test-command");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Added command 'test-command'"));

    // Verify lockfile has transitive dependencies
    assert!(lockfile_path.exists(), "Lockfile should be created");

    let command_deps = get_lockfile_dependencies(&lockfile_path, "test-command");
    assert!(
        !command_deps.is_empty(),
        "Command should have transitive dependencies after first add. Found: {:?}",
        command_deps
    );
    assert!(
        command_deps.iter().any(|d| d.contains("test-agent")),
        "Command should depend on test-agent. Found: {:?}",
        command_deps
    );

    // Step 2: Add agent dependency explicitly (making it a base dependency)
    let mut cmd2 = Command::cargo_bin("ccpm").unwrap();
    cmd2.current_dir(project_dir)
        .arg("add")
        .arg("dep")
        .arg("agent")
        .arg("test-agent.md")
        .arg("--name")
        .arg("test-agent");

    cmd2.assert()
        .success()
        .stdout(predicate::str::contains("Added agent 'test-agent'"));

    // CRITICAL: Verify command still has its dependencies after agent becomes a base dep
    let command_deps_after = get_lockfile_dependencies(&lockfile_path, "test-command");
    assert!(
        !command_deps_after.is_empty(),
        "Command dependencies should be preserved after adding agent as base dep! \
         This was the bug we fixed. Found: {:?}",
        command_deps_after
    );
    assert!(
        command_deps_after.iter().any(|d| d.contains("test-agent")),
        "Command -> Agent dependency should be maintained! Found: {:?}",
        command_deps_after
    );

    // Verify agent also has its dependencies
    let agent_deps = get_lockfile_dependencies(&lockfile_path, "test-agent");
    assert!(
        !agent_deps.is_empty(),
        "Agent should have its transitive dependencies. Found: {:?}",
        agent_deps
    );
    assert!(
        agent_deps.iter().any(|d| d.contains("helper-snippet")),
        "Agent should depend on helper-snippet. Found: {:?}",
        agent_deps
    );
}

#[test]
fn test_incremental_add_chain_of_three_dependencies() {
    let temp_dir = setup_test_project();
    let project_dir = temp_dir.path();
    let lockfile_path = project_dir.join("ccpm.lock");

    // Add command (discovers agent and snippet as transitive deps)
    let mut cmd1 = Command::cargo_bin("ccpm").unwrap();
    cmd1.current_dir(project_dir)
        .arg("add")
        .arg("dep")
        .arg("command")
        .arg("test-command.md")
        .arg("--name")
        .arg("test-command")
        .assert()
        .success();

    let command_deps_step1 = get_lockfile_dependencies(&lockfile_path, "test-command");
    assert!(
        !command_deps_step1.is_empty(),
        "Command should have dependencies"
    );

    // Add agent explicitly (was transitive, now base)
    let mut cmd2 = Command::cargo_bin("ccpm").unwrap();
    cmd2.current_dir(project_dir)
        .arg("add")
        .arg("dep")
        .arg("agent")
        .arg("test-agent.md")
        .arg("--name")
        .arg("test-agent")
        .assert()
        .success();

    let command_deps_step2 = get_lockfile_dependencies(&lockfile_path, "test-command");
    let agent_deps_step2 = get_lockfile_dependencies(&lockfile_path, "test-agent");

    assert!(
        !command_deps_step2.is_empty(),
        "Command deps should persist after step 2"
    );
    assert!(
        !agent_deps_step2.is_empty(),
        "Agent should have dependencies"
    );

    // Add helper-snippet explicitly (was transitive of agent, now base)
    let mut cmd3 = Command::cargo_bin("ccpm").unwrap();
    cmd3.current_dir(project_dir)
        .arg("add")
        .arg("dep")
        .arg("snippet")
        .arg("helper-snippet.md")
        .arg("--name")
        .arg("helper-snippet")
        .assert()
        .success();

    // Verify ALL dependency relationships are still intact
    let command_deps_final = get_lockfile_dependencies(&lockfile_path, "test-command");
    let agent_deps_final = get_lockfile_dependencies(&lockfile_path, "test-agent");

    assert!(
        !command_deps_final.is_empty(),
        "Command should still have dependencies after all adds"
    );
    assert!(
        !agent_deps_final.is_empty(),
        "Agent should still have dependencies after helper-snippet becomes base dep"
    );

    // Verify the specific dependency relationships
    assert!(
        command_deps_final.iter().any(|d| d.contains("test-agent")),
        "Command -> Agent dependency should be maintained"
    );
    assert!(
        agent_deps_final
            .iter()
            .any(|d| d.contains("helper-snippet")),
        "Agent -> Snippet dependency should be maintained"
    );
}

#[test]
fn test_incremental_add_with_shared_dependency() {
    let temp_dir = setup_test_project();
    let project_dir = temp_dir.path();

    // Create a second command that shares the test-snippet dependency
    let command2_content = r#"---
title: Second Command
dependencies:
  snippets:
    - path: test-snippet.md
---

# Second Command

This command also uses test-snippet.
"#;
    fs::write(project_dir.join("second-command.md"), command2_content).unwrap();

    let lockfile_path = project_dir.join("ccpm.lock");

    // Add first command
    Command::cargo_bin("ccpm")
        .unwrap()
        .current_dir(project_dir)
        .arg("add")
        .arg("dep")
        .arg("command")
        .arg("test-command.md")
        .arg("--name")
        .arg("test-command")
        .assert()
        .success();

    // Add second command
    Command::cargo_bin("ccpm")
        .unwrap()
        .current_dir(project_dir)
        .arg("add")
        .arg("dep")
        .arg("command")
        .arg("second-command.md")
        .arg("--name")
        .arg("second-command")
        .assert()
        .success();

    // Both commands should have their dependencies
    let cmd1_deps = get_lockfile_dependencies(&lockfile_path, "test-command");
    let cmd2_deps = get_lockfile_dependencies(&lockfile_path, "second-command");

    assert!(
        !cmd1_deps.is_empty(),
        "First command should have dependencies"
    );
    assert!(
        !cmd2_deps.is_empty(),
        "Second command should have dependencies"
    );

    // Now add the shared snippet explicitly
    Command::cargo_bin("ccpm")
        .unwrap()
        .current_dir(project_dir)
        .arg("add")
        .arg("dep")
        .arg("snippet")
        .arg("test-snippet.md")
        .arg("--name")
        .arg("test-snippet")
        .assert()
        .success();

    // Both commands should STILL have their dependencies
    let cmd1_deps_after = get_lockfile_dependencies(&lockfile_path, "test-command");
    let cmd2_deps_after = get_lockfile_dependencies(&lockfile_path, "second-command");

    assert!(
        cmd1_deps_after.iter().any(|d| d.contains("test-snippet")),
        "First command should still depend on test-snippet"
    );
    assert!(
        cmd2_deps_after.iter().any(|d| d.contains("test-snippet")),
        "Second command should still depend on test-snippet"
    );
}
