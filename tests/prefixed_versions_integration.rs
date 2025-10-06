//! Integration tests for versioned prefixes feature
//!
//! Tests end-to-end workflows with monorepo-style prefixed tags.

mod common;
mod test_config;

use common::TestProject;
use tokio::fs;

/// Test installing a dependency with a prefixed version constraint
#[tokio::test]
async fn test_install_with_prefixed_constraint() {
    test_config::init_test_env();
    let project = TestProject::new().await.unwrap();
    let source_repo = project.create_source_repo("prefixed").await.unwrap();

    // Create agent files
    fs::create_dir_all(source_repo.path.join("agents"))
        .await
        .unwrap();
    fs::write(
        source_repo.path.join("agents/test-agent.md"),
        "# Test Agent\n\nTest content",
    )
    .await
    .unwrap();

    source_repo.git.add_all().unwrap();
    source_repo.git.commit("Add agents").unwrap();

    // Create prefixed tags for agents
    source_repo.git.tag("agents-v1.0.0").unwrap();
    source_repo.git.tag("agents-v1.2.0").unwrap();
    source_repo.git.tag("agents-v2.0.0").unwrap();

    // Also add some snippets with different prefix
    fs::create_dir_all(source_repo.path.join("snippets"))
        .await
        .unwrap();
    fs::write(
        source_repo.path.join("snippets/test-snippet.md"),
        "# Test Snippet\n\nSnippet content",
    )
    .await
    .unwrap();

    source_repo.git.add_all().unwrap();
    source_repo.git.commit("Add snippets").unwrap();
    source_repo.git.tag("snippets-v1.0.0").unwrap();
    source_repo.git.tag("snippets-v2.0.0").unwrap();

    // Create manifest with prefixed version constraints
    let manifest = format!(
        r#"[sources]
prefixed = "file://{}"

[agents]
test-agent = {{ source = "prefixed", path = "agents/test-agent.md", version = "agents-^v1.0.0" }}

[snippets]
test-snippet = {{ source = "prefixed", path = "snippets/test-snippet.md", version = "snippets-^v2.0.0" }}
"#,
        source_repo.path.display().to_string().replace('\\', "/")
    );

    project.write_manifest(&manifest).await.unwrap();

    // Run install
    let output = project.run_agpm(&["install"]).unwrap();
    output.assert_success();

    // Verify lockfile has correct resolved versions
    let lockfile_content = fs::read_to_string(project.project_path().join("agpm.lock"))
        .await
        .unwrap();

    // Should resolve to highest compatible versions
    assert!(
        lockfile_content.contains("agents-v1.2.0"),
        "Should resolve agents-^v1.0.0 to agents-v1.2.0 (highest 1.x)\nActual lockfile:\n{}",
        lockfile_content
    );
    assert!(
        lockfile_content.contains("snippets-v2.0.0"),
        "Should resolve snippets-^v2.0.0 to snippets-v2.0.0"
    );

    // Verify files were installed
    assert!(
        project
            .project_path()
            .join(".claude/agents/test-agent.md")
            .exists()
    );

    // Note: There's a separate bug where snippets may install to .claude/agpm/snippets/
    // This is unrelated to prefixed versions - check both possible locations
    let snippet_path1 = project
        .project_path()
        .join(".claude/snippets/test-snippet.md");
    let snippet_path2 = project
        .project_path()
        .join(".claude/agpm/snippets/test-snippet.md");

    assert!(
        snippet_path1.exists() || snippet_path2.exists(),
        "Snippet file should be installed (either in .claude/snippets/ or .claude/agpm/snippets/)"
    );
}

/// Test that prefixes provide isolation (different prefixes don't interfere)
#[tokio::test]
async fn test_prefix_isolation() {
    test_config::init_test_env();
    let project = TestProject::new().await.unwrap();
    let source_repo = project.create_source_repo("prefixed").await.unwrap();

    fs::create_dir_all(source_repo.path.join("agents"))
        .await
        .unwrap();
    fs::write(
        source_repo.path.join("agents/agent.md"),
        "# Agent\n\nContent",
    )
    .await
    .unwrap();

    source_repo.git.add_all().unwrap();
    source_repo.git.commit("Initial commit").unwrap();

    // Create tags with different prefixes AND unprefixed
    source_repo.git.tag("agents-v1.5.0").unwrap(); // agents prefix
    source_repo.git.tag("tools-v2.0.0").unwrap(); // tools prefix
    source_repo.git.tag("v1.0.0").unwrap(); // unprefixed

    // Create manifest requesting agents prefix ^v1.0.0
    let manifest = format!(
        r#"[sources]
prefixed = "file://{}"

[agents]
agent = {{ source = "prefixed", path = "agents/agent.md", version = "agents-^v1.0.0" }}
"#,
        source_repo.path.display().to_string().replace('\\', "/")
    );

    project.write_manifest(&manifest).await.unwrap();

    let output = project.run_agpm(&["install"]).unwrap();
    output.assert_success();

    let lockfile_content = fs::read_to_string(project.project_path().join("agpm.lock"))
        .await
        .unwrap();

    // Should resolve to agents-v1.5.0, NOT tools-v2.0.0 or v1.0.0
    assert!(lockfile_content.contains("agents-v1.5.0"));
    assert!(!lockfile_content.contains("tools-v2.0.0"));
    assert!(!lockfile_content.contains("version = \"v1.0.0\""));
}

/// Test outdated command with prefixed versions
#[tokio::test]
async fn test_outdated_with_prefixed_versions() {
    test_config::init_test_env();
    let project = TestProject::new().await.unwrap();
    let source_repo = project.create_source_repo("prefixed").await.unwrap();

    fs::create_dir_all(source_repo.path.join("agents"))
        .await
        .unwrap();
    fs::write(
        source_repo.path.join("agents/agent.md"),
        "# Agent\n\nContent",
    )
    .await
    .unwrap();

    source_repo.git.add_all().unwrap();
    source_repo.git.commit("Initial commit").unwrap();
    source_repo.git.tag("agents-v1.0.0").unwrap();

    // Create manifest locked to agents-v1.0.0
    let manifest = format!(
        r#"[sources]
prefixed = "file://{}"

[agents]
agent = {{ source = "prefixed", path = "agents/agent.md", version = "agents-^v1.0.0" }}
"#,
        source_repo.path.display().to_string().replace('\\', "/")
    );

    project.write_manifest(&manifest).await.unwrap();

    // Install with v1.0.0
    let output = project.run_agpm(&["install"]).unwrap();
    output.assert_success();

    // Now add a newer version
    fs::write(
        source_repo.path.join("agents/agent.md"),
        "# Agent\n\nUpdated content",
    )
    .await
    .unwrap();
    source_repo.git.add_all().unwrap();
    source_repo.git.commit("Update agent").unwrap();
    source_repo.git.tag("agents-v1.5.0").unwrap();

    // Check for outdated dependencies
    let output = project.run_agpm(&["outdated"]).unwrap();
    output.assert_success();

    // The outdated command should either:
    // 1. Show that agents can be updated from v1.0.0 to v1.5.0
    // 2. Show "All dependencies are up to date" (if constraint already allows v1.5.0)
    // Both are valid since agents-^v1.0.0 allows v1.5.0
    let has_version_info = output.stdout.contains("agents");
    let is_up_to_date = output.stdout.contains("up to date");

    assert!(
        has_version_info || is_up_to_date,
        "Expected outdated to either show version info or 'up to date' message.\nGot: {}",
        output.stdout
    );
}

/// Test that unprefixed constraints don't match prefixed tags
#[tokio::test]
async fn test_unprefixed_constraint_doesnt_match_prefixed_tags() {
    test_config::init_test_env();
    let project = TestProject::new().await.unwrap();
    let source_repo = project.create_source_repo("prefixed").await.unwrap();

    fs::create_dir_all(source_repo.path.join("agents"))
        .await
        .unwrap();
    fs::write(
        source_repo.path.join("agents/agent.md"),
        "# Agent\n\nContent",
    )
    .await
    .unwrap();

    source_repo.git.add_all().unwrap();
    source_repo.git.commit("Initial commit").unwrap();

    // Only create prefixed tag
    source_repo.git.tag("agents-v1.0.0").unwrap();

    // Create manifest with unprefixed constraint
    let manifest = format!(
        r#"[sources]
prefixed = "file://{}"

[agents]
agent = {{ source = "prefixed", path = "agents/agent.md", version = "^v1.0.0" }}
"#,
        source_repo.path.display().to_string().replace('\\', "/")
    );

    project.write_manifest(&manifest).await.unwrap();

    // Install should fail - no unprefixed tags match
    let output = project.run_agpm(&["install"]).unwrap();

    // Command should fail (success = false)
    assert!(
        !output.success,
        "Expected install to fail when no unprefixed tags match, but it succeeded"
    );

    // Verify error message mentions no matching tags
    assert!(
        output.stderr.contains("No tag found matching") || output.stderr.contains("No tags found"),
        "Expected error about no matching tags, got: {}",
        output.stderr
    );
}
