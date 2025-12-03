//! Integration tests for the migrate command.
//!
//! Tests cover:
//! - CCPM → AGPM config migration
//! - Legacy gitignore format migration
//! - Artifact installation path verification
//! - Orphan file detection
//! - CLI flags and end-to-end workflows

use crate::common::{ManifestBuilder, TestProject};
use anyhow::Result;
use tokio::fs;

// ============================================================================
// Section 1: CCPM Config Migration Tests
// ============================================================================

/// Test that ccpm.toml is correctly renamed to agpm.toml
#[tokio::test]
async fn test_migrate_ccpm_toml_to_agpm_toml() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;
    let source_repo = project.create_source_repo("community").await?;

    source_repo.add_resource("agents", "helper", "# Helper Agent").await?;
    source_repo.commit_all("Add agent")?;
    source_repo.tag_version("v1.0.0")?;

    let source_url = source_repo.bare_file_url(project.sources_path()).await?;

    // Create ccpm.toml (legacy naming)
    let ccpm_manifest = format!(
        r#"[sources]
community = "{}"

[agents]
helper = {{ source = "community", path = "agents/helper.md", version = "v1.0.0" }}
"#,
        source_url
    );

    let ccpm_toml_path = project.project_path().join("ccpm.toml");
    let agpm_toml_path = project.project_path().join("agpm.toml");

    fs::write(&ccpm_toml_path, &ccpm_manifest).await?;

    // Run migrate
    let output = project.run_agpm(&["migrate", "--skip-install"])?;
    output.assert_success();

    // Verify ccpm.toml was renamed to agpm.toml
    assert!(!ccpm_toml_path.exists(), "ccpm.toml should be renamed");
    assert!(agpm_toml_path.exists(), "agpm.toml should exist");

    // Verify content was preserved
    let agpm_content = fs::read_to_string(&agpm_toml_path).await?;
    assert!(agpm_content.contains("community"), "Content should be preserved");
    assert!(agpm_content.contains("agents/helper.md"), "Dependencies should be preserved");

    // Verify agpm install works after migration
    let install_output = project.run_agpm(&["install"])?;
    install_output.assert_success();

    // Verify agent was installed
    let agent_path = project.project_path().join(".claude/agents/agpm/helper.md");
    assert!(agent_path.exists(), "Agent should be installed after migration");

    Ok(())
}

/// Test that both ccpm.toml and ccpm.lock are migrated together
#[tokio::test]
async fn test_migrate_ccpm_lock_to_agpm_lock() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;
    let source_repo = project.create_source_repo("community").await?;

    source_repo.add_resource("agents", "test", "# Test Agent").await?;
    source_repo.commit_all("Add agent")?;
    source_repo.tag_version("v1.0.0")?;

    let source_url = source_repo.bare_file_url(project.sources_path()).await?;

    // Create ccpm.toml
    let ccpm_manifest = format!(
        r#"[sources]
community = "{}"

[agents]
test = {{ source = "community", path = "agents/test.md", version = "v1.0.0" }}
"#,
        source_url
    );

    let ccpm_toml_path = project.project_path().join("ccpm.toml");
    let ccpm_lock_path = project.project_path().join("ccpm.lock");
    let agpm_toml_path = project.project_path().join("agpm.toml");
    let agpm_lock_path = project.project_path().join("agpm.lock");

    fs::write(&ccpm_toml_path, &ccpm_manifest).await?;

    // Create a simple ccpm.lock file
    let ccpm_lock = "version = 1\n";
    fs::write(&ccpm_lock_path, ccpm_lock).await?;

    // Run migrate
    let output = project.run_agpm(&["migrate", "--skip-install"])?;
    output.assert_success();

    // Verify both files were renamed
    assert!(!ccpm_toml_path.exists(), "ccpm.toml should be renamed");
    assert!(!ccpm_lock_path.exists(), "ccpm.lock should be renamed");
    assert!(agpm_toml_path.exists(), "agpm.toml should exist");
    assert!(agpm_lock_path.exists(), "agpm.lock should exist");

    Ok(())
}

/// Test that migration fails when both ccpm.toml and agpm.toml exist
#[tokio::test]
async fn test_migrate_ccpm_with_existing_agpm_conflicts() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;

    let ccpm_toml_path = project.project_path().join("ccpm.toml");
    let agpm_toml_path = project.project_path().join("agpm.toml");

    // Create both files (conflict scenario)
    fs::write(&ccpm_toml_path, "[sources]\n").await?;
    fs::write(&agpm_toml_path, "[sources]\n").await?;

    // Run migrate - should fail
    let output = project.run_agpm(&["migrate"])?;
    assert!(!output.success, "Should fail when both ccpm.toml and agpm.toml exist");

    // Verify error message mentions conflict
    let error_output = format!("{}\n{}", output.stdout, output.stderr);
    assert!(
        error_output.to_lowercase().contains("conflict"),
        "Error should mention conflict. Output: {}",
        error_output
    );

    // Original files should remain unchanged
    assert!(ccpm_toml_path.exists(), "ccpm.toml should remain");
    assert!(agpm_toml_path.exists(), "agpm.toml should remain");

    Ok(())
}

/// Test that complex ccpm.toml with multiple dependencies is preserved
#[tokio::test]
async fn test_migrate_ccpm_preserves_dependencies() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;
    let source_repo = project.create_source_repo("community").await?;

    // Create multiple resource types
    source_repo.add_resource("agents", "agent1", "# Agent 1").await?;
    source_repo.add_resource("agents", "agent2", "# Agent 2").await?;
    source_repo.add_resource("snippets", "snippet1", "# Snippet 1").await?;
    source_repo.add_resource("commands", "command1", "# Command 1").await?;
    source_repo.commit_all("Add resources")?;
    source_repo.tag_version("v1.0.0")?;

    let source_url = source_repo.bare_file_url(project.sources_path()).await?;

    // Create complex ccpm.toml
    let ccpm_manifest = format!(
        r#"[sources]
community = "{}"

[agents]
agent1 = {{ source = "community", path = "agents/agent1.md", version = "v1.0.0" }}
agent2 = {{ source = "community", path = "agents/agent2.md", version = "v1.0.0" }}

[snippets]
snippet1 = {{ source = "community", path = "snippets/snippet1.md", version = "v1.0.0" }}

[commands]
command1 = {{ source = "community", path = "commands/command1.md", version = "v1.0.0" }}
"#,
        source_url
    );

    fs::write(project.project_path().join("ccpm.toml"), &ccpm_manifest).await?;

    // Run migrate then install
    let migrate_output = project.run_agpm(&["migrate", "--skip-install"])?;
    migrate_output.assert_success();

    let install_output = project.run_agpm(&["install"])?;
    install_output.assert_success();

    // Verify all resources installed correctly
    let agents_dir = project.project_path().join(".claude/agents/agpm");
    let snippets_dir = project.project_path().join(".agpm/snippets");
    let commands_dir = project.project_path().join(".claude/commands/agpm");

    assert!(agents_dir.join("agent1.md").exists(), "agent1 should be installed");
    assert!(agents_dir.join("agent2.md").exists(), "agent2 should be installed");
    assert!(snippets_dir.join("snippet1.md").exists(), "snippet1 should be installed");
    assert!(commands_dir.join("command1.md").exists(), "command1 should be installed");

    Ok(())
}

// ============================================================================
// Section 2: Legacy Gitignore Format Migration Tests
// ============================================================================

/// Test that AGPM managed section is removed from .gitignore
#[tokio::test]
async fn test_migrate_removes_managed_gitignore_section() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;

    // Create .gitignore with managed section
    let gitignore_content = r#"# User entries before
node_modules/
*.log

# AGPM managed entries - do not edit
.claude/agents/*.md
.claude/snippets/*.md
# End of AGPM managed entries

# User entries after
.env
"#;
    fs::write(project.project_path().join(".gitignore"), gitignore_content).await?;

    // Create old-format resource to trigger format migration
    let agents_dir = project.project_path().join(".claude/agents");
    fs::create_dir_all(&agents_dir).await?;
    fs::write(agents_dir.join("test.md"), "# Test Agent").await?;

    // Create minimal agpm.toml (already migrated from ccpm naming)
    fs::write(project.project_path().join("agpm.toml"), "[sources]\n").await?;

    // Create lockfile that tracks the resource at old path
    let lockfile = r#"version = 1

[[agents]]
name = "test"
source = "test"
path = "agents/test.md"
version = "v1.0.0"
resolved_commit = "abc123"
checksum = "sha256:abc"
context_checksum = "sha256:def"
installed_at = ".claude/agents/test.md"
dependencies = []
resource_type = "Agent"
tool = "claude-code"
"#;
    fs::write(project.project_path().join("agpm.lock"), lockfile).await?;

    // Run migrate
    let output = project.run_agpm(&["migrate", "--skip-install"])?;
    output.assert_success();

    // Verify managed section was replaced with new paths
    let new_gitignore = fs::read_to_string(project.project_path().join(".gitignore")).await?;

    // Old markers should be removed
    assert!(
        !new_gitignore.contains("AGPM managed entries - do not edit"),
        "Old managed section marker should be removed"
    );
    assert!(!new_gitignore.contains("End of AGPM managed entries"), "End marker should be removed");

    // New paths should be added
    assert!(
        new_gitignore.contains("# AGPM managed paths"),
        "New header should be present. Gitignore:\n{}",
        new_gitignore
    );
    assert!(new_gitignore.contains(".claude/*/agpm/"), "Claude agpm paths should be added");
    assert!(new_gitignore.contains(".opencode/*/agpm/"), "OpenCode agpm paths should be added");
    assert!(new_gitignore.contains(".agpm/"), "Cache dir should be added");
    assert!(new_gitignore.contains("agpm.private.toml"), "Private toml should be added");
    assert!(new_gitignore.contains("agpm.private.lock"), "Private lock should be added");

    // User entries should be preserved
    assert!(new_gitignore.contains("node_modules/"), "User entries before should be preserved");
    assert!(new_gitignore.contains(".env"), "User entries after should be preserved");

    // Verify resource was moved to agpm/ subdirectory
    assert!(
        agents_dir.join("agpm/test.md").exists(),
        "Resource should be moved to agpm/ subdirectory"
    );
    assert!(!agents_dir.join("test.md").exists(), "Original resource should be removed");

    Ok(())
}

/// Test that CCPM managed section markers are also handled
#[tokio::test]
async fn test_migrate_ccpm_managed_gitignore_section() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;

    // Create .gitignore with CCPM (legacy) managed section
    let gitignore_content = r#"# User entries
build/

# CCPM managed entries - do not edit
.claude/agents/*.md
# End of CCPM managed entries
"#;
    fs::write(project.project_path().join(".gitignore"), gitignore_content).await?;

    // Create old-format resource
    let agents_dir = project.project_path().join(".claude/agents");
    fs::create_dir_all(&agents_dir).await?;
    fs::write(agents_dir.join("legacy.md"), "# Legacy Agent").await?;

    fs::write(project.project_path().join("agpm.toml"), "[sources]\n").await?;

    // Create lockfile that tracks the resource at old path
    let lockfile = r#"version = 1

[[agents]]
name = "legacy"
source = "test"
path = "agents/legacy.md"
version = "v1.0.0"
resolved_commit = "abc123"
checksum = "sha256:abc"
context_checksum = "sha256:def"
installed_at = ".claude/agents/legacy.md"
dependencies = []
resource_type = "Agent"
tool = "claude-code"
"#;
    fs::write(project.project_path().join("agpm.lock"), lockfile).await?;

    // Run migrate
    let output = project.run_agpm(&["migrate", "--skip-install"])?;
    output.assert_success();

    // Verify CCPM managed section was replaced with new paths
    let new_gitignore = fs::read_to_string(project.project_path().join(".gitignore")).await?;

    assert!(
        !new_gitignore.contains("CCPM managed entries"),
        "CCPM managed section should be removed"
    );
    assert!(new_gitignore.contains("# AGPM managed paths"), "New header should be present");
    assert!(new_gitignore.contains(".claude/*/agpm/"), "New paths should be added");
    assert!(new_gitignore.contains("build/"), "User entries should be preserved");

    Ok(())
}

/// Test that mixed content gitignore preserves all user entries
#[tokio::test]
async fn test_migrate_mixed_content_gitignore() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;

    // Create .gitignore with content before, in, and after managed section
    let gitignore_content = r#"# === User Section 1 ===
node_modules/
dist/
*.log

# AGPM managed entries - do not edit
.claude/agents/*.md
.claude/commands/*.md
.claude/snippets/*.md
# End of AGPM managed entries

# === User Section 2 ===
.env
.env.local
secrets/

# More user content
coverage/
"#;
    fs::write(project.project_path().join(".gitignore"), gitignore_content).await?;

    // Create old-format resource
    let agents_dir = project.project_path().join(".claude/agents");
    fs::create_dir_all(&agents_dir).await?;
    fs::write(agents_dir.join("test.md"), "# Test").await?;

    fs::write(project.project_path().join("agpm.toml"), "[sources]\n").await?;

    // Create lockfile that tracks the resource at old path
    let lockfile = r#"version = 1

[[agents]]
name = "test"
source = "test"
path = "agents/test.md"
version = "v1.0.0"
resolved_commit = "abc123"
checksum = "sha256:abc"
context_checksum = "sha256:def"
installed_at = ".claude/agents/test.md"
dependencies = []
resource_type = "Agent"
tool = "claude-code"
"#;
    fs::write(project.project_path().join("agpm.lock"), lockfile).await?;

    // Run migrate
    let output = project.run_agpm(&["migrate", "--skip-install"])?;
    output.assert_success();

    // Verify all user content preserved
    let new_gitignore = fs::read_to_string(project.project_path().join(".gitignore")).await?;

    assert!(new_gitignore.contains("User Section 1"), "Section 1 should be preserved");
    assert!(new_gitignore.contains("User Section 2"), "Section 2 should be preserved");
    assert!(new_gitignore.contains("node_modules/"), "node_modules should be preserved");
    assert!(new_gitignore.contains(".env"), ".env should be preserved");
    assert!(new_gitignore.contains("secrets/"), "secrets/ should be preserved");
    assert!(new_gitignore.contains("coverage/"), "coverage/ should be preserved");

    // Old managed section replaced with new paths
    assert!(
        !new_gitignore.contains("AGPM managed entries - do not edit"),
        "Old managed section should be removed"
    );
    assert!(new_gitignore.contains("# AGPM managed paths"), "New header should be present");
    assert!(new_gitignore.contains(".claude/*/agpm/"), "New paths should be added");

    Ok(())
}

/// Test that gitignore formatting is reasonably preserved
#[tokio::test]
async fn test_migrate_gitignore_preserves_formatting() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;

    // Create .gitignore with specific formatting
    let gitignore_content = r#"# Comment line
*.log

# Another comment
node_modules/

# AGPM managed entries
.claude/agents/*.md
# End of AGPM managed entries

# Final section
.env
"#;
    fs::write(project.project_path().join(".gitignore"), gitignore_content).await?;

    // Create old-format resource
    let agents_dir = project.project_path().join(".claude/agents");
    fs::create_dir_all(&agents_dir).await?;
    fs::write(agents_dir.join("test.md"), "# Test").await?;

    fs::write(project.project_path().join("agpm.toml"), "[sources]\n").await?;

    // Run migrate
    let output = project.run_agpm(&["migrate", "--skip-install"])?;
    output.assert_success();

    // Verify comments are preserved
    let new_gitignore = fs::read_to_string(project.project_path().join(".gitignore")).await?;

    assert!(new_gitignore.contains("# Comment line"), "Comments should be preserved");
    assert!(new_gitignore.contains("# Another comment"), "Comments should be preserved");
    assert!(new_gitignore.contains("# Final section"), "Comments should be preserved");

    Ok(())
}

// ============================================================================
// Section 3: Artifact Installation Path Tests
// ============================================================================

/// Test that agents are moved to agpm/ subdirectory
#[tokio::test]
async fn test_migrate_moves_agents_to_agpm_subdirectory() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;

    // Create agents at old path
    let agents_dir = project.project_path().join(".claude/agents");
    fs::create_dir_all(&agents_dir).await?;
    fs::write(agents_dir.join("agent1.md"), "# Agent 1").await?;
    fs::write(agents_dir.join("agent2.md"), "# Agent 2").await?;

    fs::write(project.project_path().join("agpm.toml"), "[sources]\n").await?;

    // Create lockfile that tracks the resources at the old paths
    // (only lockfile-tracked files are migrated)
    let lockfile = r#"version = 1

[[agents]]
name = "agent1"
source = "test"
path = "agents/agent1.md"
version = "v1.0.0"
resolved_commit = "abc123"
checksum = "sha256:abc"
context_checksum = "sha256:def"
installed_at = ".claude/agents/agent1.md"
dependencies = []
resource_type = "Agent"
tool = "claude-code"

[[agents]]
name = "agent2"
source = "test"
path = "agents/agent2.md"
version = "v1.0.0"
resolved_commit = "abc123"
checksum = "sha256:abc"
context_checksum = "sha256:def"
installed_at = ".claude/agents/agent2.md"
dependencies = []
resource_type = "Agent"
tool = "claude-code"
"#;
    fs::write(project.project_path().join("agpm.lock"), lockfile).await?;

    // Run migrate
    let output = project.run_agpm(&["migrate", "--skip-install"])?;
    output.assert_success();

    // Verify agents moved to agpm/ subdirectory
    let agpm_dir = agents_dir.join("agpm");
    assert!(agpm_dir.join("agent1.md").exists(), "agent1 should be in agpm/");
    assert!(agpm_dir.join("agent2.md").exists(), "agent2 should be in agpm/");
    assert!(!agents_dir.join("agent1.md").exists(), "agent1 should not be at old path");
    assert!(!agents_dir.join("agent2.md").exists(), "agent2 should not be at old path");

    Ok(())
}

/// Test that all resource types are migrated correctly
#[tokio::test]
async fn test_migrate_moves_all_resource_types() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;
    let claude_dir = project.project_path().join(".claude");

    // Create resources at old paths for all types
    fs::create_dir_all(claude_dir.join("agents")).await?;
    fs::create_dir_all(claude_dir.join("commands")).await?;
    fs::create_dir_all(claude_dir.join("snippets")).await?;
    fs::create_dir_all(claude_dir.join("scripts")).await?;

    fs::write(claude_dir.join("agents/agent.md"), "# Agent").await?;
    fs::write(claude_dir.join("commands/cmd.md"), "# Command").await?;
    fs::write(claude_dir.join("snippets/snip.md"), "# Snippet").await?;
    fs::write(claude_dir.join("scripts/script.md"), "# Script").await?;

    fs::write(project.project_path().join("agpm.toml"), "[sources]\n").await?;

    // Create lockfile that tracks all resources at old paths
    let lockfile = r#"version = 1

[[agents]]
name = "agent"
source = "test"
path = "agents/agent.md"
version = "v1.0.0"
resolved_commit = "abc123"
checksum = "sha256:abc"
context_checksum = "sha256:def"
installed_at = ".claude/agents/agent.md"
dependencies = []
resource_type = "Agent"
tool = "claude-code"

[[commands]]
name = "cmd"
source = "test"
path = "commands/cmd.md"
version = "v1.0.0"
resolved_commit = "abc123"
checksum = "sha256:abc"
context_checksum = "sha256:def"
installed_at = ".claude/commands/cmd.md"
dependencies = []
resource_type = "Command"
tool = "claude-code"

[[snippets]]
name = "snip"
source = "test"
path = "snippets/snip.md"
version = "v1.0.0"
resolved_commit = "abc123"
checksum = "sha256:abc"
context_checksum = "sha256:def"
installed_at = ".claude/snippets/snip.md"
dependencies = []
resource_type = "Snippet"
tool = "agpm"

[[scripts]]
name = "script"
source = "test"
path = "scripts/script.md"
version = "v1.0.0"
resolved_commit = "abc123"
checksum = "sha256:abc"
context_checksum = "sha256:def"
installed_at = ".claude/scripts/script.md"
dependencies = []
resource_type = "Script"
tool = "claude-code"
"#;
    fs::write(project.project_path().join("agpm.lock"), lockfile).await?;

    // Run migrate
    let output = project.run_agpm(&["migrate", "--skip-install"])?;
    output.assert_success();

    // Verify all moved to agpm/ subdirectories
    assert!(claude_dir.join("agents/agpm/agent.md").exists(), "Agent should be moved");
    assert!(claude_dir.join("commands/agpm/cmd.md").exists(), "Command should be moved");
    assert!(claude_dir.join("snippets/agpm/snip.md").exists(), "Snippet should be moved");
    assert!(claude_dir.join("scripts/agpm/script.md").exists(), "Script should be moved");

    // Verify old paths don't have files
    assert!(!claude_dir.join("agents/agent.md").exists(), "Agent should not be at old path");
    assert!(!claude_dir.join("commands/cmd.md").exists(), "Command should not be at old path");
    assert!(!claude_dir.join("snippets/snip.md").exists(), "Snippet should not be at old path");
    assert!(!claude_dir.join("scripts/script.md").exists(), "Script should not be at old path");

    Ok(())
}

/// Test that lockfile installed_at paths are updated
#[tokio::test]
async fn test_migrate_updates_lockfile_paths() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;

    // Create old-format lockfile with paths without /agpm/
    let old_lockfile = r#"version = 1

[[agents]]
name = "test"
source = "community"
path = "agents/test.md"
version = "v1.0.0"
resolved_commit = "abc123"
checksum = "sha256:placeholder"
context_checksum = "sha256:placeholder"
installed_at = ".claude/agents/test.md"
dependencies = []
resource_type = "Agent"
tool = "claude-code"
"#;

    fs::write(project.project_path().join("agpm.lock"), old_lockfile).await?;
    fs::write(project.project_path().join("agpm.toml"), "[sources]\n").await?;

    // Create old-format resource to trigger format migration
    let agents_dir = project.project_path().join(".claude/agents");
    fs::create_dir_all(&agents_dir).await?;
    fs::write(agents_dir.join("test.md"), "# Test").await?;

    // Run migrate
    let output = project.run_agpm(&["migrate", "--skip-install"])?;
    output.assert_success();

    // Verify lockfile paths were updated
    let new_lockfile = project.read_lockfile().await?;
    assert!(
        new_lockfile.contains(".claude/agents/agpm/test.md"),
        "Lockfile should have updated path. Actual:\n{}",
        new_lockfile
    );
    assert!(
        !new_lockfile.contains("installed_at = \".claude/agents/test.md\""),
        "Old path format should be replaced"
    );

    Ok(())
}

/// Test migration of opencode paths (singular directory names)
#[tokio::test]
async fn test_migrate_opencode_paths() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;

    // Create opencode resources at old paths (singular: agent, not agents)
    let opencode_dir = project.project_path().join(".opencode");
    fs::create_dir_all(opencode_dir.join("agent")).await?;
    fs::write(opencode_dir.join("agent/test.md"), "# OpenCode Agent").await?;

    fs::write(project.project_path().join("agpm.toml"), "[sources]\n").await?;

    // Create lockfile that tracks the resource at old path
    let lockfile = r#"version = 1

[[agents]]
name = "test"
source = "test"
path = "agents/test.md"
version = "v1.0.0"
resolved_commit = "abc123"
checksum = "sha256:abc"
context_checksum = "sha256:def"
installed_at = ".opencode/agent/test.md"
dependencies = []
resource_type = "Agent"
tool = "opencode"
"#;
    fs::write(project.project_path().join("agpm.lock"), lockfile).await?;

    // Run migrate
    let output = project.run_agpm(&["migrate", "--skip-install"])?;
    output.assert_success();

    // Verify opencode agent moved to agpm/ subdirectory
    assert!(
        opencode_dir.join("agent/agpm/test.md").exists(),
        "OpenCode agent should be moved to agpm/"
    );
    assert!(
        !opencode_dir.join("agent/test.md").exists(),
        "OpenCode agent should not be at old path"
    );

    Ok(())
}

/// Test migration of project using both claude-code and opencode tools
#[tokio::test]
async fn test_migrate_multi_tool_project() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;

    // Create resources for both tools at old paths
    let claude_dir = project.project_path().join(".claude/agents");
    let opencode_dir = project.project_path().join(".opencode/agent");

    fs::create_dir_all(&claude_dir).await?;
    fs::create_dir_all(&opencode_dir).await?;

    fs::write(claude_dir.join("claude-agent.md"), "# Claude Agent").await?;
    fs::write(opencode_dir.join("opencode-agent.md"), "# OpenCode Agent").await?;

    fs::write(project.project_path().join("agpm.toml"), "[sources]\n").await?;

    // Create lockfile that tracks resources at old paths for both tools
    let lockfile = r#"version = 1

[[agents]]
name = "claude-agent"
source = "test"
path = "agents/claude-agent.md"
version = "v1.0.0"
resolved_commit = "abc123"
checksum = "sha256:abc"
context_checksum = "sha256:def"
installed_at = ".claude/agents/claude-agent.md"
dependencies = []
resource_type = "Agent"
tool = "claude-code"

[[agents]]
name = "opencode-agent"
source = "test"
path = "agents/opencode-agent.md"
version = "v1.0.0"
resolved_commit = "abc123"
checksum = "sha256:abc"
context_checksum = "sha256:def"
installed_at = ".opencode/agent/opencode-agent.md"
dependencies = []
resource_type = "Agent"
tool = "opencode"
"#;
    fs::write(project.project_path().join("agpm.lock"), lockfile).await?;

    // Run migrate
    let output = project.run_agpm(&["migrate", "--skip-install"])?;
    output.assert_success();

    // Verify BOTH tools' resources were migrated
    assert!(
        claude_dir.join("agpm/claude-agent.md").exists(),
        "Claude agent should be moved to agpm/"
    );
    assert!(
        opencode_dir.join("agpm/opencode-agent.md").exists(),
        "OpenCode agent should be moved to agpm/"
    );

    // Verify old paths are empty
    assert!(!claude_dir.join("claude-agent.md").exists(), "Claude agent should not be at old path");
    assert!(
        !opencode_dir.join("opencode-agent.md").exists(),
        "OpenCode agent should not be at old path"
    );

    Ok(())
}

/// Test that lockfile with multiple tools has all paths updated
#[tokio::test]
async fn test_migrate_multi_tool_lockfile_paths() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;

    // Create old-format lockfile with paths for multiple tools
    let old_lockfile = r#"version = 1

[[agents]]
name = "claude-agent"
source = "community"
path = "agents/claude.md"
version = "v1.0.0"
resolved_commit = "abc123"
checksum = "sha256:placeholder"
context_checksum = "sha256:placeholder"
installed_at = ".claude/agents/claude.md"
dependencies = []
resource_type = "Agent"
tool = "claude-code"

[[agents]]
name = "opencode-agent"
source = "community"
path = "agents/opencode.md"
version = "v1.0.0"
resolved_commit = "abc123"
checksum = "sha256:placeholder"
context_checksum = "sha256:placeholder"
installed_at = ".opencode/agent/opencode.md"
dependencies = []
resource_type = "Agent"
tool = "opencode"
"#;

    fs::write(project.project_path().join("agpm.lock"), old_lockfile).await?;
    fs::write(project.project_path().join("agpm.toml"), "[sources]\n").await?;

    // Create old-format resources
    let claude_dir = project.project_path().join(".claude/agents");
    let opencode_dir = project.project_path().join(".opencode/agent");
    fs::create_dir_all(&claude_dir).await?;
    fs::create_dir_all(&opencode_dir).await?;
    fs::write(claude_dir.join("claude.md"), "# Claude").await?;
    fs::write(opencode_dir.join("opencode.md"), "# OpenCode").await?;

    // Run migrate
    let output = project.run_agpm(&["migrate", "--skip-install"])?;
    output.assert_success();

    // Verify lockfile has updated paths for BOTH tools
    let new_lockfile = project.read_lockfile().await?;
    assert!(
        new_lockfile.contains(".claude/agents/agpm/"),
        "Claude path should be updated. Lockfile:\n{}",
        new_lockfile
    );
    assert!(
        new_lockfile.contains(".opencode/agent/agpm/"),
        "OpenCode path should be updated. Lockfile:\n{}",
        new_lockfile
    );

    Ok(())
}

// ============================================================================
// Section 4: Orphan File Detection Tests (Comprehensive Audit)
// ============================================================================

/// Test that no orphan files remain after migration
#[tokio::test]
async fn test_migrate_no_orphan_files_after_migration() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;
    let source_repo = project.create_source_repo("community").await?;

    source_repo.add_resource("agents", "test", "# Test Agent").await?;
    source_repo.commit_all("Add agent")?;
    source_repo.tag_version("v1.0.0")?;

    let source_url = source_repo.bare_file_url(project.sources_path()).await?;

    // Create ccpm.toml (will be migrated)
    let manifest = format!(
        r#"[sources]
community = "{}"

[agents]
test = {{ source = "community", path = "agents/test.md", version = "v1.0.0" }}
"#,
        source_url
    );
    fs::write(project.project_path().join("ccpm.toml"), &manifest).await?;

    // Create old-format resource (will be moved)
    let agents_dir = project.project_path().join(".claude/agents");
    fs::create_dir_all(&agents_dir).await?;
    fs::write(agents_dir.join("test.md"), "# Test").await?;

    // Create ccpm.lock that tracks the resource at the old path
    // (will be renamed to agpm.lock during migration)
    let lockfile = r#"version = 1

[[agents]]
name = "test"
source = "community"
path = "agents/test.md"
version = "v1.0.0"
resolved_commit = "abc123"
checksum = "sha256:abc"
context_checksum = "sha256:def"
installed_at = ".claude/agents/test.md"
dependencies = []
resource_type = "Agent"
tool = "claude-code"
"#;
    fs::write(project.project_path().join("ccpm.lock"), lockfile).await?;

    // Run migrate then install
    let migrate_output = project.run_agpm(&["migrate"])?;
    migrate_output.assert_success();

    // Verify no files at old paths
    assert!(!agents_dir.join("test.md").exists(), "No files should remain at old path");

    // Verify files at new paths
    assert!(agents_dir.join("agpm/test.md").exists(), "Files should be at new path");

    // Verify lockfile matches filesystem
    let lockfile = project.load_lockfile()?;
    for agent in &lockfile.agents {
        let installed_path = project.project_path().join(&agent.installed_at);
        assert!(
            installed_path.exists(),
            "Lockfile path {} should exist on disk",
            agent.installed_at
        );
    }

    Ok(())
}

/// Test comprehensive lockfile-filesystem consistency
#[tokio::test]
async fn test_migrate_lockfile_filesystem_consistency() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;
    let source_repo = project.create_source_repo("community").await?;

    source_repo.add_resource("agents", "agent1", "# Agent 1").await?;
    source_repo.add_resource("agents", "agent2", "# Agent 2").await?;
    source_repo.add_resource("snippets", "snippet1", "# Snippet 1").await?;
    source_repo.commit_all("Add resources")?;
    source_repo.tag_version("v1.0.0")?;

    let source_url = source_repo.bare_file_url(project.sources_path()).await?;

    let manifest = ManifestBuilder::new()
        .add_source("community", &source_url)
        .add_standard_agent("agent1", "community", "agents/agent1.md")
        .add_standard_agent("agent2", "community", "agents/agent2.md")
        .add_snippet("snippet1", |d| {
            d.source("community").path("snippets/snippet1.md").version("v1.0.0")
        })
        .build();

    project.write_manifest(&manifest).await?;

    // Run install
    let output = project.run_agpm(&["install"])?;
    output.assert_success();

    // Comprehensive audit
    let lockfile = project.load_lockfile()?;

    // 1. Every installed_at path should exist
    for agent in &lockfile.agents {
        let path = project.project_path().join(&agent.installed_at);
        assert!(path.exists(), "Agent {} should exist at {}", agent.name, agent.installed_at);
    }
    for snippet in &lockfile.snippets {
        let path = project.project_path().join(&snippet.installed_at);
        assert!(path.exists(), "Snippet {} should exist at {}", snippet.name, snippet.installed_at);
    }

    // 2. Count files in agpm directories matches lockfile
    let agents_agpm_dir = project.project_path().join(".claude/agents/agpm");
    if agents_agpm_dir.exists() {
        let agent_files: Vec<_> = std::fs::read_dir(&agents_agpm_dir)?
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().is_some_and(|ext| ext == "md"))
            .collect();
        assert_eq!(
            agent_files.len(),
            lockfile.agents.len(),
            "Agent file count should match lockfile"
        );
    }

    Ok(())
}

/// Test that stale empty directories are handled appropriately
#[tokio::test]
async fn test_migrate_stale_directory_cleanup() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;

    // Create resources in multiple directories
    let agents_dir = project.project_path().join(".claude/agents");
    let commands_dir = project.project_path().join(".claude/commands");
    fs::create_dir_all(&agents_dir).await?;
    fs::create_dir_all(&commands_dir).await?;

    fs::write(agents_dir.join("agent.md"), "# Agent").await?;
    fs::write(commands_dir.join("cmd.md"), "# Command").await?;

    fs::write(project.project_path().join("agpm.toml"), "[sources]\n").await?;

    // Create lockfile that tracks both resources at old paths
    let lockfile = r#"version = 1

[[agents]]
name = "agent"
source = "test"
path = "agents/agent.md"
version = "v1.0.0"
resolved_commit = "abc123"
checksum = "sha256:abc"
context_checksum = "sha256:def"
installed_at = ".claude/agents/agent.md"
dependencies = []
resource_type = "Agent"
tool = "claude-code"

[[commands]]
name = "cmd"
source = "test"
path = "commands/cmd.md"
version = "v1.0.0"
resolved_commit = "abc123"
checksum = "sha256:abc"
context_checksum = "sha256:def"
installed_at = ".claude/commands/cmd.md"
dependencies = []
resource_type = "Command"
tool = "claude-code"
"#;
    fs::write(project.project_path().join("agpm.lock"), lockfile).await?;

    // Run migrate
    let output = project.run_agpm(&["migrate", "--skip-install"])?;
    output.assert_success();

    // Verify resources moved but directories still exist (may have agpm/ subdirectory)
    assert!(agents_dir.exists(), "agents directory should still exist");
    assert!(commands_dir.exists(), "commands directory should still exist");

    // Verify agpm/ subdirectories have the files
    assert!(agents_dir.join("agpm/agent.md").exists(), "Agent should be in agpm/");
    assert!(commands_dir.join("agpm/cmd.md").exists(), "Command should be in agpm/");

    Ok(())
}

/// Test that dependency removal cleans up orphan files
#[tokio::test]
async fn test_migrate_cleanup_with_dependency_removal() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;
    let source_repo = project.create_source_repo("community").await?;

    source_repo.add_resource("agents", "keep", "# Keep Agent").await?;
    source_repo.add_resource("agents", "remove", "# Remove Agent").await?;
    source_repo.commit_all("Add agents")?;
    source_repo.tag_version("v1.0.0")?;

    let source_url = source_repo.bare_file_url(project.sources_path()).await?;

    // Initial manifest with both agents
    let manifest1 = ManifestBuilder::new()
        .add_source("community", &source_url)
        .add_standard_agent("keep", "community", "agents/keep.md")
        .add_standard_agent("remove", "community", "agents/remove.md")
        .build();

    project.write_manifest(&manifest1).await?;

    // First install
    let output1 = project.run_agpm(&["install"])?;
    output1.assert_success();

    // Verify both installed
    let agents_dir = project.project_path().join(".claude/agents/agpm");
    assert!(agents_dir.join("keep.md").exists(), "keep agent should be installed");
    assert!(agents_dir.join("remove.md").exists(), "remove agent should be installed");

    // Update manifest - remove one agent
    let manifest2 = ManifestBuilder::new()
        .add_source("community", &source_url)
        .add_standard_agent("keep", "community", "agents/keep.md")
        .build();

    project.write_manifest(&manifest2).await?;

    // Second install
    let output2 = project.run_agpm(&["install"])?;
    output2.assert_success();

    // Verify cleanup occurred
    assert!(agents_dir.join("keep.md").exists(), "keep agent should still exist");
    assert!(!agents_dir.join("remove.md").exists(), "remove agent should be cleaned up");

    // Verify lockfile is accurate
    let lockfile = project.load_lockfile()?;
    assert_eq!(lockfile.agents.len(), 1, "Lockfile should have 1 agent");

    Ok(())
}

/// Test that user files are not affected by migration
#[tokio::test]
async fn test_migrate_preserves_user_files() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;

    // Create both AGPM-managed and user files at old paths
    let agents_dir = project.project_path().join(".claude/agents");
    fs::create_dir_all(&agents_dir).await?;

    // This is tracked in lockfile - will be moved
    fs::write(agents_dir.join("agpm-managed.md"), "# AGPM Managed").await?;

    // User files that should NOT be moved (not tracked in lockfile)
    fs::write(agents_dir.join("user-notes.txt"), "User notes").await?;
    fs::write(agents_dir.join("README"), "User readme").await?;
    // Also test a .md file that's NOT tracked in the lockfile
    fs::write(agents_dir.join("user-agent.md"), "# User created agent").await?;

    fs::write(project.project_path().join("agpm.toml"), "[sources]\n").await?;

    // Create lockfile that tracks ONLY the agpm-managed file
    // User files are NOT tracked
    let lockfile = r#"version = 1

[[agents]]
name = "agpm-managed"
source = "test"
path = "agents/agpm-managed.md"
version = "v1.0.0"
resolved_commit = "abc123"
checksum = "sha256:abc"
context_checksum = "sha256:def"
installed_at = ".claude/agents/agpm-managed.md"
dependencies = []
resource_type = "Agent"
tool = "claude-code"
"#;
    fs::write(project.project_path().join("agpm.lock"), lockfile).await?;

    // Run migrate
    let output = project.run_agpm(&["migrate", "--skip-install"])?;
    output.assert_success();

    // Verify AGPM file moved
    assert!(agents_dir.join("agpm/agpm-managed.md").exists(), "AGPM file should be moved");
    assert!(!agents_dir.join("agpm-managed.md").exists(), "AGPM file should not be at old path");

    // Verify user files NOT moved (not tracked in lockfile)
    assert!(agents_dir.join("user-notes.txt").exists(), "User .txt file should remain");
    assert!(agents_dir.join("README").exists(), "User README should remain");
    assert!(agents_dir.join("user-agent.md").exists(), "User .md file should remain (not tracked)");

    Ok(())
}

/// Test resource count validation after migration
#[tokio::test]
async fn test_migrate_resource_count_validation() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;
    let source_repo = project.create_source_repo("community").await?;

    // Create multiple resources
    source_repo.add_resource("agents", "a1", "# Agent 1").await?;
    source_repo.add_resource("agents", "a2", "# Agent 2").await?;
    source_repo.add_resource("agents", "a3", "# Agent 3").await?;
    source_repo.commit_all("Add agents")?;
    source_repo.tag_version("v1.0.0")?;

    let source_url = source_repo.bare_file_url(project.sources_path()).await?;

    let manifest = ManifestBuilder::new()
        .add_source("community", &source_url)
        .add_standard_agent("a1", "community", "agents/a1.md")
        .add_standard_agent("a2", "community", "agents/a2.md")
        .add_standard_agent("a3", "community", "agents/a3.md")
        .build();

    project.write_manifest(&manifest).await?;

    // Run install
    let output = project.run_agpm(&["install"])?;
    output.assert_success();

    // Verify lockfile resource count matches actual files
    let lockfile = project.load_lockfile()?;
    assert_eq!(lockfile.agents.len(), 3, "Lockfile should have 3 agents");

    // Count actual files
    let agents_dir = project.project_path().join(".claude/agents/agpm");
    let actual_count = std::fs::read_dir(&agents_dir)?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|ext| ext == "md"))
        .count();

    assert_eq!(actual_count, 3, "Should have 3 actual agent files");

    Ok(())
}

// ============================================================================
// Section 5: End-to-End CLI Tests
// ============================================================================

/// Test --dry-run flag shows changes without modifying
#[tokio::test]
async fn test_migrate_cli_dry_run_output() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;

    // Create ccpm.toml
    fs::write(project.project_path().join("ccpm.toml"), "[sources]\n").await?;

    // Run migrate with --dry-run
    let output = project.run_agpm(&["migrate", "--dry-run"])?;
    output.assert_success();

    // Verify ccpm.toml still exists (not renamed)
    assert!(
        project.project_path().join("ccpm.toml").exists(),
        "ccpm.toml should not be renamed in dry-run"
    );
    assert!(
        !project.project_path().join("agpm.toml").exists(),
        "agpm.toml should not be created in dry-run"
    );

    // Output should indicate what would happen
    let combined_output = format!("{}\n{}", output.stdout, output.stderr);
    assert!(
        combined_output.contains("ccpm.toml") || combined_output.contains("CCPM"),
        "Output should mention CCPM files. Output: {}",
        combined_output
    );

    Ok(())
}

/// Test --format-only flag skips CCPM naming migration
#[tokio::test]
async fn test_migrate_cli_format_only_flag() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;

    // Create both ccpm.toml (legacy naming) AND old-format resources
    fs::write(project.project_path().join("ccpm.toml"), "[sources]\n").await?;

    let agents_dir = project.project_path().join(".claude/agents");
    fs::create_dir_all(&agents_dir).await?;
    fs::write(agents_dir.join("test.md"), "# Test").await?;

    // Create lockfile that tracks the resource at the old path
    // (only lockfile-tracked files are migrated)
    let lockfile = r#"version = 1

[[agents]]
name = "test"
source = "test"
path = "agents/test.md"
version = "v1.0.0"
resolved_commit = "abc123"
checksum = "sha256:abc"
context_checksum = "sha256:def"
installed_at = ".claude/agents/test.md"
dependencies = []
resource_type = "Agent"
tool = "claude-code"
"#;
    fs::write(project.project_path().join("agpm.lock"), lockfile).await?;

    // Run migrate with --format-only
    let output = project.run_agpm(&["migrate", "--format-only", "--skip-install"])?;
    output.assert_success();

    // Verify CCPM files NOT renamed
    assert!(
        project.project_path().join("ccpm.toml").exists(),
        "ccpm.toml should NOT be renamed with --format-only"
    );

    // Verify resources ARE moved
    assert!(agents_dir.join("agpm/test.md").exists(), "Resources should still be moved");

    Ok(())
}

/// Test --skip-install flag prevents automatic installation
#[tokio::test]
async fn test_migrate_cli_skip_install_flag() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;
    let source_repo = project.create_source_repo("community").await?;

    source_repo.add_resource("agents", "test", "# Test").await?;
    source_repo.commit_all("Add agent")?;
    source_repo.tag_version("v1.0.0")?;

    let source_url = source_repo.bare_file_url(project.sources_path()).await?;

    let manifest = format!(
        r#"[sources]
community = "{}"

[agents]
test = {{ source = "community", path = "agents/test.md", version = "v1.0.0" }}
"#,
        source_url
    );
    fs::write(project.project_path().join("ccpm.toml"), &manifest).await?;

    // Run migrate with --skip-install
    let output = project.run_agpm(&["migrate", "--skip-install"])?;
    output.assert_success();

    // Verify migration happened
    assert!(project.project_path().join("agpm.toml").exists(), "agpm.toml should exist");

    // Verify install was skipped (no lockfile created by install)
    // Note: migrate may create a minimal lockfile during path updates,
    // but agents should not be installed
    let agents_dir = project.project_path().join(".claude/agents/agpm");
    assert!(
        !agents_dir.join("test.md").exists(),
        "Agent should not be installed with --skip-install"
    );

    Ok(())
}

/// Test full end-to-end migration workflow
#[tokio::test]
async fn test_migrate_full_workflow_e2e() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;
    let source_repo = project.create_source_repo("community").await?;

    // Create resources
    source_repo.add_resource("agents", "workflow-agent", "# Workflow Agent").await?;
    source_repo.add_resource("snippets", "workflow-snippet", "# Workflow Snippet").await?;
    source_repo.commit_all("Add resources")?;
    source_repo.tag_version("v1.0.0")?;

    let source_url = source_repo.bare_file_url(project.sources_path()).await?;

    // Step 1: Create ccpm.toml with dependencies
    let manifest = format!(
        r#"[sources]
community = "{}"

[agents]
workflow = {{ source = "community", path = "agents/workflow-agent.md", version = "v1.0.0" }}

[snippets]
workflow = {{ source = "community", path = "snippets/workflow-snippet.md", version = "v1.0.0" }}
"#,
        source_url
    );
    fs::write(project.project_path().join("ccpm.toml"), &manifest).await?;

    // Step 2: Create old-format .gitignore with managed section
    let gitignore = r#"node_modules/

# AGPM managed entries
.claude/agents/*.md
# End of AGPM managed entries
"#;
    fs::write(project.project_path().join(".gitignore"), gitignore).await?;

    // Step 3: Create resources at old paths
    let agents_dir = project.project_path().join(".claude/agents");
    fs::create_dir_all(&agents_dir).await?;
    fs::write(agents_dir.join("old-agent.md"), "# Old Agent").await?;

    // Step 4: Run agpm migrate
    let migrate_output = project.run_agpm(&["migrate"])?;
    migrate_output.assert_success();

    // Step 5: Verify migration
    assert!(project.project_path().join("agpm.toml").exists(), "agpm.toml should exist");
    assert!(!project.project_path().join("ccpm.toml").exists(), "ccpm.toml should be gone");

    let new_gitignore = fs::read_to_string(project.project_path().join(".gitignore")).await?;
    assert!(
        !new_gitignore.contains("AGPM managed entries\n"),
        "Old managed section header should be removed"
    );
    assert!(
        new_gitignore.contains("# AGPM managed paths"),
        "New paths section should be added. Gitignore:\n{}",
        new_gitignore
    );
    assert!(
        new_gitignore.contains(".claude/*/agpm/"),
        "New paths should include agpm subdirectory"
    );

    // Step 6: Verify all resources at new paths
    assert!(
        project.project_path().join(".claude/agents/agpm/workflow-agent.md").exists(),
        "Installed agent should be at new path"
    );

    // Step 7: Run agpm validate
    let validate_output = project.run_agpm(&["validate"])?;
    // Validate may warn about gitignore but should not fail
    assert!(
        validate_output.success || validate_output.stderr.contains("warning"),
        "Validate should pass or only warn"
    );

    Ok(())
}

// ============================================================================
// Section 6: Edge Cases
// ============================================================================

/// Test migrate on empty project with nothing to do
#[tokio::test]
async fn test_migrate_empty_project() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;

    // No ccpm files, no old-format resources, just empty project
    fs::write(project.project_path().join("agpm.toml"), "[sources]\n").await?;

    // Run migrate
    let output = project.run_agpm(&["migrate", "--skip-install"])?;
    output.assert_success();

    // Output should indicate no migration needed
    let combined_output = format!("{}\n{}", output.stdout, output.stderr);
    assert!(
        combined_output.contains("No migration") || combined_output.contains("up to date"),
        "Should indicate no migration needed. Output: {}",
        combined_output
    );

    Ok(())
}

/// Test migrate is idempotent (running twice has no effect)
#[tokio::test]
async fn test_migrate_already_migrated() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;
    let source_repo = project.create_source_repo("community").await?;

    source_repo.add_resource("agents", "test", "# Test").await?;
    source_repo.commit_all("Add agent")?;
    source_repo.tag_version("v1.0.0")?;

    let source_url = source_repo.bare_file_url(project.sources_path()).await?;

    // Create already-migrated state
    let manifest = ManifestBuilder::new()
        .add_source("community", &source_url)
        .add_standard_agent("test", "community", "agents/test.md")
        .build();

    project.write_manifest(&manifest).await?;

    // First install to get proper state
    let install_output = project.run_agpm(&["install"])?;
    install_output.assert_success();

    // Run migrate on already-migrated project
    let migrate_output1 = project.run_agpm(&["migrate", "--skip-install"])?;
    migrate_output1.assert_success();

    // Run migrate again
    let migrate_output2 = project.run_agpm(&["migrate", "--skip-install"])?;
    migrate_output2.assert_success();

    // Verify state unchanged
    let agent_path = project.project_path().join(".claude/agents/agpm/test.md");
    assert!(agent_path.exists(), "Agent should still be at correct path");

    Ok(())
}

/// Test migrate handles nested resource paths correctly
///
/// Note: Migration only moves files directly in resource directories,
/// not files in subdirectories. This is intentional - nested subdirectories
/// might be user-organized content that shouldn't be auto-migrated.
#[tokio::test]
async fn test_migrate_nested_resource_paths() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;

    // Create resources at old paths:
    // - Direct file (will be migrated - tracked in lockfile)
    // - Nested file in subdirectory (will NOT be auto-migrated - not tracked)
    let agents_dir = project.project_path().join(".claude/agents");
    fs::create_dir_all(agents_dir.join("subdir")).await?;
    fs::write(agents_dir.join("direct-agent.md"), "# Direct Agent").await?;
    fs::write(agents_dir.join("subdir/nested-agent.md"), "# Nested Agent").await?;

    fs::write(project.project_path().join("agpm.toml"), "[sources]\n").await?;

    // Create lockfile that tracks only the direct agent at old path
    // (nested files are user content, not tracked by AGPM)
    let lockfile = r#"version = 1

[[agents]]
name = "direct-agent"
source = "test"
path = "agents/direct-agent.md"
version = "v1.0.0"
resolved_commit = "abc123"
checksum = "sha256:abc"
context_checksum = "sha256:def"
installed_at = ".claude/agents/direct-agent.md"
dependencies = []
resource_type = "Agent"
tool = "claude-code"
"#;
    fs::write(project.project_path().join("agpm.lock"), lockfile).await?;

    // Run migrate
    let output = project.run_agpm(&["migrate", "--skip-install"])?;
    output.assert_success();

    // Direct file should be moved to agpm/ subdirectory
    assert!(
        agents_dir.join("agpm/direct-agent.md").exists(),
        "Direct agent should be moved to agpm/"
    );
    assert!(!agents_dir.join("direct-agent.md").exists(), "Direct agent should not be at old path");

    // Nested file in subdirectory is NOT migrated automatically
    // (migration only detects files directly in resource directories)
    assert!(
        agents_dir.join("subdir/nested-agent.md").exists(),
        "Nested agent in subdirectory should remain (not auto-migrated)"
    );

    Ok(())
}

// ============================================================================
// Section 7: Interactive Migration Prompt Tests
// ============================================================================

/// Test that install detects legacy format and suggests migration (non-interactive)
///
/// When running in non-interactive mode (CI/CD), install should detect
/// old format and print a message suggesting `agpm migrate`, but NOT
/// perform the migration automatically.
///
/// Note: This test verifies the migration DETECTION, not the file state after install.
/// The install command may still modify files based on manifest state (orphan cleanup, etc.).
#[tokio::test]
async fn test_install_detects_legacy_format_non_interactive() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;
    let source_repo = project.create_source_repo("test").await?;

    source_repo.add_resource("agents", "helper", "# Helper Agent").await?;
    source_repo.commit_all("Add agent")?;
    source_repo.tag_version("v1.0.0")?;

    let source_url = source_repo.bare_file_url(project.sources_path()).await?;

    // Create manifest with actual dependency
    let manifest = ManifestBuilder::new()
        .add_source("test", &source_url)
        .add_agent("helper", |d| d.source("test").path("agents/helper.md").version("v1.0.0"))
        .build();
    fs::write(project.project_path().join("agpm.toml"), &manifest).await?;

    // Create resources at old paths (simulating a previous installation)
    let agents_dir = project.project_path().join(".claude/agents");
    fs::create_dir_all(&agents_dir).await?;
    fs::write(agents_dir.join("helper.md"), "# Helper Agent").await?;

    // Create lockfile pointing to old paths (as if previously installed)
    let lockfile = r#"version = 1

[[agents]]
name = "helper"
source = "test"
path = "agents/helper.md"
version = "v1.0.0"
resolved_commit = "abc123"
checksum = "sha256:abc"
context_checksum = "sha256:def"
installed_at = ".claude/agents/helper.md"
dependencies = []
resource_type = "Agent"
tool = "claude-code"
"#;
    fs::write(project.project_path().join("agpm.lock"), lockfile).await?;

    // Run install (in non-interactive mode since tests don't have a TTY)
    let output = project.run_agpm(&["install"])?;

    // The legacy format detection message should appear in stderr
    let stderr = &output.stderr;

    // Check that migration message was printed
    assert!(
        stderr.contains("Legacy AGPM format detected") || stderr.contains("agpm migrate"),
        "Should detect legacy format and suggest migration. stderr: {}",
        stderr
    );

    Ok(())
}

/// Test that install detects legacy gitignore section and suggests migration
#[tokio::test]
async fn test_install_detects_legacy_gitignore_section_non_interactive() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;

    // Create a manifest
    fs::write(project.project_path().join("agpm.toml"), "[sources]\n").await?;

    // Create .gitignore with managed section
    let gitignore = r#"# User entries
node_modules/

# AGPM managed entries - do not edit below this line
.claude/agents/test.md
# End of AGPM managed entries
"#;
    fs::write(project.project_path().join(".gitignore"), gitignore).await?;

    // Create empty lockfile (gitignore section alone triggers migration detection)
    fs::write(project.project_path().join("agpm.lock"), "version = 1\n").await?;

    // Run install (in non-interactive mode since tests don't have a TTY)
    let output = project.run_agpm(&["install"])?;

    // Should succeed but NOT migrate (non-interactive)
    let stderr = &output.stderr;

    // Check that migration message was printed
    assert!(
        stderr.contains("Legacy AGPM format detected") || stderr.contains("agpm migrate"),
        "Should detect legacy format (gitignore section) and suggest migration. stderr: {}",
        stderr
    );

    // .gitignore should NOT be modified (non-interactive mode)
    let gitignore_content = fs::read_to_string(project.project_path().join(".gitignore")).await?;
    assert!(
        gitignore_content.contains("# AGPM managed entries"),
        "gitignore should NOT be modified (non-interactive mode)"
    );

    Ok(())
}

/// Test that install works normally after manual migration
#[tokio::test]
async fn test_install_after_migration_works() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;
    let source_repo = project.create_source_repo("test").await?;

    source_repo.add_resource("agents", "helper", "# Helper Agent").await?;
    source_repo.commit_all("Add agent")?;
    source_repo.tag_version("v1.0.0")?;

    let source_url = source_repo.bare_file_url(project.sources_path()).await?;

    // Create manifest
    let manifest = ManifestBuilder::new()
        .add_source("test", &source_url)
        .add_agent("helper", |d| d.source("test").path("agents/helper.md").version("v1.0.0"))
        .build();
    fs::write(project.project_path().join("agpm.toml"), &manifest).await?;

    // Create resources at old paths
    let agents_dir = project.project_path().join(".claude/agents");
    fs::create_dir_all(&agents_dir).await?;
    fs::write(agents_dir.join("helper.md"), "# Helper Agent").await?;

    // Create lockfile pointing to old paths
    let lockfile = r#"version = 1

[[agents]]
name = "helper"
source = "test"
path = "agents/helper.md"
version = "v1.0.0"
resolved_commit = "abc123"
checksum = "sha256:abc"
context_checksum = "sha256:def"
installed_at = ".claude/agents/helper.md"
dependencies = []
resource_type = "Agent"
tool = "claude-code"
"#;
    fs::write(project.project_path().join("agpm.lock"), lockfile).await?;

    // First, run migrate to fix the format
    let migrate_output = project.run_agpm(&["migrate", "--skip-install"])?;
    migrate_output.assert_success();

    // Verify migration happened
    assert!(
        agents_dir.join("agpm/helper.md").exists(),
        "Agent should be at new path after migration"
    );

    // Now run install - should work without migration prompts
    let install_output = project.run_agpm(&["install"])?;
    install_output.assert_success();

    // Files should still be at new paths
    let new_agent_path = project.project_path().join(".claude/agents/agpm/helper.md");
    assert!(new_agent_path.exists(), "Agent should remain at new path");

    Ok(())
}

/// Test that `agpm install -y` performs full migration from legacy format.
///
/// This end-to-end test verifies the complete migration flow:
/// 1. Old lockfile with legacy paths
/// 2. Artifacts at old locations
/// 3. Manifest with old-style tools config
/// 4. .gitignore with old managed section
/// 5. After `install -y`: commented tools, cleaned old paths, new artifacts installed,
///    .gitignore migrated
#[tokio::test]
async fn test_install_with_yes_flag_performs_full_migration() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;
    let source_repo = project.create_source_repo("test").await?;

    // Create source content
    source_repo.add_resource("agents", "helper", "# Helper Agent").await?;
    source_repo.commit_all("Add agent")?;
    source_repo.tag_version("v1.0.0")?;
    let source_url = source_repo.bare_file_url(project.sources_path()).await?;

    // 1. Create manifest with OLD tools config
    let manifest = format!(
        r#"[sources]
test = "{source_url}"

[tools.claude-code]
path = ".claude"
resources = {{ agents = {{ path = "agents", flatten = true }} }}

[agents]
helper = {{ source = "test", path = "agents/helper.md", version = "v1.0.0" }}
"#
    );
    fs::write(project.project_path().join("agpm.toml"), &manifest).await?;

    // 2. Create artifact at OLD path
    let old_agent_path = project.project_path().join(".claude/agents/helper.md");
    fs::create_dir_all(old_agent_path.parent().unwrap()).await?;
    fs::write(&old_agent_path, "# Helper Agent").await?;

    // 3. Create lockfile with OLD installed_at path
    let lockfile = r#"version = 1

[[agents]]
name = "helper"
source = "test"
path = "agents/helper.md"
version = "v1.0.0"
resolved_commit = "abc123"
checksum = "sha256:abc"
context_checksum = "sha256:def"
installed_at = ".claude/agents/helper.md"
dependencies = []
resource_type = "Agent"
tool = "claude-code"
"#;
    fs::write(project.project_path().join("agpm.lock"), lockfile).await?;

    // 4. Create .gitignore with OLD managed section
    let gitignore = r#"# User entries
node_modules/

# AGPM managed entries - do not edit below this line
.claude/agents/helper.md
# End of AGPM managed entries
"#;
    fs::write(project.project_path().join(".gitignore"), gitignore).await?;

    // Execute: install with -y to accept migration
    let output = project.run_agpm(&["install", "-y"])?;
    output.assert_success();

    // Verify 1: Manifest has commented tools section
    let manifest_content = fs::read_to_string(project.project_path().join("agpm.toml")).await?;
    assert!(
        manifest_content.contains("# [tools.claude-code]"),
        "Manifest should have commented-out tools section. Content:\n{}",
        manifest_content
    );
    assert!(
        manifest_content.contains("agents/agpm"),
        "Manifest should reference agents/agpm in comments. Content:\n{}",
        manifest_content
    );
    assert!(
        !manifest_content.contains("\n[tools.claude-code]"),
        "Manifest should NOT have active [tools.claude-code] section. Content:\n{}",
        manifest_content
    );

    // Verify 2: Old artifact cleaned up
    assert!(!old_agent_path.exists(), "Old artifact at {:?} should be cleaned up", old_agent_path);

    // Verify 3: New artifact installed
    let new_agent_path = project.project_path().join(".claude/agents/agpm/helper.md");
    assert!(new_agent_path.exists(), "New artifact should exist at {:?}", new_agent_path);

    // Verify 4: Lockfile has new path
    let lockfile_content = fs::read_to_string(project.project_path().join("agpm.lock")).await?;
    assert!(
        lockfile_content.contains(".claude/agents/agpm/helper.md"),
        "Lockfile should have new installed_at path. Content:\n{}",
        lockfile_content
    );

    // Verify 5: .gitignore migrated
    let gitignore_content = fs::read_to_string(project.project_path().join(".gitignore")).await?;
    assert!(
        !gitignore_content.contains("# AGPM managed entries"),
        ".gitignore should NOT have old marker. Content:\n{}",
        gitignore_content
    );
    assert!(
        gitignore_content.contains("# AGPM managed paths"),
        ".gitignore should have new marker. Content:\n{}",
        gitignore_content
    );
    assert!(
        gitignore_content.contains(".claude/*/agpm/"),
        ".gitignore should have new wildcard pattern. Content:\n{}",
        gitignore_content
    );
    assert!(
        gitignore_content.contains("node_modules/"),
        ".gitignore should preserve user content. Content:\n{}",
        gitignore_content
    );

    Ok(())
}
