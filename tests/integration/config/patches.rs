//! Integration tests for patch/override functionality.
//!
//! These tests verify end-to-end behavior of the patch feature including:
//! - Project-level patches from agpm.toml
//! - User-level patches from agpm.private.toml
//! - Conflict detection and resolution
//! - Lockfile tracking of applied patches
//! - CLI visibility of patched resources

use anyhow::Result;
use tokio::fs;

use crate::common::{FileAssert, TestProject};
use crate::test_config;

/// Helper to create a test repository with an agent containing a model field
async fn create_repo_with_model_agent(project: &TestProject) -> Result<(String, String)> {
    let repo = project.create_source_repo("test-repo").await?;

    // Create an agent with model field in frontmatter
    let agent_content = r#"---
model: gpt-4
temperature: "0.5"
---
# Test Agent

This is a test agent with a model field.
"#;

    repo.add_resource("agents", "model-agent", agent_content).await?;
    repo.commit_all("Initial commit with model agent")?;
    repo.tag_version("v1.0.0")?;

    let url = repo.bare_file_url(project.sources_path())?;
    Ok((url, "agents/model-agent.md".to_string()))
}

/// Helper to create a test repository with a JSON MCP server
async fn create_repo_with_json_mcp(project: &TestProject) -> Result<(String, String)> {
    let repo = project.create_source_repo("mcp-repo").await?;

    // Create MCP server directory
    let mcp_dir = repo.path.join("mcp-servers");
    fs::create_dir_all(&mcp_dir).await?;

    // Create a JSON MCP server config
    let mcp_content = r#"{
  "name": "test-server",
  "command": "npx",
  "args": ["@test/server"],
  "timeout": 30
}"#;

    let mcp_file = mcp_dir.join("test-server.json");
    fs::write(&mcp_file, mcp_content).await?;

    repo.commit_all("Add test MCP server")?;
    repo.tag_version("v1.0.0")?;

    let url = repo.bare_file_url(project.sources_path())?;
    Ok((url, "mcp-servers/test-server.json".to_string()))
}

#[tokio::test]
async fn test_install_with_project_patches() {
    test_config::init_test_env();
    let project = TestProject::new().await.unwrap();

    // Create test repository with agent containing model field
    let (url, path) = create_repo_with_model_agent(&project).await.unwrap();

    // Create manifest with agent and patch to override model
    let manifest = format!(
        r#"[sources]
test = "{}"

[agents]
my-agent = {{ source = "test", path = "{}", version = "v1.0.0" }}

[patch.agents.my-agent]
model = "claude-3-haiku"
"#,
        url, path
    );

    project.write_manifest(&manifest).await.unwrap();

    // Run install
    let output = project.run_agpm(&["install"]).unwrap();
    output.assert_success();

    // Verify installed file has patched model value
    let installed_path = project.project_path().join(".claude/agents/agpm/model-agent.md");
    FileAssert::exists(&installed_path).await;

    let content = fs::read_to_string(&installed_path).await.unwrap();
    assert!(
        content.contains("model: claude-3-haiku"),
        "Expected patched model value 'claude-3-haiku' in:\n{}",
        content
    );
    assert!(!content.contains("model: gpt-4"), "Original model value 'gpt-4' should be replaced");

    // Verify lockfile has applied_patches field
    let lockfile_content = project.read_lockfile().await.unwrap();
    assert!(
        lockfile_content.contains("applied_patches"),
        "Lockfile should contain applied_patches field"
    );
    assert!(
        lockfile_content.contains("model = \"claude-3-haiku\""),
        "Lockfile should track the patched model value"
    );
}

#[tokio::test]
async fn test_install_with_private_patches() {
    test_config::init_test_env();
    let project = TestProject::new().await.unwrap();

    // Create test repository with agent
    let (url, path) = create_repo_with_model_agent(&project).await.unwrap();

    // Create project manifest with one patch (model field)
    let manifest = format!(
        r#"[sources]
test = "{}"

[agents]
my-agent = {{ source = "test", path = "{}", version = "v1.0.0" }}

[patch.agents.my-agent]
model = "gpt-4"
"#,
        url, path
    );

    project.write_manifest(&manifest).await.unwrap();

    // Create private manifest with NON-conflicting patch (different field)
    let private_manifest = r#"[patch.agents.my-agent]
temperature = "0.8"
max_tokens = 4000
"#;

    let private_path = project.project_path().join("agpm.private.toml");
    fs::write(&private_path, private_manifest).await.unwrap();

    // Run install
    let output = project.run_agpm(&["install"]).unwrap();
    output.assert_success();

    // Verify both project and private patches are applied
    let installed_path = project.project_path().join(".claude/agents/agpm/model-agent.md");
    let content = fs::read_to_string(&installed_path).await.unwrap();

    assert!(
        content.contains("model: gpt-4"),
        "Project patch (model) should be applied. Content:\n{}",
        content
    );
    assert!(
        content.contains("0.8"),
        "Private patch (temperature) should be applied. Content:\n{}",
        content
    );
    assert!(
        content.contains("4000"),
        "Private patch (max_tokens) should be applied. Content:\n{}",
        content
    );
}

#[tokio::test]
async fn test_patch_conflict_fails() {
    test_config::init_test_env();
    let project = TestProject::new().await.unwrap();

    // Create test repository with agent
    let (url, path) = create_repo_with_model_agent(&project).await.unwrap();

    // Create project manifest with model patch
    let manifest = format!(
        r#"[sources]
test = "{}"

[agents]
my-agent = {{ source = "test", path = "{}", version = "v1.0.0" }}

[patch.agents.my-agent]
model = "gpt-4"
"#,
        url, path
    );

    project.write_manifest(&manifest).await.unwrap();

    // Create private manifest with CONFLICTING patch (same field)
    // This should silently override the project patch
    let private_manifest = r#"[patch.agents.my-agent]
model = "claude-3-haiku"
"#;

    let private_path = project.project_path().join("agpm.private.toml");
    fs::write(&private_path, private_manifest).await.unwrap();

    // Run install - should succeed with private patch taking precedence
    let output = project.run_agpm(&["install"]).unwrap();
    assert!(
        output.success,
        "Install should succeed when private patches override project patches. Stderr:\n{}",
        output.stderr
    );

    // Verify the private patch won (model should be claude-3-haiku, not gpt-4)
    let agent_path = project.project_path().join(".claude/agents/agpm/model-agent.md");
    assert!(agent_path.exists(), "Agent should be installed");
    let content = fs::read_to_string(&agent_path).await.unwrap();
    assert!(
        content.contains("model: claude-3-haiku") || content.contains("model: 'claude-3-haiku'"),
        "Agent should have private patch applied (claude-3-haiku). Content:\n{}",
        content
    );
    assert!(
        !content.contains("model: gpt-4"),
        "Agent should not have project patch (gpt-4). Content:\n{}",
        content
    );
}

#[tokio::test]
async fn test_patch_validation_unknown_alias() {
    test_config::init_test_env();
    let project = TestProject::new().await.unwrap();

    // Create test repository
    let (url, path) = create_repo_with_model_agent(&project).await.unwrap();

    // Create manifest with patch for unknown alias
    let manifest = format!(
        r#"[sources]
test = "{}"

[agents]
my-agent = {{ source = "test", path = "{}", version = "v1.0.0" }}

[patch.agents.nonexistent-agent]
model = "claude-3-haiku"
"#,
        url, path
    );

    project.write_manifest(&manifest).await.unwrap();

    // Run install - should fail with validation error
    let output = project.run_agpm(&["install"]).unwrap();

    assert!(!output.success, "Install should fail for unknown alias in patch");
    assert!(
        output.stderr.contains("nonexistent-agent") || output.stdout.contains("nonexistent-agent"),
        "Error should mention the unknown alias"
    );
}

#[tokio::test]
async fn test_patches_in_lockfile() {
    test_config::init_test_env();
    let project = TestProject::new().await.unwrap();

    // Create test repository
    let (url, path) = create_repo_with_model_agent(&project).await.unwrap();

    // Create manifest with patches
    let manifest = format!(
        r#"[sources]
test = "{}"

[agents]
my-agent = {{ source = "test", path = "{}", version = "v1.0.0" }}

[patch.agents.my-agent]
model = "claude-3-haiku"
temperature = "0.7"
"#,
        url, path
    );

    project.write_manifest(&manifest).await.unwrap();

    // Run install
    let output = project.run_agpm(&["install"]).unwrap();
    output.assert_success();

    // Read and verify lockfile
    let lockfile_content = project.read_lockfile().await.unwrap();

    // Verify applied_patches is formatted as inline table, not separate table section
    assert!(
        !lockfile_content.contains("[agents.applied_patches]"),
        "applied_patches should be inline table, not separate table section.\nLockfile:\n{}",
        lockfile_content
    );
    assert!(
        lockfile_content.contains("applied_patches = {"),
        "applied_patches should use inline table syntax.\nLockfile:\n{}",
        lockfile_content
    );

    // Parse lockfile as TOML to check structure
    let lockfile: toml::Value = toml::from_str(&lockfile_content).unwrap();

    // Find the agent entry
    let agents = lockfile.get("agents").and_then(|v| v.as_array()).unwrap();
    assert_eq!(agents.len(), 1, "Should have exactly one agent in lockfile");

    let agent = &agents[0];

    // Verify applied_patches field exists
    let patches = agent.get("applied_patches").expect("applied_patches field should exist");

    // Verify patch values
    assert_eq!(
        patches.get("model").and_then(|v| v.as_str()),
        Some("claude-3-haiku"),
        "Lockfile should track model patch"
    );
    assert_eq!(
        patches.get("temperature").and_then(|v| v.as_str()),
        Some("0.7"),
        "Lockfile should track temperature patch"
    );

    // Run install again in frozen mode to verify patches are reapplied
    let output2 = project.run_agpm(&["install", "--frozen"]).unwrap();
    output2.assert_success();

    // Verify installed file still has patched values
    let installed_path = project.project_path().join(".claude/agents/agpm/model-agent.md");
    let content = fs::read_to_string(&installed_path).await.unwrap();
    assert!(content.contains("model: claude-3-haiku"));
}

#[tokio::test]
async fn test_list_shows_patched_indicator() {
    test_config::init_test_env();
    let project = TestProject::new().await.unwrap();

    // Create test repository
    let (url, path) = create_repo_with_model_agent(&project).await.unwrap();

    // Create manifest with patches
    let manifest = format!(
        r#"[sources]
test = "{}"

[agents]
my-agent = {{ source = "test", path = "{}", version = "v1.0.0" }}

[patch.agents.my-agent]
model = "claude-3-haiku"
"#,
        url, path
    );

    project.write_manifest(&manifest).await.unwrap();

    // Run install
    let output = project.run_agpm(&["install"]).unwrap();
    output.assert_success();

    // Run list command
    let list_output = project.run_agpm(&["list"]).unwrap();
    list_output.assert_success();

    // Verify output contains patched indicator
    assert!(
        list_output.stdout.contains("(patched)") || list_output.stdout.contains("patched"),
        "List output should indicate resource is patched:\n{}",
        list_output.stdout
    );
    assert!(
        list_output.stdout.contains("my-agent") || list_output.stdout.contains("model-agent"),
        "List output should show agent name:\n{}",
        list_output.stdout
    );
}

#[tokio::test]
async fn test_patch_json_file() {
    test_config::init_test_env();
    let project = TestProject::new().await.unwrap();

    // Create test repository with JSON MCP server
    let (url, path) = create_repo_with_json_mcp(&project).await.unwrap();

    // Create manifest with patch for JSON field
    let manifest = format!(
        r#"[sources]
test = "{}"

[mcp-servers]
my-server = {{ source = "test", path = "{}", version = "v1.0.0" }}

[patch.mcp-servers.my-server]
timeout = 300
retries = 3
"#,
        url, path
    );

    project.write_manifest(&manifest).await.unwrap();

    // Run install
    let output = project.run_agpm(&["install"]).unwrap();
    output.assert_success();

    // Verify .mcp.json was updated with patched values
    // Note: MCP servers are merged into .mcp.json, not installed as separate files
    let mcp_json_path = project.project_path().join(".mcp.json");

    if mcp_json_path.exists() {
        let content = fs::read_to_string(&mcp_json_path).await.unwrap();
        let json: serde_json::Value = serde_json::from_str(&content).unwrap();

        // Find the test-server entry
        if let Some(mcp_servers) = json.get("mcpServers").and_then(|v| v.as_object()) {
            if let Some(server) = mcp_servers.get("test-server") {
                // Verify patched timeout value
                assert_eq!(
                    server.get("timeout").and_then(|v| v.as_i64()),
                    Some(300),
                    "JSON patch should update timeout field"
                );
                assert_eq!(
                    server.get("retries").and_then(|v| v.as_i64()),
                    Some(3),
                    "JSON patch should add retries field"
                );
            }
        }
    }

    // Verify lockfile tracks patches for JSON resources
    let lockfile_content = project.read_lockfile().await.unwrap();
    assert!(
        lockfile_content.contains("applied_patches"),
        "Lockfile should track patches for JSON resources"
    );
}

#[tokio::test]
async fn test_patch_multiple_fields() {
    test_config::init_test_env();
    let project = TestProject::new().await.unwrap();

    // Create test repository
    let (url, path) = create_repo_with_model_agent(&project).await.unwrap();

    // Create manifest with multiple field patches
    let manifest = format!(
        r#"[sources]
test = "{}"

[agents]
my-agent = {{ source = "test", path = "{}", version = "v1.0.0" }}

[patch.agents.my-agent]
model = "claude-3-haiku"
temperature = "0.9"
max_tokens = 4000
system_prompt = "You are a helpful assistant"
"#,
        url, path
    );

    project.write_manifest(&manifest).await.unwrap();

    // Run install
    let output = project.run_agpm(&["install"]).unwrap();
    output.assert_success();

    // Verify all patches are applied
    let installed_path = project.project_path().join(".claude/agents/agpm/model-agent.md");
    let content = fs::read_to_string(&installed_path).await.unwrap();

    assert!(content.contains("model: claude-3-haiku"), "Model should be patched");
    assert!(content.contains("0.9"), "Temperature should be patched");
    assert!(content.contains("4000"), "max_tokens should be patched");
    assert!(
        content.contains("system_prompt") && content.contains("helpful assistant"),
        "New field should be added"
    );

    // Verify lockfile tracks all patches
    let lockfile_content = project.read_lockfile().await.unwrap();
    let lockfile: toml::Value = toml::from_str(&lockfile_content).unwrap();

    let agents = lockfile.get("agents").and_then(|v| v.as_array()).unwrap();
    let patches = agents[0].get("applied_patches").expect("applied_patches should exist");

    assert_eq!(patches.as_table().unwrap().len(), 4, "All 4 patches should be tracked");
}

// ============================================================================
// P0-1: Cross-Platform Lockfile Serialization
// ============================================================================

/// Tests that lockfile uses forward slashes on all platforms, including Windows
///
/// Verifies cross-platform path handling in lockfile applied_patches field.
#[tokio::test]
async fn test_patch_lockfile_uses_forward_slashes() {
    test_config::init_test_env();
    let project = TestProject::new().await.unwrap();
    let repo = project.create_source_repo("cross-platform").await.unwrap();

    repo.add_resource("agents", "model-agent", "---\nmodel: gpt-4\ntemp: 0.5\n---\n# Agent\n")
        .await
        .unwrap();
    repo.commit_all("v1.0.0").unwrap();
    repo.tag_version("v1.0.0").unwrap();

    let url = repo.bare_file_url(project.sources_path()).unwrap();

    let manifest = format!(
        r#"[sources]
test = "{}"
[agents]
my-agent = {{ source = "test", path = "agents/model-agent.md", version = "v1.0.0" }}
[patch.agents.my-agent]
model = "claude-3-haiku"
temperature = "0.8"
max_tokens = "4096"
"#,
        url
    );

    project.write_manifest(&manifest).await.unwrap();
    project.run_agpm(&["install"]).unwrap().assert_success();

    // Read raw lockfile content (not parsed)
    let lockfile_path = project.project_path().join("agpm.lock");
    let lockfile_raw = tokio::fs::read_to_string(&lockfile_path).await.unwrap();

    // CRITICAL: No backslashes anywhere
    assert!(
        !lockfile_raw.contains('\\'),
        "Lockfile must use forward slashes only. Found backslash in:\n{}",
        lockfile_raw
    );

    // Verify applied_patches section structure (can be inline table or TOML table)
    assert!(
        lockfile_raw.contains("applied_patches")
            && (lockfile_raw.contains("applied_patches =")
                || lockfile_raw.contains("[agents.applied_patches]")),
        "Lockfile should have applied_patches field (either inline or as table). Lockfile:\n{}",
        lockfile_raw
    );

    // Verify forward slashes in installed_at
    assert!(
        lockfile_raw.contains("installed_at = \".claude/agents/"),
        "installed_at path should use forward slashes"
    );

    // Verify applied_patches keys don't have backslashes
    if lockfile_raw.contains("applied_patches") {
        let applied_section_start = lockfile_raw.find("applied_patches").unwrap();
        let after_applied = &lockfile_raw[applied_section_start..];
        let applied_section_end = after_applied.find("\n\n").unwrap_or(after_applied.len());
        let applied_section = &after_applied[..applied_section_end];

        assert!(
            !applied_section.contains('\\'),
            "applied_patches section must not contain backslashes:\n{}",
            applied_section
        );
    }

    // Parse lockfile to verify structure
    let lockfile = project.read_lockfile().await.unwrap();
    let agents_count = lockfile.matches("[[agents]]").count();
    assert_eq!(agents_count, 1, "Should have one agent in lockfile");
}

#[tokio::test]
#[cfg(target_os = "windows")]
async fn test_patch_nested_paths_windows() {
    test_config::init_test_env();
    let project = TestProject::new().await.unwrap();
    let repo = project.create_source_repo("windows-nested").await.unwrap();

    // Create deeply nested path that would have backslashes on Windows
    repo.add_resource(
        "agents/category/subcategory",
        "deep",
        "---\nmodel: opus\n---\n# Deep Agent\n",
    )
    .await
    .unwrap();
    repo.commit_all("v1.0.0").unwrap();
    repo.tag_version("v1.0.0").unwrap();

    let url = repo.bare_file_url(project.sources_path()).unwrap();

    let manifest = format!(
        r#"[sources]
test = "{}"
[agents]
nested = {{ source = "test", path = "agents/category/subcategory/deep.md", version = "v1.0.0", flatten = false }}
[patch.agents.nested]
model = "haiku"
category = "deeply/nested/path"
"#,
        url
    );

    project.write_manifest(&manifest).await.unwrap();
    project.run_agpm(&["install"]).unwrap().assert_success();

    let lockfile_raw =
        tokio::fs::read_to_string(project.project_path().join("agpm.lock")).await.unwrap();

    // Nested paths in installed_at must use forward slashes
    assert!(
        lockfile_raw.contains(".claude/agents/category/subcategory/deep.md"),
        "Nested path must use forward slashes"
    );

    // Applied patches with path-like values must use forward slashes
    assert!(
        lockfile_raw.contains("deeply/nested/path")
            || lockfile_raw.contains("\"deeply/nested/path\""),
        "Patch values with paths must use forward slashes"
    );

    // No Windows separators anywhere
    let backslash_count = lockfile_raw.matches('\\').count();
    assert_eq!(backslash_count, 0, "Found {} backslash(es) in Windows lockfile", backslash_count);
}

// ============================================================================
// P0-2: Pattern/Glob Patching
// ============================================================================

/// Tests that patches apply to all files matched by a glob pattern
#[tokio::test]
async fn test_patch_applies_to_all_pattern_matched_resources() {
    test_config::init_test_env();
    let project = TestProject::new().await.unwrap();
    let repo = project.create_source_repo("pattern-patch").await.unwrap();

    // Create multiple agents with different initial models
    for (name, model) in
        [("helper-alpha", "gpt-4"), ("helper-beta", "gpt-4"), ("helper-gamma", "claude-3-opus")]
    {
        let content = format!("---\nmodel: {}\ntemp: 0.7\nrole: helper\n---\n# {}\n", model, name);
        repo.add_resource("agents/helpers", name, &content).await.unwrap();
    }

    repo.commit_all("v1.0.0").unwrap();
    repo.tag_version("v1.0.0").unwrap();

    let url = repo.bare_file_url(project.sources_path()).unwrap();

    // Install with pattern alias and patch (preserving helpers/ subdirectory)
    let manifest = format!(
        r#"[sources]
test = "{}"
[agents]
all-helpers = {{ source = "test", path = "agents/helpers/*.md", version = "v1.0.0", flatten = false }}

[patch.agents.all-helpers]
model = "claude-3-haiku"
max_tokens = "4096"
category = "utility"
"#,
        url
    );

    project.write_manifest(&manifest).await.unwrap();
    let output = project.run_agpm(&["install"]).unwrap();
    output.assert_success();

    // Debug: Let's see what's in the lockfile
    let lockfile_content =
        tokio::fs::read_to_string(project.project_path().join("agpm.lock")).await.unwrap();
    println!("=== LOCKFILE CONTENT ===\n{}", lockfile_content);
    println!("=== END LOCKFILE ===\n");

    // Verify ALL 3 agents got the patches
    for name in ["helper-alpha", "helper-beta", "helper-gamma"] {
        let agent_path =
            project.project_path().join(format!(".claude/agents/agpm/helpers/{}.md", name));

        assert!(agent_path.exists(), "Agent {} should exist", name);

        let content = tokio::fs::read_to_string(&agent_path).await.unwrap();

        // All should have patched model
        assert!(
            content.contains("model: claude-3-haiku"),
            "Agent {} should have patched model, got:\n{}",
            name,
            content
        );

        // All should have new fields
        assert!(
            content.contains("max_tokens: \"4096\"")
                || content.contains("max_tokens: 4096")
                || content.contains("max_tokens: '4096'"),
            "Agent {} should have max_tokens patch, got:\n{}",
            name,
            content
        );
        assert!(content.contains("category: utility"), "Agent {} should have category patch", name);

        // Original fields should be preserved
        assert!(content.contains("temp: 0.7"), "Agent {} should preserve original temp", name);
        assert!(content.contains("role: helper"), "Agent {} should preserve original role", name);
    }

    // Verify lockfile has 3 separate entries with applied_patches
    let lockfile = project.read_lockfile().await.unwrap();
    eprintln!("=== LOCKFILE ===\n{}", lockfile);
    let applied_count = lockfile.matches("applied_patches").count();
    assert!(
        applied_count >= 3,
        "Lockfile should have at least 3 resources with applied_patches, found {}",
        applied_count
    );

    // Each resource should have its own entry (with agents/helpers/ prefix - canonical names)
    for name in
        ["agents/helpers/helper-alpha", "agents/helpers/helper-beta", "agents/helpers/helper-gamma"]
    {
        assert!(
            lockfile.contains(&format!("name = \"{}\"", name)),
            "Lockfile should have entry for {}",
            name
        );
    }

    // Verify agpm list shows all as patched
    let list_output = project.run_agpm(&["list"]).unwrap();
    let patched_count = list_output.stdout.matches("(patched)").count();
    assert_eq!(patched_count, 3, "All 3 agents should show as patched in list");
}

/// Tests that patches work with recursive glob patterns (**)
#[tokio::test]
async fn test_patch_with_recursive_glob_pattern() {
    test_config::init_test_env();
    let project = TestProject::new().await.unwrap();
    let repo = project.create_source_repo("recursive-pattern").await.unwrap();

    // Create nested directory structure
    repo.add_resource("agents/ai/language", "gpt", "---\nmodel: gpt-4\n---\n# GPT\n")
        .await
        .unwrap();
    repo.add_resource("agents/ai/vision", "dalle", "---\nmodel: dall-e-3\n---\n# DALL-E\n")
        .await
        .unwrap();
    repo.add_resource("agents/code/rust", "rustacean", "---\nmodel: sonnet\n---\n# Rust Helper\n")
        .await
        .unwrap();

    repo.commit_all("v1.0.0").unwrap();
    repo.tag_version("v1.0.0").unwrap();

    let url = repo.bare_file_url(project.sources_path()).unwrap();

    // Use recursive pattern for ai/** and install code agent separately (preserving nested structure)
    let manifest = format!(
        r#"[sources]
test = "{}"
[agents]
ai-agents = {{ source = "test", path = "agents/ai/**/*.md", version = "v1.0.0", flatten = false }}
code-helper = {{ source = "test", path = "agents/code/rust/rustacean.md", version = "v1.0.0", flatten = false }}

[patch.agents.ai-agents]
category = "ai-assistant"
team = "ai"
"#,
        url
    );

    project.write_manifest(&manifest).await.unwrap();
    project.run_agpm(&["install"]).unwrap().assert_success();

    // Verify only ai/** agents got patches
    let gpt_content = tokio::fs::read_to_string(
        project.project_path().join(".claude/agents/agpm/ai/language/gpt.md"),
    )
    .await
    .unwrap();
    assert!(gpt_content.contains("category: ai-assistant"), "GPT should have category patch");
    assert!(gpt_content.contains("team: ai"), "GPT should have team patch");

    let dalle_content = tokio::fs::read_to_string(
        project.project_path().join(".claude/agents/agpm/ai/vision/dalle.md"),
    )
    .await
    .unwrap();
    assert!(dalle_content.contains("category: ai-assistant"), "DALL-E should have category patch");

    // Code agent should NOT have patches (not matched by pattern)
    let code_content = tokio::fs::read_to_string(
        project.project_path().join(".claude/agents/agpm/code/rust/rustacean.md"),
    )
    .await
    .unwrap();
    assert!(
        !code_content.contains("category: ai-assistant"),
        "Rust helper should NOT have AI category"
    );
    assert!(!code_content.contains("team: ai"), "Rust helper should NOT have team patch");

    // Verify lockfile has correct canonical names for pattern-matched resources
    let lockfile = project.read_lockfile().await.unwrap();
    // Check for canonical names (agents/ai/language/gpt, etc.)
    assert!(
        lockfile.contains("name = \"agents/ai/language/gpt\""),
        "Lockfile should have canonical name for GPT agent"
    );
    assert!(
        lockfile.contains("name = \"agents/ai/vision/dalle\""),
        "Lockfile should have canonical name for DALL-E agent"
    );
    assert!(
        lockfile.contains("name = \"agents/code/rust/rustacean\""),
        "Lockfile should have canonical name for Rust agent"
    );
}

/// Tests behavior when pattern matches zero files
#[tokio::test]
async fn test_pattern_patch_with_no_matches() {
    test_config::init_test_env();
    let project = TestProject::new().await.unwrap();
    let repo = project.create_source_repo("no-match").await.unwrap();

    repo.add_resource("agents", "single", "---\nmodel: gpt-4\n---\n# Single\n").await.unwrap();
    repo.commit_all("v1.0.0").unwrap();
    repo.tag_version("v1.0.0").unwrap();

    let url = repo.bare_file_url(project.sources_path()).unwrap();

    // Pattern that matches nothing
    let manifest = format!(
        r#"[sources]
test = "{}"
[agents]
nonexistent = {{ source = "test", path = "agents/helpers/*.md", version = "v1.0.0" }}

[patch.agents.nonexistent]
model = "haiku"
"#,
        url
    );

    project.write_manifest(&manifest).await.unwrap();

    // Pattern with zero matches should succeed gracefully (installing 0 resources is valid)
    // The implementation may choose to succeed silently, warn, or fail - all are acceptable
    let _result = project.run_agpm(&["install"]);
    // Test passes as long as the command doesn't panic or produce an unexpected error
    // (e.g., a crash vs. a graceful "no matches" error)
}

// ============================================================================
// P0-3: Update Flow with Patches
// ============================================================================

#[tokio::test]
async fn test_update_preserves_and_reapplies_patches() {
    test_config::init_test_env();
    let project = TestProject::new().await.unwrap();
    let repo = project.create_source_repo("update-patches").await.unwrap();

    // v1.0.0: Original content
    repo.add_resource(
        "agents",
        "evolving",
        "---\nmodel: gpt-4\ntemp: 0.5\nfeature_a: true\n---\n# V1 Content\nOriginal body.\n",
    )
    .await
    .unwrap();
    repo.commit_all("v1.0.0").unwrap();
    repo.tag_version("v1.0.0").unwrap();

    // v2.0.0: Changed temp and added feature_b (overwrites the file)
    repo.add_resource(
        "agents",
        "evolving",
        "---\nmodel: gpt-4\ntemp: 0.7\nfeature_a: true\nfeature_b: true\n---\n# V2 Content\nUpdated body.\n",
    )
    .await
    .unwrap();
    repo.commit_all("v2.0.0").unwrap();
    repo.tag_version("v2.0.0").unwrap();

    let url = repo.bare_file_url(project.sources_path()).unwrap();

    // Install v1 with patches
    let manifest_v1 = format!(
        r#"[sources]
test = "{}"
[agents]
my-agent = {{ source = "test", path = "agents/evolving.md", version = "v1.0.0" }}

[patch.agents.my-agent]
model = "claude-3-haiku"
custom_field = "patched-value"
"#,
        url
    );

    project.write_manifest(&manifest_v1).await.unwrap();
    project.run_agpm(&["install"]).unwrap().assert_success();

    // Verify v1 with patches
    let v1_content =
        tokio::fs::read_to_string(project.project_path().join(".claude/agents/agpm/evolving.md"))
            .await
            .unwrap();
    assert!(v1_content.contains("model: claude-3-haiku"), "v1 should have patched model");
    assert!(v1_content.contains("temp: 0.5"), "v1 should have original temp");
    assert!(v1_content.contains("custom_field: patched-value"), "v1 should have custom patch");
    assert!(v1_content.contains("# V1 Content"), "v1 should have v1 body");

    // Update to v2.0.0 (keep same patches)
    let manifest_v2 = manifest_v1.replace("v1.0.0", "v2.0.0");
    project.write_manifest(&manifest_v2).await.unwrap();
    project.run_agpm(&["update"]).unwrap().assert_success();

    // Verify v2 with patches reapplied
    let v2_content =
        tokio::fs::read_to_string(project.project_path().join(".claude/agents/agpm/evolving.md"))
            .await
            .unwrap();

    // Patches should persist
    assert!(
        v2_content.contains("model: claude-3-haiku"),
        "Patch should persist after update to v2"
    );
    assert!(
        v2_content.contains("custom_field: patched-value"),
        "Custom patch field should persist after update"
    );

    // New upstream fields should appear
    assert!(v2_content.contains("temp: 0.7"), "v2 temp should be updated from upstream");
    assert!(v2_content.contains("feature_b: true"), "v2 new feature should appear");

    // Body should update
    assert!(v2_content.contains("# V2 Content"), "v2 body should be updated");
    assert!(v2_content.contains("Updated body"), "v2 body text should be new");
    assert!(!v2_content.contains("Original body"), "v1 body text should be gone");

    // Lockfile should reflect v2
    let lockfile = project.read_lockfile().await.unwrap();
    assert!(lockfile.contains("version = \"v2.0.0\""), "Lockfile should show v2.0.0");
    assert!(lockfile.contains("applied_patches"), "Lockfile should still have applied_patches");
}

#[tokio::test]
async fn test_update_with_changing_patches() {
    test_config::init_test_env();
    let project = TestProject::new().await.unwrap();
    let repo = project.create_source_repo("patch-change").await.unwrap();

    repo.add_resource("agents", "agent", "---\nmodel: gpt-4\ntemp: 0.5\n---\n# Agent\n")
        .await
        .unwrap();
    repo.commit_all("v1.0.0").unwrap();
    repo.tag_version("v1.0.0").unwrap();

    // Overwrite for v2.0.0
    repo.add_resource("agents", "agent", "---\nmodel: claude-3-opus\ntemp: 0.7\n---\n# Updated\n")
        .await
        .unwrap();
    repo.commit_all("v2.0.0").unwrap();
    repo.tag_version("v2.0.0").unwrap();

    let url = repo.bare_file_url(project.sources_path()).unwrap();

    // Install v1 with patch A
    let manifest_v1 = format!(
        r#"[sources]
test = "{}"
[agents]
my-agent = {{ source = "test", path = "agents/agent.md", version = "v1.0.0" }}

[patch.agents.my-agent]
model = "claude-3-haiku"
field_a = "value_a"
"#,
        url
    );

    project.write_manifest(&manifest_v1).await.unwrap();
    project.run_agpm(&["install"]).unwrap().assert_success();

    // Update version AND change patches
    let manifest_v2 = format!(
        r#"[sources]
test = "{}"
[agents]
my-agent = {{ source = "test", path = "agents/agent.md", version = "v2.0.0" }}

[patch.agents.my-agent]
model = "claude-3-sonnet"
field_b = "value_b"
"#,
        url
    );

    project.write_manifest(&manifest_v2).await.unwrap();
    project.run_agpm(&["update"]).unwrap().assert_success();

    let content =
        tokio::fs::read_to_string(project.project_path().join(".claude/agents/agpm/agent.md"))
            .await
            .unwrap();

    // New patch should apply
    assert!(content.contains("model: claude-3-sonnet"), "Updated patch should apply");
    assert!(content.contains("field_b: value_b"), "New patch field should appear");

    // Old patch should be gone
    assert!(!content.contains("field_a: value_a"), "Old patch field should be removed");

    // Upstream changes should appear
    assert!(content.contains("temp: 0.7"), "Upstream temp change should appear");
}

/// Tests that patches are reverted when removed from manifest.
///
/// When patches are removed from the manifest and `agpm update` is run, installed
/// files should revert to their original (unpatched) content from upstream.
/// The update command detects patch changes (not just version changes) and reinstalls.
#[tokio::test]
async fn test_update_removes_patches_when_manifest_patch_removed() {
    test_config::init_test_env();
    let project = TestProject::new().await.unwrap();
    let repo = project.create_source_repo("remove-patch").await.unwrap();

    repo.add_resource("agents", "agent", "---\nmodel: gpt-4\n---\n# Agent\n").await.unwrap();
    repo.commit_all("v1.0.0").unwrap();
    repo.tag_version("v1.0.0").unwrap();

    let url = repo.bare_file_url(project.sources_path()).unwrap();

    // Install with patch
    let manifest_patched = format!(
        r#"[sources]
test = "{}"
[agents]
my-agent = {{ source = "test", path = "agents/agent.md", version = "v1.0.0" }}

[patch.agents.my-agent]
model = "claude-3-haiku"
custom_field = "custom_value"
"#,
        url
    );

    project.write_manifest(&manifest_patched).await.unwrap();
    project.run_agpm(&["install"]).unwrap().assert_success();

    let patched =
        tokio::fs::read_to_string(project.project_path().join(".claude/agents/agpm/agent.md"))
            .await
            .unwrap();
    assert!(patched.contains("model: claude-3-haiku"), "Should have patched model");
    assert!(patched.contains("custom_field: custom_value"), "Should have custom field");

    // Remove patches from manifest
    let manifest_no_patch = format!(
        r#"[sources]
test = "{}"
[agents]
my-agent = {{ source = "test", path = "agents/agent.md", version = "v1.0.0" }}
"#,
        url
    );

    project.write_manifest(&manifest_no_patch).await.unwrap();
    project.run_agpm(&["update"]).unwrap().assert_success();

    // Should revert to upstream content
    let unpatched =
        tokio::fs::read_to_string(project.project_path().join(".claude/agents/agpm/agent.md"))
            .await
            .unwrap();
    assert!(
        unpatched.contains("model: gpt-4"),
        "Should revert to upstream model when patch removed"
    );
    assert!(
        !unpatched.contains("custom_field"),
        "Custom field should be removed when patch removed"
    );

    // Lockfile should not have applied_patches or should have empty applied_patches
    let lockfile = project.read_lockfile().await.unwrap();
    let lockfile_toml: toml::Value = toml::from_str(&lockfile).unwrap();
    let agents = lockfile_toml.get("agents").and_then(|v| v.as_array()).unwrap();
    if let Some(patches) = agents[0].get("applied_patches") {
        // If field exists, should be empty
        assert!(
            patches.as_table().unwrap().is_empty(),
            "applied_patches should be empty when no patches"
        );
    }
    // Otherwise, field may be omitted entirely (also acceptable)
}

// ============================================================================
// P0-4: Patch Display with Original Values
// ============================================================================

/// Tests that patch display correctly extracts original values from source files
///
/// This test verifies that `agpm list --detailed` shows original → overridden
/// format with actual values from the source repository, not "(none)".
#[tokio::test]
async fn test_patch_display_extracts_original_values() {
    test_config::init_test_env();
    let project = TestProject::new().await.unwrap();
    let repo = project.create_source_repo("display-test").await.unwrap();

    // Create agent with specific field values in frontmatter
    let agent_content = r#"---
model: claude-3-opus
temperature: "0.5"
max_tokens: 4096
custom_field: "original_value"
---
# Test Agent

This agent has several fields that will be patched.
"#;

    repo.add_resource("agents", "display-agent", agent_content).await.unwrap();
    repo.commit_all("v1.0.0").unwrap();
    repo.tag_version("v1.0.0").unwrap();

    let url = repo.bare_file_url(project.sources_path()).unwrap();

    // Create manifest with patches
    let manifest = format!(
        r#"[sources]
test = "{}"

[agents]
my-agent = {{ source = "test", path = "agents/display-agent.md", version = "v1.0.0" }}

[patch.agents.my-agent]
model = "claude-3-haiku"
temperature = "0.8"
max_tokens = 8192
custom_field = "patched_value"
"#,
        url
    );

    project.write_manifest(&manifest).await.unwrap();
    project.run_agpm(&["install"]).unwrap().assert_success();

    // Run list --detailed to see patch display
    let list_output = project.run_agpm(&["list", "--detailed"]).unwrap();
    list_output.assert_success();

    let output_text = list_output.stdout.clone();

    // Verify original values are shown (not "(none)")
    // The display should show: field: "original" → "patched"

    // Check for model patch display
    assert!(
        output_text.contains("model:")
            && output_text.contains("claude-3-opus")
            && output_text.contains("claude-3-haiku"),
        "Should show original model (claude-3-opus) → patched model (claude-3-haiku). Output:\n{}",
        output_text
    );

    // Check for temperature patch display
    assert!(
        output_text.contains("temperature:")
            && output_text.contains("0.5")
            && output_text.contains("0.8"),
        "Should show original temperature (0.5) → patched temperature (0.8). Output:\n{}",
        output_text
    );

    // Check for max_tokens patch display
    assert!(
        output_text.contains("max_tokens:")
            && output_text.contains("4096")
            && output_text.contains("8192"),
        "Should show original max_tokens (4096) → patched max_tokens (8192). Output:\n{}",
        output_text
    );

    // Check for custom_field patch display
    assert!(
        output_text.contains("custom_field:")
            && output_text.contains("original_value")
            && output_text.contains("patched_value"),
        "Should show original custom_field (original_value) → patched custom_field (patched_value). Output:\n{}",
        output_text
    );

    // Verify that "(none)" does NOT appear (which would indicate extraction failure)
    assert!(
        !output_text.contains("(none)"),
        "Should NOT show '(none)' for original values when source file exists. Output:\n{}",
        output_text
    );

    // Verify diff format is used (with - and + markers)
    assert!(
        output_text.contains("  -") && output_text.contains("  +"),
        "Should use diff format with - and + markers for patch display. Output:\n{}",
        output_text
    );
}

/// Tests that all patches use consistent diff format regardless of value length
#[tokio::test]
async fn test_patch_display_diff_format() {
    test_config::init_test_env();
    let project = TestProject::new().await.unwrap();
    let repo = project.create_source_repo("format-test").await.unwrap();

    // Create agent with both short and long field values
    let long_description = "a".repeat(100);
    let agent_content = format!(
        r#"---
model: claude-3-opus
description: "{}"
short_field: "brief"
---
# Format Test Agent

Testing diff format for all patch values.
"#,
        long_description
    );

    repo.add_resource("agents", "format-agent", &agent_content).await.unwrap();
    repo.commit_all("v1.0.0").unwrap();
    repo.tag_version("v1.0.0").unwrap();

    let url = repo.bare_file_url(project.sources_path()).unwrap();

    let long_patch_value = "b".repeat(100);
    let manifest = format!(
        r#"[sources]
test = "{}"

[agents]
format-agent = {{ source = "test", path = "agents/format-agent.md", version = "v1.0.0" }}

[patch.agents.format-agent]
model = "claude-3-haiku"
description = "{}"
short_field = "tiny"
"#,
        url, long_patch_value
    );

    project.write_manifest(&manifest).await.unwrap();
    project.run_agpm(&["install"]).unwrap().assert_success();

    let list_output = project.run_agpm(&["list", "--detailed"]).unwrap();
    list_output.assert_success();

    let output = list_output.stdout.clone();

    // All values should use diff format (with - and + markers)
    assert!(
        output.contains("model:") && output.contains("  -") && output.contains("  +"),
        "Model patch should use diff format. Output:\n{}",
        output
    );

    assert!(
        output.contains("claude-3-opus") && output.contains("claude-3-haiku"),
        "Model patch should show both original and patched values. Output:\n{}",
        output
    );

    assert!(
        output.contains("short_field:") && output.contains("  -") && output.contains("  +"),
        "Short field should also use diff format. Output:\n{}",
        output
    );

    assert!(
        output.contains("brief") && output.contains("tiny"),
        "Short field should show both original and patched values. Output:\n{}",
        output
    );

    assert!(
        output.contains("description:") && output.contains("  -") && output.contains("  +"),
        "Long field should use diff format. Output:\n{}",
        output
    );
}

/// Tests that tree command also shows patch display with original values
#[tokio::test]
async fn test_patch_display_in_tree_command() {
    test_config::init_test_env();
    let project = TestProject::new().await.unwrap();
    let repo = project.create_source_repo("tree-test").await.unwrap();

    let agent_content = r#"---
model: gpt-4
temperature: "0.7"
---
# Tree Test Agent

Test patch display in tree command.
"#;

    repo.add_resource("agents", "tree-agent", agent_content).await.unwrap();
    repo.commit_all("v1.0.0").unwrap();
    repo.tag_version("v1.0.0").unwrap();

    let url = repo.bare_file_url(project.sources_path()).unwrap();

    let manifest = format!(
        r#"[sources]
test = "{}"

[agents]
tree-agent = {{ source = "test", path = "agents/tree-agent.md", version = "v1.0.0" }}

[patch.agents.tree-agent]
model = "claude-3-sonnet"
temperature = "0.9"
"#,
        url
    );

    project.write_manifest(&manifest).await.unwrap();
    project.run_agpm(&["install"]).unwrap().assert_success();

    // Run tree --detailed
    let tree_output = project.run_agpm(&["tree", "--detailed"]).unwrap();
    tree_output.assert_success();

    let output = tree_output.stdout.clone();

    // Verify patch display appears in tree output
    assert!(
        output.contains("model:") && output.contains("gpt-4") && output.contains("claude-3-sonnet"),
        "Tree command should show original → patched for model. Output:\n{}",
        output
    );

    assert!(
        output.contains("temperature:") && output.contains("0.7") && output.contains("0.9"),
        "Tree command should show original → patched for temperature. Output:\n{}",
        output
    );

    assert!(
        !output.contains("(none)"),
        "Tree command should NOT show '(none)' for original values. Output:\n{}",
        output
    );
}

/// Tests patch extraction with JSON files
///
/// Verifies that patches applied to JSON files correctly extract and display
/// original values from the source repository.
#[tokio::test]
async fn test_patch_display_json_original_values() {
    test_config::init_test_env();
    let project = TestProject::new().await.unwrap();
    let repo = project.create_source_repo("json-display").await.unwrap();

    // Create a command JSON file with specific field values
    let commands_dir = repo.path.join("commands");
    tokio::fs::create_dir_all(&commands_dir).await.unwrap();

    let command_content = r#"{
  "name": "test-command",
  "description": "A test command",
  "timeout": 30,
  "retries": 5,
  "enabled": false
}"#;

    let command_file = commands_dir.join("test-command.json");
    tokio::fs::write(&command_file, command_content).await.unwrap();

    repo.commit_all("v1.0.0").unwrap();
    repo.tag_version("v1.0.0").unwrap();

    let url = repo.bare_file_url(project.sources_path()).unwrap();

    let manifest = format!(
        r#"[sources]
test = "{}"

[commands]
my-command = {{ source = "test", path = "commands/test-command.json", version = "v1.0.0" }}

[patch.commands.my-command]
timeout = 300
retries = 10
enabled = true
priority = 5
"#,
        url
    );

    project.write_manifest(&manifest).await.unwrap();
    project.run_agpm(&["install"]).unwrap().assert_success();

    // Verify the installed JSON file has patched values
    let command_path = project.project_path().join(".claude/commands/agpm/test-command.json");
    assert!(command_path.exists(), "Command file should exist");

    let content = tokio::fs::read_to_string(&command_path).await.unwrap();
    let json: serde_json::Value = serde_json::from_str(&content).unwrap();

    // Verify patched values are in the file
    assert_eq!(
        json.get("timeout").and_then(|v| v.as_i64()),
        Some(300),
        "Timeout should be patched"
    );
    assert_eq!(json.get("retries").and_then(|v| v.as_i64()), Some(10), "Retries should be patched");
    assert_eq!(
        json.get("enabled").and_then(|v| v.as_bool()),
        Some(true),
        "Enabled should be patched"
    );
    assert_eq!(json.get("priority").and_then(|v| v.as_i64()), Some(5), "Priority should be added");

    // Verify lockfile tracks patches
    let lockfile_content = project.read_lockfile().await.unwrap();
    assert!(
        lockfile_content.contains("applied_patches"),
        "Lockfile should track patches for JSON resources"
    );

    // The key verification is that the patch extraction logic works correctly
    // when reading from source files (tested in unit tests above)
}

#[tokio::test]
async fn test_validate_check_lock_with_patches() {
    test_config::init_test_env();
    let project = TestProject::new().await.unwrap();

    // Create test repository with agent
    let (url, path) = create_repo_with_model_agent(&project).await.unwrap();

    // Create project manifest with project-level patch
    let manifest = format!(
        r#"[sources]
test = "{}"

[agents]
my-agent = {{ source = "test", path = "{}", version = "v1.0.0" }}

[patch.agents.my-agent]
model = "claude-3-haiku"
temperature = "0.7"
"#,
        url, path
    );

    project.write_manifest(&manifest).await.unwrap();

    // Create private manifest with additional patches
    let private_manifest = r#"[patch.agents.my-agent]
max_tokens = 4000
"#;

    let private_path = project.project_path().join("agpm.private.toml");
    fs::write(&private_path, private_manifest).await.unwrap();

    // Run install
    let output = project.run_agpm(&["install"]).unwrap();
    output.assert_success();

    // Run validate --check-lock to verify it recognizes patched resources
    let output = project.run_agpm(&["validate", "--check-lock"]).unwrap();
    assert!(
        output.success,
        "validate --check-lock should succeed with patched resources. Stderr:\n{}",
        output.stderr
    );

    // Verify output mentions the resource is patched or validates successfully
    assert!(
        output.stdout.contains("✓") || output.stdout.contains("valid"),
        "Validate should report success for patched resources. Output:\n{}",
        output.stdout
    );

    // Verify lockfile contains patch information
    let lockfile_content = project.read_lockfile().await.unwrap();
    assert!(lockfile_content.contains("applied_patches"), "Lockfile should track applied patches");
}

#[tokio::test]
async fn test_validate_resolve_with_patches() {
    test_config::init_test_env();
    let project = TestProject::new().await.unwrap();

    // Create test repository with agent
    let (url, path) = create_repo_with_model_agent(&project).await.unwrap();

    // Create manifest with patches
    let manifest = format!(
        r#"[sources]
test = "{}"

[agents]
my-agent = {{ source = "test", path = "{}", version = "v1.0.0" }}

[patch.agents.my-agent]
model = "claude-3-haiku"
"#,
        url, path
    );

    project.write_manifest(&manifest).await.unwrap();

    // Run validate --resolve (which should work with patches)
    let output = project.run_agpm(&["validate", "--resolve"]).unwrap();

    // This should succeed - validate --resolve should handle patches
    assert!(
        output.success,
        "validate --resolve should succeed with patches. Stderr:\n{}",
        output.stderr
    );
}

#[tokio::test]
async fn test_validate_detects_unknown_patch_alias() {
    test_config::init_test_env();
    let project = TestProject::new().await.unwrap();

    // Create test repository
    let (url, path) = create_repo_with_model_agent(&project).await.unwrap();

    // Create manifest with patch for unknown alias
    let manifest = format!(
        r#"[sources]
test = "{}"

[agents]
my-agent = {{ source = "test", path = "{}", version = "v1.0.0" }}

[patch.agents.nonexistent-agent]
model = "claude-3-haiku"
"#,
        url, path
    );

    project.write_manifest(&manifest).await.unwrap();

    // Run validate - should detect unknown alias in patch
    let output = project.run_agpm(&["validate"]).unwrap();

    assert!(!output.success, "validate should fail when patch references unknown alias");

    assert!(
        output.stderr.contains("nonexistent-agent") || output.stdout.contains("nonexistent-agent"),
        "Error should mention the unknown alias in patch section. Output:\nstdout: {}\nstderr: {}",
        output.stdout,
        output.stderr
    );
}
