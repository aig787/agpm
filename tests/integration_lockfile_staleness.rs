use anyhow::Result;

mod common;
use common::TestProject;

mod fixtures;
use fixtures::ManifestFixture;

/// Test that stale lockfile is detected when dependency is missing
#[test]
fn test_install_detects_missing_dependency() -> Result<()> {
    let project = TestProject::new()?;

    // Create a source repo with an agent
    let source_repo = project.create_source_repo("test-source")?;
    source_repo.add_resource("agents", "agent-one", "# Agent One\nTest agent one")?;
    source_repo.add_resource("agents", "agent-two", "# Agent Two\nTest agent two")?;
    source_repo.commit_all("Add agents")?;
    source_repo.tag_version("v1.0.0")?;

    // Create manifest with two agents
    let manifest = format!(
        r#"[sources]
test-source = "{}"

[agents]
agent-one = {{ source = "test-source", path = "agents/agent-one.md", version = "v1.0.0" }}
agent-two = {{ source = "test-source", path = "agents/agent-two.md", version = "v1.0.0" }}
"#,
        source_repo.file_url()
    );
    project.write_manifest(&manifest)?;

    // Install first to create lockfile
    let output = project.run_ccpm(&["install", "--quiet"])?;
    assert!(output.success, "Initial install failed: {}", output.stderr);

    // Now remove one agent from lockfile to make it stale
    let lockfile = project.read_lockfile()?;

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
        project.write_lockfile(&modified_lockfile)?;
    } else {
        panic!("Could not find agent-two in lockfile to remove");
    }

    // Try to install with stale lockfile in CI mode
    let output = project.run_ccpm_with_env(&["install"], &[("CI", "true")])?;
    assert!(!output.success);
    assert!(output.stderr.contains("Lockfile is stale"));
    assert!(output.stderr.contains("missing from lockfile"));

    Ok(())
}

/// Test that --force flag bypasses staleness check
#[test]
fn test_install_with_force_flag() -> Result<()> {
    let project = TestProject::new()?;

    // Create a source repo
    let source_repo = project.create_source_repo("test-source")?;
    source_repo.add_resource("agents", "test-agent", "# Test Agent\nContent")?;
    source_repo.commit_all("Add agent")?;
    source_repo.tag_version("v1.0.0")?;

    // Create manifest
    let manifest = format!(
        r#"[sources]
test-source = "{}"

[agents]
test-agent = {{ source = "test-source", path = "agents/test-agent.md", version = "v1.0.0" }}
"#,
        source_repo.file_url()
    );
    project.write_manifest(&manifest)?;

    // First do a normal install to create a valid lockfile
    let output = project.run_ccpm(&["install", "--quiet"])?;
    assert!(output.success, "Initial install failed: {}", output.stderr);

    // Add a v2.0.0 version to the repo
    source_repo.add_resource("agents", "test-agent", "# Test Agent v2\nContent v2")?;
    source_repo.commit_all("Update to v2")?;
    source_repo.tag_version("v2.0.0")?;

    // Now update the manifest to use v2.0.0 to create staleness
    let updated_manifest = manifest.replace("v1.0.0", "v2.0.0");
    project.write_manifest(&updated_manifest)?;

    // Install with --force should succeed despite stale lockfile
    let output = project.run_ccpm_with_env(&["install", "--force"], &[("CI", "true")])?;
    assert!(
        output.success,
        "Install with --force failed: {}",
        output.stderr
    );

    Ok(())
}

/// Test that --regenerate flag deletes and recreates lockfile
#[test]
fn test_install_with_regenerate_flag() -> Result<()> {
    let project = TestProject::new()?;

    // Create a source repo
    let source_repo = project.create_source_repo("test-source")?;
    source_repo.add_resource("agents", "new-agent", "# New Agent\nContent")?;
    source_repo.commit_all("Add new agent")?;
    source_repo.tag_version("v2.0.0")?;

    // Create manifest pointing to new agent
    let manifest = format!(
        r#"[sources]
test-source = "{}"

[agents]
new-agent = {{ source = "test-source", path = "agents/new-agent.md", version = "v2.0.0" }}
"#,
        source_repo.file_url()
    );
    project.write_manifest(&manifest)?;

    // Create lockfile with old data
    let old_lockfile = format!(
        r#"version = 1

[[sources]]
name = "test-source"
url = "{}"
commit = "oldcommit"

[[agents]]
name = "old-agent"
source = "test-source"
path = "agents/old.md"
version = "v1.0.0"
resolved_commit = "oldcommit"
checksum = "sha256:old"
installed_at = ".claude/agents/old-agent.md"
"#,
        source_repo.file_url()
    );
    project.write_lockfile(&old_lockfile)?;

    // Install with --regenerate
    let output = project.run_ccpm(&["install", "--regenerate"])?;
    assert!(
        output.success,
        "Install with --regenerate failed: {}",
        output.stderr
    );
    assert!(
        output
            .stdout
            .contains("Removing existing lockfile for regeneration")
    );

    // Verify the lockfile was regenerated (old-agent should be gone, new-agent present)
    let lockfile_content = project.read_lockfile()?;
    assert!(!lockfile_content.contains("old-agent"));
    assert!(lockfile_content.contains("new-agent"));

    Ok(())
}

/// Test that version mismatch is detected
#[test]
fn test_install_detects_version_mismatch() -> Result<()> {
    let project = TestProject::new()?;

    // Create a source repo with multiple versions
    let source_repo = project.create_source_repo("test-source")?;
    source_repo.add_resource("agents", "test-agent", "# Test Agent v1\nVersion 1")?;
    source_repo.commit_all("Version 1")?;
    source_repo.tag_version("v1.0.0")?;

    source_repo.add_resource("agents", "test-agent", "# Test Agent v2\nVersion 2")?;
    source_repo.commit_all("Version 2")?;
    source_repo.tag_version("v2.0.0")?;

    // Create manifest with v1.0.0
    let manifest = format!(
        r#"[sources]
test-source = "{}"

[agents]
test-agent = {{ source = "test-source", path = "agents/test-agent.md", version = "v1.0.0" }}
"#,
        source_repo.file_url()
    );
    project.write_manifest(&manifest)?;

    // Install v1.0.0
    let output = project.run_ccpm(&["install", "--quiet"])?;
    assert!(output.success, "Initial install failed: {}", output.stderr);

    // Update manifest to v2.0.0
    let manifest_v2 = manifest.replace("v1.0.0", "v2.0.0");
    project.write_manifest(&manifest_v2)?;

    // Try to install with outdated lockfile in CI mode
    let output = project.run_ccpm_with_env(&["install"], &[("CI", "true")])?;
    assert!(!output.success);
    assert!(
        output
            .stderr
            .contains("version changed from 'v1.0.0' to 'v2.0.0'")
    );

    Ok(())
}

/// Test that extra lockfile entries are allowed (for transitive dependencies)
#[test]
fn test_install_detects_removed_dependency() -> Result<()> {
    let project = TestProject::new()?;

    // Create a source repo with two agents
    let source_repo = project.create_source_repo("test-source")?;
    source_repo.add_resource("agents", "agent-one", "# Agent One")?;
    source_repo.add_resource("agents", "agent-two", "# Agent Two")?;
    source_repo.commit_all("Add agents")?;
    source_repo.tag_version("v1.0.0")?;

    // Create manifest with two agents
    let manifest_two_agents = format!(
        r#"[sources]
test-source = "{}"

[agents]
agent-one = {{ source = "test-source", path = "agents/agent-one.md", version = "v1.0.0" }}
agent-two = {{ source = "test-source", path = "agents/agent-two.md", version = "v1.0.0" }}
"#,
        source_repo.file_url()
    );
    project.write_manifest(&manifest_two_agents)?;

    // Install both agents
    let output = project.run_ccpm(&["install", "--quiet"])?;
    assert!(output.success, "Initial install failed: {}", output.stderr);

    // Remove agent-two from manifest (simulating it becoming only a transitive dependency)
    let manifest_one_agent = format!(
        r#"[sources]
test-source = "{}"

[agents]
agent-one = {{ source = "test-source", path = "agents/agent-one.md", version = "v1.0.0" }}
"#,
        source_repo.file_url()
    );
    project.write_manifest(&manifest_one_agent)?;

    // Try to install with lockfile containing extra entry in CI mode
    // This should succeed now - extra entries are allowed for transitive dependencies
    let output = project.run_ccpm_with_env(&["install"], &[("CI", "true")])?;
    assert!(
        output.success,
        "Install should succeed with extra lockfile entries for transitive deps: {}",
        output.stderr
    );
    assert!(
        !output.stderr.contains("stale"),
        "Should not report lockfile as stale"
    );

    Ok(())
}

/// Test that path change is detected
#[test]
fn test_install_detects_path_change() -> Result<()> {
    let project = TestProject::new()?;

    // Create a source repo with agent in two locations
    let source_repo = project.create_source_repo("test-source")?;
    source_repo.add_resource("agents", "old-path", "# Agent at old path")?;
    source_repo.add_resource("agents", "new-path", "# Agent at new path")?;
    source_repo.commit_all("Add agents")?;
    source_repo.tag_version("v1.0.0")?;

    // Create manifest pointing to old path
    let manifest_old = format!(
        r#"[sources]
test-source = "{}"

[agents]
test-agent = {{ source = "test-source", path = "agents/old-path.md", version = "v1.0.0" }}
"#,
        source_repo.file_url()
    );
    project.write_manifest(&manifest_old)?;

    // Install with old path
    let output = project.run_ccpm(&["install", "--quiet"])?;
    assert!(output.success, "Initial install failed: {}", output.stderr);

    // Update manifest to new path
    let manifest_new = manifest_old.replace("old-path", "new-path");
    project.write_manifest(&manifest_new)?;

    // Try to install with outdated lockfile in CI mode
    let output = project.run_ccpm_with_env(&["install"], &[("CI", "true")])?;
    assert!(!output.success);
    assert!(output.stderr.contains("path changed from"));

    Ok(())
}

/// Test that source URL change is detected
#[test]
fn test_install_detects_source_url_change() -> Result<()> {
    let project = TestProject::new()?;

    // Create two different source repos
    let old_repo = project.create_source_repo("old-repo")?;
    old_repo.add_resource("agents", "test-agent", "# Agent from old repo")?;
    old_repo.commit_all("Add agent")?;
    old_repo.tag_version("v1.0.0")?;

    let new_repo = project.create_source_repo("new-repo")?;
    new_repo.add_resource("agents", "test-agent", "# Agent from new repo")?;
    new_repo.commit_all("Add agent")?;
    new_repo.tag_version("v1.0.0")?;

    // Create manifest pointing to old repo
    let manifest_old = format!(
        r#"[sources]
test-source = "{}"

[agents]
test-agent = {{ source = "test-source", path = "agents/test-agent.md", version = "v1.0.0" }}
"#,
        old_repo.file_url()
    );
    project.write_manifest(&manifest_old)?;

    // Install from old repo
    let output = project.run_ccpm(&["install", "--quiet"])?;
    assert!(output.success, "Initial install failed: {}", output.stderr);

    // Update manifest to point to new repo
    let manifest_new = format!(
        r#"[sources]
test-source = "{}"

[agents]
test-agent = {{ source = "test-source", path = "agents/test-agent.md", version = "v1.0.0" }}
"#,
        new_repo.file_url()
    );
    project.write_manifest(&manifest_new)?;

    // Try to install with outdated lockfile in CI mode
    let output = project.run_ccpm_with_env(&["install"], &[("CI", "true")])?;
    assert!(!output.success);
    assert!(
        output
            .stderr
            .contains("Source repository 'test-source' URL changed")
    );

    Ok(())
}

/// Test that duplicate entries are detected
#[test]
fn test_install_detects_duplicate_entries() -> Result<()> {
    let project = TestProject::new()?;

    // Create a source repo
    let source_repo = project.create_source_repo("test-source")?;
    source_repo.add_resource("agents", "test-agent", "# Test Agent")?;
    source_repo.commit_all("Add agent")?;

    // Create and set main branch for the test repo
    source_repo.git.create_branch("main")?;

    // Create manifest
    let manifest = format!(
        r#"[sources]
test-source = "{}"

[agents]
test-agent = {{ source = "test-source", path = "agents/test-agent.md", version = "main" }}
"#,
        source_repo.file_url()
    );
    project.write_manifest(&manifest)?;

    // First do a normal install to create a valid lockfile
    let output = project.run_ccpm(&["install", "--quiet"])?;
    assert!(output.success, "Initial install failed: {}", output.stderr);

    // Read the valid lockfile and manually duplicate an entry
    let lockfile = project.read_lockfile()?;

    // Find the agents section and duplicate the first agent entry
    if let Some(agents_pos) = lockfile.find("[[agents]]") {
        let agent_section = &lockfile[agents_pos..];

        // Find the end of this agent entry
        let next_section = agent_section[11..]
            .find("[[")
            .unwrap_or(agent_section.len() - 11)
            + 11;
        let agent_entry = &agent_section[..next_section];

        // Add a duplicate at the end
        let corrupted_lockfile = format!("{}\n{}", lockfile.trim(), agent_entry);
        project.write_lockfile(&corrupted_lockfile)?;
    } else {
        panic!("Could not find agents section in lockfile");
    }

    // Try to install with corrupted lockfile in CI mode
    let output = project.run_ccpm_with_env(&["install"], &[("CI", "true")])?;
    assert!(
        !output.success,
        "Expected failure due to duplicate entries, but command succeeded"
    );
    assert!(
        output.stderr.contains("duplicate entries")
            || output.stderr.contains("Found") && output.stderr.contains("duplicate"),
        "Expected duplicate entries error, got: {}",
        output.stderr
    );

    Ok(())
}

/// Test that branch references don't cause staleness errors
/// Branches are expected to move, so we shouldn't treat them as stale
#[test]
fn test_install_allows_branch_references() -> Result<()> {
    let project = TestProject::new()?;

    // Create a source repo with a bare clone for stable serving
    let source_repo = project.create_source_repo("test-source")?;
    source_repo.add_resource("agents", "test-agent", "# Test Agent v1")?;
    source_repo.commit_all("Initial commit")?;

    // Create and set main branch for the test repo
    source_repo.git.create_branch("main")?;

    // Get the bare URL for stable file:// serving
    let bare_url = source_repo.bare_file_url(project.sources_path())?;

    // Create manifest using 'main' branch
    let manifest = format!(
        r#"[sources]
test-source = "{}"

[agents]
test-agent = {{ source = "test-source", path = "agents/test-agent.md", version = "main" }}
"#,
        bare_url
    );
    project.write_manifest(&manifest)?;

    // Install to create lockfile
    let output = project.run_ccpm(&["install", "--quiet"])?;
    assert!(output.success, "Initial install failed: {}", output.stderr);

    // Try to install again - should succeed because branches are allowed to move
    // This is expected behavior, not a staleness issue
    let output = project.run_ccpm_with_env(&["install"], &[("CI", "true")])?;
    assert!(
        output.success,
        "Install should succeed with branch references, got error: {}",
        output.stderr
    );

    Ok(())
}

/// Test that --force and --regenerate flags are mutually exclusive
#[test]
fn test_force_and_regenerate_are_mutually_exclusive() -> Result<()> {
    let project = TestProject::new()?;

    // Create minimal manifest
    let manifest = ManifestFixture::basic();
    project.write_manifest(&manifest.content)?;

    // Try to use both flags
    let output = project.run_ccpm(&["install", "--force", "--regenerate"])?;
    assert!(!output.success, "Command should have failed but succeeded");

    // Check for clap's conflict error messages
    assert!(
        output.stderr.contains("cannot be used with")
            || output.stderr.contains("conflicts with")
            || output
                .stderr
                .contains("the following required arguments were not provided")
            || output.stderr.contains("error:"),
        "Expected conflict error, got: {}",
        output.stderr
    );

    Ok(())
}

/// Test CI vs interactive environment detection
#[test]
fn test_ci_environment_detection() -> Result<()> {
    let project = TestProject::new()?;

    // Create a source repo
    let source_repo = project.create_source_repo("test-source")?;
    source_repo.add_resource("agents", "test-agent", "# Test Agent")?;
    source_repo.commit_all("Add agent")?;
    source_repo.tag_version("v1.0.0")?;

    // Create manifest with dependency
    let manifest = format!(
        r#"[sources]
test-source = "{}"

[agents]
test-agent = {{ source = "test-source", path = "agents/test-agent.md", version = "v1.0.0" }}
"#,
        source_repo.file_url()
    );
    project.write_manifest(&manifest)?;

    // Create empty lockfile (stale)
    project.write_lockfile("version = 1")?;

    // Test CI environment (CI=true)
    let output = project.run_ccpm_with_env(&["install"], &[("CI", "true")])?;
    assert!(!output.success);
    assert!(output.stderr.contains("Lockfile is stale"));
    assert!(
        output.stderr.contains("Run with --force")
            || output.stderr.contains("regenerate the lockfile")
    );

    // Test GitHub Actions environment
    let output = project.run_ccpm_with_env(&["install"], &[("GITHUB_ACTIONS", "true")])?;
    assert!(!output.success);
    assert!(output.stderr.contains("Lockfile is stale"));
    assert!(
        output.stderr.contains("Run with --force")
            || output.stderr.contains("regenerate the lockfile")
    );

    // Test quiet mode
    let output = project.run_ccpm(&["install", "--quiet"])?;
    assert!(!output.success); // Should fail without prompting

    Ok(())
}
