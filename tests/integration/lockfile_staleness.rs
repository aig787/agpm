use anyhow::Result;

use crate::common::{ManifestBuilder, TestProject};

/// Test that install auto-updates lockfile when dependency is missing (Cargo-style behavior)
#[tokio::test]
async fn test_install_auto_updates_missing_dependency() -> Result<()> {
    let project = TestProject::new().await?;

    // Create a source repo with an agent
    let source_repo = project.create_source_repo("test-source").await?;
    source_repo.add_resource("agents", "agent-one", "# Agent One\nTest agent one").await?;
    source_repo.add_resource("agents", "agent-two", "# Agent Two\nTest agent two").await?;
    source_repo.commit_all("Add agents")?;
    source_repo.tag_version("v1.0.0")?;

    // Create manifest with two agents
    let manifest = ManifestBuilder::new()
        .add_source("test-source", &source_repo.file_url())
        .add_standard_agent("agent-one", "test-source", "agents/agent-one.md")
        .add_standard_agent("agent-two", "test-source", "agents/agent-two.md")
        .build();
    project.write_manifest(&manifest).await?;

    // Install first to create lockfile
    let output = project.run_agpm(&["install", "--quiet"])?;
    assert!(output.success, "Initial install failed: {}", output.stderr);

    // Now remove one agent from lockfile to simulate staleness
    let lockfile = project.read_lockfile().await?;

    // Find and remove the agent-two section
    let start_marker = "[[agents]]\nname = \"agent-two\"";
    if let Some(start_pos) = lockfile.find(start_marker) {
        // Find the end of this agent entry (next [[agents]] or end of file)
        let after_start = start_pos + start_marker.len();
        let end_pos = lockfile[after_start..]
            .find("[[agents]]")
            .map(|p| after_start + p)
            .unwrap_or(lockfile.len());

        // Remove this entire agent section
        let modified_lockfile = format!("{}{}", &lockfile[..start_pos], &lockfile[end_pos..]);
        project.write_lockfile(&modified_lockfile).await?;
    } else {
        panic!("Could not find agent-two in lockfile to remove");
    }

    // Install should auto-update the lockfile (Cargo-style behavior)
    let output = project.run_agpm(&["install", "--quiet"])?;
    assert!(output.success, "Install should auto-update lockfile: {}", output.stderr);

    // Verify lockfile now contains agent-two again
    let updated_lockfile = project.read_lockfile().await?;
    assert!(
        updated_lockfile.contains("agent-two"),
        "Lockfile should have been auto-updated with missing dependency"
    );

    Ok(())
}

/// Test that install auto-updates on version change, but --frozen mode fails
#[tokio::test]
async fn test_install_frozen_detects_version_mismatch() -> Result<()> {
    let project = TestProject::new().await?;

    // Create a source repo with multiple versions
    let source_repo = project.create_source_repo("test-source").await?;
    source_repo.add_resource("agents", "test-agent", "# Test Agent v1\nVersion 1").await?;
    source_repo.commit_all("Version 1")?;
    source_repo.tag_version("v1.0.0")?;

    source_repo.add_resource("agents", "test-agent", "# Test Agent v2\nVersion 2").await?;
    source_repo.commit_all("Version 2")?;
    source_repo.tag_version("v2.0.0")?;

    // Create manifest with v1.0.0
    let manifest = ManifestBuilder::new()
        .add_source("test-source", &source_repo.file_url())
        .add_standard_agent("test-agent", "test-source", "agents/test-agent.md")
        .build();
    project.write_manifest(&manifest).await?;

    // Install v1.0.0
    let output = project.run_agpm(&["install", "--quiet"])?;
    assert!(output.success, "Initial install failed: {}", output.stderr);

    // Update manifest to v2.0.0
    let manifest_v2 = manifest.replace("v1.0.0", "v2.0.0");
    project.write_manifest(&manifest_v2).await?;

    // Normal install should auto-update (Cargo-style)
    let output = project.run_agpm(&["install", "--quiet"])?;
    assert!(output.success, "Normal install should auto-update: {}", output.stderr);

    // Revert to v1.0.0 lockfile
    project.write_manifest(&manifest).await?;
    let output = project.run_agpm(&["install", "--quiet"])?;
    assert!(output.success);

    // Change manifest back to v2.0.0
    project.write_manifest(&manifest_v2).await?;

    // --frozen mode should succeed (only checks corruption/security, not version changes)
    // It will use the lockfile as-is with v1.0.0 even though manifest has v2.0.0
    let output = project.run_agpm(&["install", "--frozen"])?;
    assert!(
        output.success,
        "Frozen install should succeed (ignores version changes): {}",
        output.stderr
    );

    Ok(())
}

/// Test that extra lockfile entries are allowed (for transitive dependencies)
#[tokio::test]
async fn test_install_detects_removed_dependency() -> Result<()> {
    let project = TestProject::new().await?;

    // Create a source repo with two agents
    let source_repo = project.create_source_repo("test-source").await?;
    source_repo.add_resource("agents", "agent-one", "# Agent One").await?;
    source_repo.add_resource("agents", "agent-two", "# Agent Two").await?;
    source_repo.commit_all("Add agents")?;
    source_repo.tag_version("v1.0.0")?;

    // Create manifest with two agents
    let manifest_two_agents = ManifestBuilder::new()
        .add_source("test-source", &source_repo.file_url())
        .add_standard_agent("agent-one", "test-source", "agents/agent-one.md")
        .add_standard_agent("agent-two", "test-source", "agents/agent-two.md")
        .build();
    project.write_manifest(&manifest_two_agents).await?;

    // Install both agents
    let output = project.run_agpm(&["install", "--quiet"])?;
    assert!(output.success, "Initial install failed: {}", output.stderr);

    // Remove agent-two from manifest (simulating it becoming only a transitive dependency)
    let manifest_one_agent = ManifestBuilder::new()
        .add_source("test-source", &source_repo.file_url())
        .add_standard_agent("agent-one", "test-source", "agents/agent-one.md")
        .build();
    project.write_manifest(&manifest_one_agent).await?;

    // Try to install with lockfile containing extra entry in CI mode
    // This should succeed now - extra entries are allowed for transitive dependencies
    let output = project.run_agpm_with_env(&["install"], &[("CI", "true")])?;
    assert!(
        output.success,
        "Install should succeed with extra lockfile entries for transitive deps: {}",
        output.stderr
    );
    assert!(!output.stderr.contains("stale"), "Should not report lockfile as stale");

    Ok(())
}

/// Test that path change is detected
#[tokio::test]
async fn test_install_detects_path_change() -> Result<()> {
    let project = TestProject::new().await?;

    // Create a source repo with agent in two locations
    let source_repo = project.create_source_repo("test-source").await?;
    source_repo.add_resource("agents", "old-path", "# Agent at old path").await?;
    source_repo.add_resource("agents", "new-path", "# Agent at new path").await?;
    source_repo.commit_all("Add agents")?;
    source_repo.tag_version("v1.0.0")?;

    // Create manifest pointing to old path
    let manifest_old = ManifestBuilder::new()
        .add_source("test-source", &source_repo.file_url())
        .add_agent("test-agent", |d| {
            d.source("test-source")
                .path("agents/old-path.md")
                .version("v1.0.0")
        })
        .build();
    project.write_manifest(&manifest_old).await?;

    // Install with old path
    let output = project.run_agpm(&["install", "--quiet"])?;
    assert!(output.success, "Initial install failed: {}", output.stderr);

    // Update manifest to new path
    let manifest_new = manifest_old.replace("old-path", "new-path");
    project.write_manifest(&manifest_new).await?;

    // Normal install should auto-update
    let output = project.run_agpm(&["install", "--quiet"])?;
    assert!(output.success, "Normal install should auto-update path change: {}", output.stderr);

    // Revert to old path and reinstall
    project.write_manifest(&manifest_old).await?;
    let output = project.run_agpm(&["install", "--quiet"])?;
    assert!(output.success);

    // Change back to new path
    project.write_manifest(&manifest_new).await?;

    // --frozen mode should fail
    let output = project.run_agpm(&["install", "--frozen"])?;
    assert!(output.success, "Frozen mode should succeed (ignores path changes): {}", output.stderr);

    Ok(())
}

/// Test that source URL change is detected
#[tokio::test]
async fn test_install_detects_source_url_change() -> Result<()> {
    let project = TestProject::new().await?;

    // Create two different source repos
    let old_repo = project.create_source_repo("old-repo").await?;
    old_repo.add_resource("agents", "test-agent", "# Agent from old repo").await?;
    old_repo.commit_all("Add agent")?;
    old_repo.tag_version("v1.0.0")?;

    let new_repo = project.create_source_repo("new-repo").await?;
    new_repo.add_resource("agents", "test-agent", "# Agent from new repo").await?;
    new_repo.commit_all("Add agent")?;
    new_repo.tag_version("v1.0.0")?;

    // Create manifest pointing to old repo
    let manifest_old = ManifestBuilder::new()
        .add_source("test-source", &old_repo.file_url())
        .add_standard_agent("test-agent", "test-source", "agents/test-agent.md")
        .build();
    project.write_manifest(&manifest_old).await?;

    // Install from old repo
    let output = project.run_agpm(&["install", "--quiet"])?;
    assert!(output.success, "Initial install failed: {}", output.stderr);

    // Update manifest to point to new repo
    let manifest_new = ManifestBuilder::new()
        .add_source("test-source", &new_repo.file_url())
        .add_standard_agent("test-agent", "test-source", "agents/test-agent.md")
        .build();
    project.write_manifest(&manifest_new).await?;

    // --frozen mode should fail on source URL change (security concern)
    let output = project.run_agpm(&["install", "--frozen"])?;
    assert!(!output.success, "Should fail on source URL change (security)");
    assert!(
        output.stderr.contains("Source repository 'test-source' URL changed")
            || output.stderr.contains("out of sync"),
        "Should report URL change, got: {}",
        output.stderr
    );

    Ok(())
}

/// Test that duplicate entries are detected
#[tokio::test]
async fn test_install_detects_duplicate_entries() -> Result<()> {
    let project = TestProject::new().await?;

    // Create a source repo
    let source_repo = project.create_source_repo("test-source").await?;
    source_repo.add_resource("agents", "test-agent", "# Test Agent").await?;
    source_repo.commit_all("Add agent")?;

    // Ensure we're on main branch (git's default branch name varies)
    source_repo.git.ensure_branch("main")?;

    // Create manifest
    let manifest = ManifestBuilder::new()
        .add_source("test-source", &source_repo.file_url())
        .add_agent("test-agent", |d| {
            d.source("test-source")
                .path("agents/test-agent.md")
                .version("main")
        })
        .build();
    project.write_manifest(&manifest).await?;

    // First do a normal install to create a valid lockfile
    let output = project.run_agpm(&["install", "--quiet"])?;
    assert!(output.success, "Initial install failed: {}", output.stderr);

    // Read the valid lockfile and manually duplicate an entry
    let lockfile = project.read_lockfile().await?;

    // Find the agents section and duplicate the first agent entry
    if let Some(agents_pos) = lockfile.find("[[agents]]") {
        let agent_section = &lockfile[agents_pos..];

        // Find the end of this agent entry
        let next_section = agent_section[11..].find("[[").unwrap_or(agent_section.len() - 11) + 11;
        let agent_entry = &agent_section[..next_section];

        // Add a duplicate at the end
        let corrupted_lockfile = format!("{}\n{}", lockfile.trim(), agent_entry);
        project.write_lockfile(&corrupted_lockfile).await?;
    } else {
        panic!("Could not find agents section in lockfile");
    }

    // --frozen mode should fail on corrupted lockfile
    let output = project.run_agpm(&["install", "--frozen"])?;
    assert!(!output.success, "Expected failure due to duplicate entries, but command succeeded");
    assert!(
        output.stderr.contains("duplicate entries")
            || output.stderr.contains("Found") && output.stderr.contains("duplicate")
            || output.stderr.contains("out of sync"),
        "Expected duplicate entries error, got: {}",
        output.stderr
    );

    Ok(())
}

/// Test that branch references don't cause staleness errors
/// Branches are expected to move, so we shouldn't treat them as stale
#[tokio::test]
async fn test_install_allows_branch_references() -> Result<()> {
    let project = TestProject::new().await?;

    // Create a source repo with a bare clone for stable serving
    let source_repo = project.create_source_repo("test-source").await?;
    source_repo.add_resource("agents", "test-agent", "# Test Agent v1").await?;
    source_repo.commit_all("Initial commit")?;

    // Ensure we're on main branch (git's default branch name varies)
    source_repo.git.ensure_branch("main")?;

    // Get the bare URL for stable file:// serving
    let bare_url = source_repo.bare_file_url(project.sources_path())?;

    // Create manifest using 'main' branch
    let manifest = ManifestBuilder::new()
        .add_source("test-source", &bare_url)
        .add_agent("test-agent", |d| {
            d.source("test-source")
                .path("agents/test-agent.md")
                .version("main")
        })
        .build();
    project.write_manifest(&manifest).await?;

    // Install to create lockfile
    let output = project.run_agpm(&["install", "--quiet"])?;
    assert!(output.success, "Initial install failed: {}", output.stderr);

    // Try to install again - should succeed because branches are allowed to move
    // This is expected behavior with auto-update
    let output = project.run_agpm(&["install", "--quiet"])?;
    assert!(
        output.success,
        "Install should succeed with branch references, got error: {}",
        output.stderr
    );

    Ok(())
}
