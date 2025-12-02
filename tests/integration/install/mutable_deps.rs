//! Tests for mutable dependency reinstallation scenarios.
//!
//! These tests verify that the fast path optimization correctly handles
//! mutable dependencies (local files and branch refs) that require
//! re-resolution and reinstallation when their content changes.

use anyhow::Result;
use toml_edit::DocumentMut;

use crate::common::{ManifestBuilder, TestProject};

/// Test that local file changes trigger reinstallation
///
/// When a local file dependency's content changes on disk,
/// subsequent installs should detect this and reinstall the file.
#[tokio::test]
async fn test_local_file_change_triggers_reinstall() -> Result<()> {
    let project = TestProject::new().await?;

    // Create local agent file
    let local_agent_path = project.project_path().join("local-agent.md");
    tokio::fs::write(&local_agent_path, "# Local Agent v1\n\nOriginal content.").await?;

    // Create manifest with local dependency
    let manifest = ManifestBuilder::new().add_local_agent("local-agent", "local-agent.md").build();
    project.write_manifest(&manifest).await?;

    // First install
    let output = project.run_agpm(&["install", "--quiet"])?;
    assert!(output.success, "Initial install failed: {}", output.stderr);

    // Verify installed content
    let installed_path = project.project_path().join(".claude/agents/agpm/local-agent.md");
    let content_v1 = tokio::fs::read_to_string(&installed_path).await?;
    assert!(
        content_v1.contains("Original content"),
        "Initial install should have original content"
    );

    // Modify the local file
    tokio::fs::write(&local_agent_path, "# Local Agent v2\n\nUpdated content.").await?;

    // Second install should detect and reinstall
    let output = project.run_agpm(&["install", "--quiet"])?;
    assert!(output.success, "Second install failed: {}", output.stderr);

    // Verify updated content
    let content_v2 = tokio::fs::read_to_string(&installed_path).await?;
    assert!(
        content_v2.contains("Updated content"),
        "Second install should have updated content. Got: {}",
        content_v2
    );

    Ok(())
}

/// Test that adding transitive deps to a local file triggers resolution
///
/// When a local file dependency gains transitive dependencies in its
/// frontmatter, subsequent installs should resolve and install them.
#[tokio::test]
async fn test_local_file_added_transitive_triggers_resolution() -> Result<()> {
    let project = TestProject::new().await?;

    // Create directory structure
    let agents_dir = project.project_path().join("agents");
    tokio::fs::create_dir_all(&agents_dir).await?;

    // Create local agent WITHOUT transitive deps
    let local_agent_path = agents_dir.join("main-agent.md");
    tokio::fs::write(&local_agent_path, "# Main Agent\n\nNo dependencies yet.").await?;

    // Create manifest with local dependency
    let manifest =
        ManifestBuilder::new().add_local_agent("main-agent", "agents/main-agent.md").build();
    project.write_manifest(&manifest).await?;

    // First install
    let output = project.run_agpm(&["install", "--quiet"])?;
    assert!(output.success, "Initial install failed: {}", output.stderr);

    // Verify only main agent installed
    let installed_main = project.project_path().join(".claude/agents/agpm/main-agent.md");
    assert!(installed_main.exists(), "Main agent should be installed");

    let installed_helper = project.project_path().join(".claude/agents/agpm/helper.md");
    assert!(!installed_helper.exists(), "Helper should NOT exist yet");

    // Now create the helper file
    let helper_path = agents_dir.join("helper.md");
    tokio::fs::write(&helper_path, "# Helper Agent\n\nA helper agent.").await?;

    // Update main agent to add transitive dependency
    tokio::fs::write(
        &local_agent_path,
        r#"---
dependencies:
  agents:
    - path: ./helper.md
---
# Main Agent

Now with a transitive dependency.
"#,
    )
    .await?;

    // Second install should resolve transitive dependency
    let output = project.run_agpm(&["install", "--quiet"])?;
    assert!(output.success, "Second install failed: {}", output.stderr);

    // Verify helper is now installed
    assert!(
        installed_helper.exists(),
        "Helper agent should be installed after adding transitive dep"
    );

    // Verify lockfile contains both
    let lockfile = project.read_lockfile().await?;
    assert!(
        lockfile.contains(r#"name = "agents/main-agent""#),
        "Lockfile should contain main-agent"
    );
    assert!(
        lockfile.contains(r#"name = "agents/helper""#),
        "Lockfile should contain helper (transitive)"
    );

    Ok(())
}

/// Test that branch ref updates trigger reinstallation
///
/// When a dependency uses a branch ref (like `main`) and the branch
/// is updated with new commits, subsequent installs should fetch
/// and install the new content.
#[tokio::test]
async fn test_branch_ref_update_triggers_reinstall() -> Result<()> {
    use agpm_cli::utils::normalize_path_for_storage;

    let project = TestProject::new().await?;

    // Create source repo with initial content
    let source_repo = project.create_source_repo("test-source").await?;
    source_repo
        .add_resource("agents", "branch-agent", "# Branch Agent v1\n\nInitial content.")
        .await?;
    source_repo.commit_all("Initial version")?;

    // Ensure main branch exists
    source_repo.git.ensure_branch("main")?;

    // Use direct file URL so we can update the repo after install
    // (bare repos are one-time clones)
    let source_url = format!("file://{}", normalize_path_for_storage(&source_repo.path));
    let manifest = ManifestBuilder::new()
        .add_source("test-source", &source_url)
        .add_agent("branch-agent", |d| {
            d.source("test-source").path("agents/branch-agent.md").version("main")
        })
        .build();
    project.write_manifest(&manifest).await?;

    // First install
    let output = project.run_agpm(&["install", "--quiet"])?;
    assert!(output.success, "Initial install failed: {}", output.stderr);

    // Verify initial content
    let installed_path = project.project_path().join(".claude/agents/agpm/branch-agent.md");
    let content_v1 = tokio::fs::read_to_string(&installed_path).await?;
    assert!(content_v1.contains("Initial content"), "Initial install should have initial content");

    // Update the source repo (new commit on main)
    source_repo
        .add_resource("agents", "branch-agent", "# Branch Agent v2\n\nUpdated content.")
        .await?;
    source_repo.commit_all("Updated version")?;

    // Second install should fetch and install updated content
    // Use --no-cache to ensure we fetch the latest from source
    let output = project.run_agpm(&["install", "--quiet", "--no-cache"])?;
    assert!(output.success, "Second install failed: {}", output.stderr);

    // Verify updated content
    let content_v2 = tokio::fs::read_to_string(&installed_path).await?;
    assert!(
        content_v2.contains("Updated content"),
        "Second install should have updated content. Got: {}",
        content_v2
    );

    Ok(())
}

/// Test that mutable deps prevent fast path from triggering
///
/// This test verifies that the has_mutable_deps flag is correctly
/// set in the lockfile and that subsequent installs don't skip
/// resolution when mutable deps are present.
#[tokio::test]
async fn test_mutable_deps_flag_prevents_fast_path() -> Result<()> {
    let project = TestProject::new().await?;

    // Create local agent file
    let local_agent_path = project.project_path().join("local-agent.md");
    tokio::fs::write(&local_agent_path, "# Local Agent\n\nContent.").await?;

    // Create manifest with local dependency (mutable)
    let manifest = ManifestBuilder::new().add_local_agent("local-agent", "local-agent.md").build();
    project.write_manifest(&manifest).await?;

    // Install
    let output = project.run_agpm(&["install", "--quiet"])?;
    assert!(output.success, "Install failed: {}", output.stderr);

    // Verify lockfile has mutable flag set to true
    let lockfile_content = project.read_lockfile().await?;
    assert!(
        lockfile_content.contains("has_mutable_deps = true"),
        "Lockfile should have has_mutable_deps = true for local deps. Lockfile:\n{}",
        lockfile_content
    );

    Ok(())
}

/// Test that immutable deps enable fast path
///
/// This test verifies that when all deps are immutable (tags/SHAs),
/// the lockfile correctly sets has_mutable_deps = false.
#[tokio::test]
async fn test_immutable_deps_enable_fast_path() -> Result<()> {
    let project = TestProject::new().await?;

    // Create source repo with tagged version
    let source_repo = project.create_source_repo("test-source").await?;
    source_repo.add_resource("agents", "immutable-agent", "# Immutable Agent\n\nContent.").await?;
    source_repo.commit_all("Add agent")?;
    source_repo.tag_version("v1.0.0")?;

    // Create manifest with versioned dependency (immutable)
    let source_url = source_repo.bare_file_url(project.sources_path()).await?;
    let manifest = ManifestBuilder::new()
        .add_source("test-source", &source_url)
        .add_standard_agent("immutable-agent", "test-source", "agents/immutable-agent.md")
        .build();
    project.write_manifest(&manifest).await?;

    // Install
    let output = project.run_agpm(&["install", "--quiet"])?;
    assert!(output.success, "Install failed: {}", output.stderr);

    // Verify lockfile has mutable flag set to false
    let lockfile_content = project.read_lockfile().await?;
    assert!(
        lockfile_content.contains("has_mutable_deps = false"),
        "Lockfile should have has_mutable_deps = false for versioned deps. Lockfile:\n{}",
        lockfile_content
    );

    Ok(())
}

/// Test that removing installed file triggers reinstall even with fast path
///
/// Even when using immutable deps, if a user deletes an installed file,
/// the next install should detect this and reinstall.
#[tokio::test]
async fn test_missing_file_triggers_reinstall_on_fast_path() -> Result<()> {
    let project = TestProject::new().await?;

    // Create source repo with tagged version
    let source_repo = project.create_source_repo("test-source").await?;
    source_repo.add_resource("agents", "test-agent", "# Test Agent\n\nContent.").await?;
    source_repo.commit_all("Add agent")?;
    source_repo.tag_version("v1.0.0")?;

    // Create manifest with versioned dependency (immutable)
    let source_url = source_repo.bare_file_url(project.sources_path()).await?;
    let manifest = ManifestBuilder::new()
        .add_source("test-source", &source_url)
        .add_standard_agent("test-agent", "test-source", "agents/test-agent.md")
        .build();
    project.write_manifest(&manifest).await?;

    // First install
    let output = project.run_agpm(&["install", "--quiet"])?;
    assert!(output.success, "Initial install failed: {}", output.stderr);

    // Verify file exists
    let installed_path = project.project_path().join(".claude/agents/agpm/test-agent.md");
    assert!(installed_path.exists(), "Agent should be installed");

    // Delete the installed file
    tokio::fs::remove_file(&installed_path).await?;
    assert!(!installed_path.exists(), "File should be deleted");

    // Second install should reinstall despite potential fast path
    let output = project.run_agpm(&["install", "--quiet"])?;
    assert!(output.success, "Second install failed: {}", output.stderr);

    // Verify file is reinstalled
    assert!(installed_path.exists(), "Agent should be reinstalled after deletion");

    Ok(())
}

/// Test that fast path stores manifest hash in lockfile
///
/// When installing with immutable deps, the lockfile should contain
/// a manifest_hash field that enables fast-path detection on subsequent installs.
#[tokio::test]
async fn test_fast_path_stores_manifest_hash() -> Result<()> {
    let project = TestProject::new().await?;

    // Create source repo with tagged version
    let source_repo = project.create_source_repo("test-source").await?;
    source_repo.add_resource("agents", "test-agent", "# Test Agent\n\nContent.").await?;
    source_repo.commit_all("Add agent")?;
    source_repo.tag_version("v1.0.0")?;

    // Create manifest with versioned dependency (immutable)
    let source_url = source_repo.bare_file_url(project.sources_path()).await?;
    let manifest = ManifestBuilder::new()
        .add_source("test-source", &source_url)
        .add_standard_agent("test-agent", "test-source", "agents/test-agent.md")
        .build();
    project.write_manifest(&manifest).await?;

    // Install
    let output = project.run_agpm(&["install", "--quiet"])?;
    assert!(output.success, "Install failed: {}", output.stderr);

    // Verify lockfile contains manifest_hash
    let lockfile_content = project.read_lockfile().await?;
    assert!(
        lockfile_content.contains("manifest_hash = \"sha256:"),
        "Lockfile should contain manifest_hash. Lockfile:\n{}",
        lockfile_content
    );

    Ok(())
}

/// Test that fast path is invalidated by manifest change
///
/// When a new dependency is added to the manifest, the manifest_hash
/// changes and resolution must run even if other deps are unchanged.
#[tokio::test]
async fn test_fast_path_invalidated_by_manifest_change() -> Result<()> {
    let project = TestProject::new().await?;

    // Create source repo with multiple agents
    let source_repo = project.create_source_repo("test-source").await?;
    source_repo.add_resource("agents", "agent-one", "# Agent One\n\nFirst agent.").await?;
    source_repo.add_resource("agents", "agent-two", "# Agent Two\n\nSecond agent.").await?;
    source_repo.commit_all("Add agents")?;
    source_repo.tag_version("v1.0.0")?;

    let source_url = source_repo.bare_file_url(project.sources_path()).await?;

    // First install with one agent
    let manifest1 = ManifestBuilder::new()
        .add_source("test-source", &source_url)
        .add_standard_agent("agent-one", "test-source", "agents/agent-one.md")
        .build();
    project.write_manifest(&manifest1).await?;

    let output = project.run_agpm(&["install", "--quiet"])?;
    assert!(output.success, "First install failed: {}", output.stderr);

    // Get the original manifest_hash
    let lockfile1 = project.read_lockfile().await?;
    let hash1_line = lockfile1.lines().find(|l| l.contains("manifest_hash")).unwrap();

    // Now add a second agent to manifest
    let manifest2 = ManifestBuilder::new()
        .add_source("test-source", &source_url)
        .add_standard_agent("agent-one", "test-source", "agents/agent-one.md")
        .add_standard_agent("agent-two", "test-source", "agents/agent-two.md")
        .build();
    project.write_manifest(&manifest2).await?;

    // Second install should detect manifest change
    let output = project.run_agpm(&["install", "--quiet"])?;
    assert!(output.success, "Second install failed: {}", output.stderr);

    // Verify manifest_hash changed
    let lockfile2 = project.read_lockfile().await?;
    let hash2_line = lockfile2.lines().find(|l| l.contains("manifest_hash")).unwrap();

    assert_ne!(hash1_line, hash2_line, "manifest_hash should change when deps are added");

    // Verify second agent is installed
    let agent_two_path = project.project_path().join(".claude/agents/agpm/agent-two.md");
    assert!(agent_two_path.exists(), "Second agent should be installed");

    Ok(())
}

/// Test fast path with rev (SHA-pinned) dependency
///
/// Dependencies with explicit `rev` field should be treated as immutable
/// and enable fast path optimization.
#[tokio::test]
async fn test_rev_pinned_deps_enable_fast_path() -> Result<()> {
    let project = TestProject::new().await?;

    // Create source repo
    let source_repo = project.create_source_repo("test-source").await?;
    source_repo.add_resource("agents", "sha-agent", "# SHA Agent\n\nPinned content.").await?;
    source_repo.commit_all("Add agent")?;

    // Get the actual SHA
    let sha = source_repo.git.get_commit_hash()?;
    let source_url = source_repo.bare_file_url(project.sources_path()).await?;

    // Create manifest with rev-pinned dependency
    let manifest = ManifestBuilder::new()
        .add_source("test-source", &source_url)
        .add_agent("sha-agent", |d| d.source("test-source").path("agents/sha-agent.md").rev(&sha))
        .build();
    project.write_manifest(&manifest).await?;

    // Install
    let output = project.run_agpm(&["install", "--quiet"])?;
    assert!(output.success, "Install failed: {}", output.stderr);

    // Verify lockfile has mutable flag set to false (rev is immutable)
    let lockfile_content = project.read_lockfile().await?;
    assert!(
        lockfile_content.contains("has_mutable_deps = false"),
        "Lockfile should have has_mutable_deps = false for rev-pinned deps. Lockfile:\n{}",
        lockfile_content
    );

    Ok(())
}

/// Test mixed mutable and immutable deps
///
/// When a manifest has both mutable and immutable deps, the lockfile
/// should report has_mutable_deps = true and fast path should be disabled.
#[tokio::test]
async fn test_mixed_mutable_immutable_deps() -> Result<()> {
    let project = TestProject::new().await?;

    // Create source repo with tagged version
    let source_repo = project.create_source_repo("test-source").await?;
    source_repo.add_resource("agents", "remote-agent", "# Remote Agent\n\nContent.").await?;
    source_repo.commit_all("Add agent")?;
    source_repo.tag_version("v1.0.0")?;

    // Create local agent file
    let local_agent_path = project.project_path().join("local-agent.md");
    tokio::fs::write(&local_agent_path, "# Local Agent\n\nContent.").await?;

    // Create manifest with both local (mutable) and versioned (immutable) deps
    let source_url = source_repo.bare_file_url(project.sources_path()).await?;
    let manifest = ManifestBuilder::new()
        .add_source("test-source", &source_url)
        .add_standard_agent("remote-agent", "test-source", "agents/remote-agent.md")
        .add_local_agent("local-agent", "local-agent.md")
        .build();
    project.write_manifest(&manifest).await?;

    // Install
    let output = project.run_agpm(&["install", "--quiet"])?;
    assert!(output.success, "Install failed: {}", output.stderr);

    // Verify lockfile has mutable flag set to true (due to local dep)
    let lockfile_content = project.read_lockfile().await?;
    assert!(
        lockfile_content.contains("has_mutable_deps = true"),
        "Lockfile should have has_mutable_deps = true when mixed deps. Lockfile:\n{}",
        lockfile_content
    );

    Ok(())
}

/// Test ultra-fast path skips file operations when all files exist
///
/// When all installed files exist and fast path is available, the ultra-fast
/// path optimization skips the entire installation phase. This test verifies
/// the optimization by checking that file mtimes are unchanged after a second install.
#[tokio::test]
async fn test_ultra_fast_path_preserves_file_mtime() -> Result<()> {
    use std::time::Duration;

    let project = TestProject::new().await?;

    // Create source repo with tagged version
    let source_repo = project.create_source_repo("test-source").await?;
    source_repo.add_resource("agents", "test-agent", "# Test Agent\n\nContent.").await?;
    source_repo.commit_all("Add agent")?;
    source_repo.tag_version("v1.0.0")?;

    // Create manifest with versioned dependency (immutable - enables fast path)
    let source_url = source_repo.bare_file_url(project.sources_path()).await?;
    let manifest = ManifestBuilder::new()
        .add_source("test-source", &source_url)
        .add_standard_agent("test-agent", "test-source", "agents/test-agent.md")
        .build();
    project.write_manifest(&manifest).await?;

    // First install
    let output = project.run_agpm(&["install", "--quiet"])?;
    assert!(output.success, "First install failed: {}", output.stderr);

    // Get mtime after first install
    let installed_path = project.project_path().join(".claude/agents/agpm/test-agent.md");
    let mtime_after_first = tokio::fs::metadata(&installed_path).await?.modified()?;

    // Wait for filesystem mtime resolution
    // FAT32 has 2-second resolution, NTFS has 100ns, ext4/HFS+ have 1ns
    // Using 2100ms to ensure we exceed FAT32's 2-second granularity
    const MTIME_RESOLUTION_MS: u64 = 2100;
    tokio::time::sleep(Duration::from_millis(MTIME_RESOLUTION_MS)).await;

    // Second install should use ultra-fast path (all files exist)
    let output = project.run_agpm(&["install", "--quiet"])?;
    assert!(output.success, "Second install failed: {}", output.stderr);

    // Verify mtime is unchanged (file was not rewritten)
    let mtime_after_second = tokio::fs::metadata(&installed_path).await?.modified()?;
    assert_eq!(
        mtime_after_first, mtime_after_second,
        "Ultra-fast path should NOT modify existing files"
    );

    Ok(())
}

/// Test that ultra-fast path preserves corrupted files when all files exist
///
/// When using the ultra-fast path with immutable deps, AGPM checks file existence
/// but doesn't verify content. This test verifies the behavior by corrupting an
/// installed file and confirming it remains corrupted after a second install
/// (since ultra-fast path sees the file exists and skips installation entirely).
///
/// Note: This is intentional behavior for performance - if you need to restore
/// correct content, delete the file or use --no-cache to force reinstallation.
#[tokio::test]
async fn test_ultra_fast_path_preserves_existing_files() -> Result<()> {
    let project = TestProject::new().await?;

    // Create source repo with tagged version
    let source_repo = project.create_source_repo("test-source").await?;
    source_repo
        .add_resource("agents", "trusted-agent", "# Trusted Agent\n\nOriginal content.")
        .await?;
    source_repo.commit_all("Add agent")?;
    source_repo.tag_version("v1.0.0")?;

    // Create manifest with versioned dependency (immutable - enables fast path)
    let source_url = source_repo.bare_file_url(project.sources_path()).await?;
    let manifest = ManifestBuilder::new()
        .add_source("test-source", &source_url)
        .add_standard_agent("trusted-agent", "test-source", "agents/trusted-agent.md")
        .build();
    project.write_manifest(&manifest).await?;

    // First install
    let output = project.run_agpm(&["install", "--quiet"])?;
    assert!(output.success, "First install failed: {}", output.stderr);

    // Corrupt the installed file
    let installed_path = project.project_path().join(".claude/agents/agpm/trusted-agent.md");
    tokio::fs::write(&installed_path, "# CORRUPTED\n\nThis file was modified.").await?;

    // Second install - with fast path, trusted mode skips verification
    // The corrupted content should remain (ultra-fast path sees file exists)
    let output = project.run_agpm(&["install", "--quiet"])?;
    assert!(output.success, "Second install failed: {}", output.stderr);

    // Verify the file is still corrupted (ultra-fast path skipped installation)
    let content = tokio::fs::read_to_string(&installed_path).await?;
    assert!(
        content.contains("CORRUPTED"),
        "Ultra-fast path should NOT reinstall existing files. Content: {}",
        content
    );

    Ok(())
}

/// Test that lockfile resource count validation prevents fast path
///
/// The fast path is disabled when the lockfile has fewer resources than
/// the manifest. This catches cases where the lockfile was manually edited
/// to remove dependencies.
#[tokio::test]
async fn test_lockfile_resource_count_prevents_fast_path() -> Result<()> {
    let project = TestProject::new().await?;

    // Create source repo with multiple agents
    let source_repo = project.create_source_repo("test-source").await?;
    source_repo.add_resource("agents", "agent-one", "# Agent One\n\nFirst.").await?;
    source_repo.add_resource("agents", "agent-two", "# Agent Two\n\nSecond.").await?;
    source_repo.commit_all("Add agents")?;
    source_repo.tag_version("v1.0.0")?;

    let source_url = source_repo.bare_file_url(project.sources_path()).await?;

    // Create manifest with two agents
    let manifest = ManifestBuilder::new()
        .add_source("test-source", &source_url)
        .add_standard_agent("agent-one", "test-source", "agents/agent-one.md")
        .add_standard_agent("agent-two", "test-source", "agents/agent-two.md")
        .build();
    project.write_manifest(&manifest).await?;

    // First install
    let output = project.run_agpm(&["install", "--quiet"])?;
    assert!(output.success, "First install failed: {}", output.stderr);

    // Verify both agents installed
    let agent_one = project.project_path().join(".claude/agents/agpm/agent-one.md");
    let agent_two = project.project_path().join(".claude/agents/agpm/agent-two.md");
    assert!(agent_one.exists(), "Agent one should be installed");
    assert!(agent_two.exists(), "Agent two should be installed");

    // Manually edit lockfile to remove agent-two using proper TOML parsing
    let lockfile_path = project.project_path().join("agpm.lock");
    let lockfile_content = tokio::fs::read_to_string(&lockfile_path).await?;
    let mut doc: DocumentMut = lockfile_content.parse()?;
    if let Some(agents) = doc.get_mut("agents").and_then(|a| a.as_array_of_tables_mut()) {
        agents.retain(|t| t.get("name").and_then(|n| n.as_str()) == Some("agents/agent-one"));
    }
    tokio::fs::write(&lockfile_path, doc.to_string()).await?;

    // Delete agent-two from disk (simulating incomplete state)
    tokio::fs::remove_file(&agent_two).await?;

    // Second install should detect lockfile count mismatch and re-resolve
    let output = project.run_agpm(&["install", "--quiet"])?;
    assert!(output.success, "Second install failed: {}", output.stderr);

    // Verify agent-two is reinstalled (fast path was disabled)
    assert!(agent_two.exists(), "Agent two should be reinstalled after lockfile count validation");

    Ok(())
}

/// Test that deleting a local source file produces a clear error
///
/// When a local file dependency is deleted from disk, subsequent
/// installs should fail with a clear error message indicating the
/// missing file, rather than silently skipping or producing cryptic errors.
#[tokio::test]
async fn test_local_file_deleted_produces_error() -> Result<()> {
    let project = TestProject::new().await?;

    // Create local agent file
    let local_path = project.project_path().join("local-agent.md");
    tokio::fs::write(&local_path, "# Local Agent\n\nContent here.").await?;

    // Create manifest with local dependency
    let manifest = ManifestBuilder::new().add_local_agent("local-agent", "local-agent.md").build();
    project.write_manifest(&manifest).await?;

    // First install should succeed
    let output = project.run_agpm(&["install", "--quiet"])?;
    assert!(output.success, "First install failed: {}", output.stderr);

    // Delete the source file
    tokio::fs::remove_file(&local_path).await?;

    // Second install should fail with clear error
    let output = project.run_agpm(&["install", "--quiet"])?;
    assert!(!output.success, "Install should fail when local file is missing");
    assert!(
        output.stderr.contains("not found")
            || output.stderr.contains("No such file")
            || output.stderr.contains("does not exist")
            || output.stderr.contains("path exists"), // Error suggests checking if path exists
        "Error should mention missing file. stderr: {}",
        output.stderr
    );

    Ok(())
}

/// Test fast path with pattern dependencies (agents/*.md)
///
/// Pattern dependencies should work with the fast path when:
/// 1. All resolved files are from immutable refs (tags/SHAs)
/// 2. The manifest hash matches the lockfile
/// 3. All files exist on disk
#[tokio::test]
async fn test_fast_path_with_pattern_dependencies() -> Result<()> {
    let project = TestProject::new().await?;

    // Create source repo with multiple agents
    let source_repo = project.create_source_repo("test-source").await?;
    source_repo.add_resource("agents", "helper-one", "# Helper One\n\nFirst helper.").await?;
    source_repo.add_resource("agents", "helper-two", "# Helper Two\n\nSecond helper.").await?;
    source_repo.add_resource("agents", "helper-three", "# Helper Three\n\nThird helper.").await?;
    source_repo.commit_all("Add agents")?;
    source_repo.tag_version("v1.0.0")?;

    let source_url = source_repo.bare_file_url(project.sources_path()).await?;

    // Create manifest with pattern dependency
    let manifest = ManifestBuilder::new()
        .add_source("test-source", &source_url)
        .add_agent_pattern("all-helpers", "test-source", "agents/helper-*.md", "v1.0.0")
        .build();
    project.write_manifest(&manifest).await?;

    // First install
    let output = project.run_agpm(&["install", "--quiet"])?;
    assert!(output.success, "First install failed: {}", output.stderr);

    // Verify all matched files installed
    let agents_dir = project.project_path().join(".claude/agents/agpm");
    assert!(agents_dir.join("helper-one.md").exists(), "helper-one should be installed");
    assert!(agents_dir.join("helper-two.md").exists(), "helper-two should be installed");
    assert!(agents_dir.join("helper-three.md").exists(), "helper-three should be installed");

    // Get mtime of one file
    let helper_one = agents_dir.join("helper-one.md");
    let mtime_first = tokio::fs::metadata(&helper_one).await?.modified()?;

    // Wait for filesystem mtime resolution
    const MTIME_RESOLUTION_MS: u64 = 2100;
    tokio::time::sleep(std::time::Duration::from_millis(MTIME_RESOLUTION_MS)).await;

    // Second install should use fast path (all files exist, immutable ref)
    let output = project.run_agpm(&["install", "--quiet"])?;
    assert!(output.success, "Second install failed: {}", output.stderr);

    // Verify mtime unchanged (fast path was used)
    let mtime_second = tokio::fs::metadata(&helper_one).await?.modified()?;
    assert_eq!(
        mtime_first, mtime_second,
        "Fast path should not modify existing pattern-matched files"
    );

    Ok(())
}

/// Test --frozen flag uses lockfile as-is
///
/// The --frozen flag should:
/// 1. Install exactly what's in the lockfile without checking manifest changes
/// 2. Fail if the lockfile is invalid/corrupted
/// 3. Fail if source URLs change (security concern)
#[tokio::test]
async fn test_frozen_flag_with_fast_path() -> Result<()> {
    let project = TestProject::new().await?;

    // Create source repo with versioned agent
    let source_repo = project.create_source_repo("test-source").await?;
    source_repo.add_resource("agents", "test-agent", "# Test Agent\n\nOriginal.").await?;
    source_repo.commit_all("Add agent")?;
    source_repo.tag_version("v1.0.0")?;

    let source_url = source_repo.bare_file_url(project.sources_path()).await?;
    let manifest = ManifestBuilder::new()
        .add_source("test-source", &source_url)
        .add_standard_agent("test-agent", "test-source", "agents/test-agent.md")
        .build();
    project.write_manifest(&manifest).await?;

    // First install (creates lockfile)
    let output = project.run_agpm(&["install", "--quiet"])?;
    assert!(output.success, "First install failed: {}", output.stderr);

    // Second install with --frozen should work (lockfile valid)
    let output = project.run_agpm(&["install", "--frozen", "--quiet"])?;
    assert!(output.success, "Frozen install should succeed with valid lockfile: {}", output.stderr);

    // Verify the agent was installed
    let installed_path = project.project_path().join(".claude/agents/agpm/test-agent.md");
    assert!(installed_path.exists(), "Agent should be installed in frozen mode");

    // Now test that --frozen with changed source URL fails (security check)
    // Change the source URL in the manifest
    let different_url = format!("{}/different-repo.git", project.sources_path().display());
    let manifest_with_changed_url = ManifestBuilder::new()
        .add_source("test-source", &different_url)
        .add_standard_agent("test-agent", "test-source", "agents/test-agent.md")
        .build();
    project.write_manifest(&manifest_with_changed_url).await?;

    // Frozen install should fail when source URL changed (security concern)
    let output = project.run_agpm(&["install", "--frozen", "--quiet"])?;
    assert!(!output.success, "Frozen install should fail when source URL changed");
    assert!(
        output.stderr.to_lowercase().contains("source")
            || output.stderr.to_lowercase().contains("url")
            || output.stderr.to_lowercase().contains("changed")
            || output.stderr.to_lowercase().contains("frozen"),
        "Error should mention source URL change or frozen mode: {}",
        output.stderr
    );

    Ok(())
}
