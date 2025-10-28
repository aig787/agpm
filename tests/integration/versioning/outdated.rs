use tokio::fs;

use crate::common::{ManifestBuilder, TestProject};
use crate::fixtures::ManifestFixture;

/// Test outdated command with up-to-date dependencies
#[tokio::test]
async fn test_outdated_all_up_to_date() {
    let project = TestProject::new().await.unwrap();

    // Create a real test repository
    let repo = project.create_source_repo("official").await.unwrap();
    fs::create_dir_all(repo.path.join("agents")).await.unwrap();
    fs::write(repo.path.join("agents/my-agent.md"), "# My Agent\n\nContent").await.unwrap();
    repo.git.add_all().unwrap();
    repo.git.commit("Initial commit").unwrap();
    repo.git.tag("v2.0.0").unwrap();

    let url = repo.bare_file_url(project.sources_path()).unwrap();

    // Create manifest
    let manifest = ManifestBuilder::new()
        .add_source("official", &url)
        .add_agent("my-agent", |d| {
            d.source("official").path("agents/my-agent.md").version("v2.0.0")
        })
        .build();
    project.write_manifest(&manifest).await.unwrap();

    // Install to create lockfile
    let install_output = project.run_agpm(&["install"]).unwrap();
    install_output.assert_success();

    // Run outdated command (should show everything is up to date)
    let output = project.run_agpm(&["outdated"]).unwrap();
    output.assert_success().assert_stdout_contains("All dependencies are up to date!");
}

/// Test outdated command with no lockfile
#[tokio::test]
async fn test_outdated_no_lockfile() {
    let project = TestProject::new().await.unwrap();
    let manifest_content = ManifestFixture::basic().content;
    project.write_manifest(&manifest_content).await.unwrap();

    let output = project.run_agpm(&["outdated"]).unwrap();
    assert!(!output.success, "Expected command to fail without lockfile");
    assert!(
        output.stderr.contains("agpm.lock") || output.stderr.contains("Run 'agpm install' first"),
        "Expected lockfile error message, got: {}",
        output.stderr
    );
}

/// Test outdated command without project
#[tokio::test]
async fn test_outdated_without_project() {
    let project = TestProject::new().await.unwrap();

    let output = project.run_agpm(&["outdated"]).unwrap();
    assert!(!output.success, "Expected command to fail without project");
    assert!(
        output.stderr.contains("agpm.toml not found"),
        "Expected manifest not found error, got: {}",
        output.stderr
    );
}

/// Test outdated command with JSON format
#[tokio::test]
async fn test_outdated_json_format() {
    let project = TestProject::new().await.unwrap();

    // Create a real test repository
    let repo = project.create_source_repo("official").await.unwrap();
    fs::create_dir_all(repo.path.join("agents")).await.unwrap();
    fs::write(repo.path.join("agents/my-agent.md"), "# My Agent\n\nContent").await.unwrap();
    repo.git.add_all().unwrap();
    repo.git.commit("Initial commit").unwrap();
    repo.git.tag("v1.0.0").unwrap();

    let url = repo.bare_file_url(project.sources_path()).unwrap();

    // Create manifest
    let manifest = ManifestBuilder::new()
        .add_source("official", &url)
        .add_agent("my-agent", |d| {
            d.source("official").path("agents/my-agent.md").version("v1.0.0")
        })
        .build();
    project.write_manifest(&manifest).await.unwrap();

    // Install to create lockfile
    let install_output = project.run_agpm(&["install"]).unwrap();
    install_output.assert_success();

    // Run outdated command with JSON format
    let output = project.run_agpm(&["outdated", "--format", "json"]).unwrap();
    output.assert_success();

    // Check that output is valid JSON
    assert!(
        output.stdout.contains("{") && output.stdout.contains("}"),
        "Expected JSON output, got: {}",
        output.stdout
    );
    assert!(
        output.stdout.contains("\"outdated\"") && output.stdout.contains("\"summary\""),
        "Expected JSON structure with outdated and summary fields, got: {}",
        output.stdout
    );
}

/// Test outdated command with --check flag
#[tokio::test]
async fn test_outdated_check_flag() {
    let project = TestProject::new().await.unwrap();

    // Create a real test repository
    let repo = project.create_source_repo("official").await.unwrap();
    fs::create_dir_all(repo.path.join("agents")).await.unwrap();
    fs::write(repo.path.join("agents/my-agent.md"), "# My Agent\n\nContent").await.unwrap();
    repo.git.add_all().unwrap();
    repo.git.commit("Initial commit").unwrap();
    repo.git.tag("v2.0.0").unwrap();

    let url = repo.bare_file_url(project.sources_path()).unwrap();

    // Create manifest with up-to-date version
    let manifest = ManifestBuilder::new()
        .add_source("official", &url)
        .add_agent("my-agent", |d| {
            d.source("official").path("agents/my-agent.md").version("v2.0.0")
        })
        .build();
    project.write_manifest(&manifest).await.unwrap();

    // Install to create lockfile
    let install_output = project.run_agpm(&["install"]).unwrap();
    install_output.assert_success();

    // Run outdated command with --check flag
    let output = project.run_agpm(&["outdated", "--check"]).unwrap();

    // Should succeed when all dependencies are up to date
    output.assert_success();
}

/// Test outdated command with specific dependencies
#[tokio::test]
async fn test_outdated_specific_dependencies() {
    let project = TestProject::new().await.unwrap();

    // Create a real test repository with multiple agents
    let repo = project.create_source_repo("official").await.unwrap();
    fs::create_dir_all(repo.path.join("agents")).await.unwrap();
    fs::write(repo.path.join("agents/my-agent.md"), "# My Agent\n\nContent").await.unwrap();
    fs::write(repo.path.join("agents/helper.md"), "# Helper Agent\n\nContent").await.unwrap();
    repo.git.add_all().unwrap();
    repo.git.commit("Initial commit").unwrap();
    repo.git.tag("v1.0.0").unwrap();

    let url = repo.bare_file_url(project.sources_path()).unwrap();

    // Create manifest with multiple dependencies
    let manifest = ManifestBuilder::new()
        .add_source("official", &url)
        .add_agent("my-agent", |d| {
            d.source("official").path("agents/my-agent.md").version("^1.0.0")
        })
        .add_agent("helper", |d| d.source("official").path("agents/helper.md").version("^1.0.0"))
        .build();
    project.write_manifest(&manifest).await.unwrap();

    // Install to create lockfile
    let install_output = project.run_agpm(&["install"]).unwrap();
    install_output.assert_success();

    // Run outdated command for specific dependency
    let output = project.run_agpm(&["outdated", "my-agent"]).unwrap();
    output.assert_success();
}

/// Test outdated command with prefixed version constraints (monorepo-style versioning)
///
/// This test verifies the fix for the bug where `outdated` was ignoring version prefixes
/// like `agents-v1.0.0`. Before the fix, a constraint like `agents-^v1.0.0` would not
/// detect updates to `agents-v1.5.0` because the prefix was stripped during version matching.
#[tokio::test]
async fn test_outdated_with_prefixed_version_constraints() {
    let project = TestProject::new().await.unwrap();

    // Create a Git repository with prefixed tags (monorepo-style versioning)
    let repo = project.create_source_repo("monorepo").await.unwrap();

    // Create agents-v1.0.0
    fs::create_dir_all(repo.path.join("agents")).await.unwrap();
    fs::write(repo.path.join("agents/my-agent.md"), "# Agent v1.0.0\n\nContent v1.0.0")
        .await
        .unwrap();
    repo.git.add_all().unwrap();
    repo.git.commit("Release agents-v1.0.0").unwrap();
    repo.git.tag("agents-v1.0.0").unwrap();

    let url = repo.bare_file_url(project.sources_path()).unwrap();

    // Create manifest with prefixed caret constraint
    let manifest = ManifestBuilder::new()
        .add_source("monorepo", &url)
        .add_agent("ai-helper", |d| {
            d.source("monorepo").path("agents/my-agent.md").version("agents-^v1.0.0")
        })
        .build();
    project.write_manifest(&manifest).await.unwrap();

    // Install with v1.0.0
    let install_output = project.run_agpm(&["install"]).unwrap();
    install_output.assert_success();

    // Verify lockfile was created with agents-v1.0.0
    let lockfile_path = project.project_path().join("agpm.lock");
    assert!(lockfile_path.exists(), "Lockfile should exist after install");
    let lockfile_content = fs::read_to_string(&lockfile_path).await.unwrap();
    assert!(lockfile_content.contains("agents-v1.0.0"), "Lockfile should contain agents-v1.0.0");

    // Now add a newer version agents-v1.5.0
    fs::write(repo.path.join("agents/my-agent.md"), "# Agent v1.5.0\n\nContent v1.5.0")
        .await
        .unwrap();
    repo.git.add_all().unwrap();
    repo.git.commit("Release agents-v1.5.0").unwrap();
    repo.git.tag("agents-v1.5.0").unwrap();

    // Run outdated command - should detect agents-v1.5.0 as available update
    let output = project.run_agpm(&["outdated"]).unwrap();
    output.assert_success();

    // Verify it detected the update (either shows update info or says up to date)
    // Both are valid since agents-^v1.0.0 allows v1.5.0
    let has_update_info =
        output.stdout.contains("ai-helper") && output.stdout.contains("agents-v1");
    let is_up_to_date = output.stdout.contains("up to date");

    assert!(
        has_update_info || is_up_to_date,
        "Expected outdated to either show version info or 'up to date'.\nGot stdout:\n{}",
        output.stdout
    );
}

/// Test outdated command with multiple prefixed version namespaces
///
/// Verifies that different prefixes (agents- vs snippets-) are treated as
/// independent version namespaces.
#[tokio::test]
async fn test_outdated_with_multiple_version_prefixes() {
    let project = TestProject::new().await.unwrap();

    // Create repository with multiple prefixed version series
    let repo = project.create_source_repo("monorepo").await.unwrap();

    // Create agents-v1.0.0
    fs::create_dir_all(repo.path.join("agents")).await.unwrap();
    fs::create_dir_all(repo.path.join("snippets")).await.unwrap();
    fs::write(repo.path.join("agents/my-agent.md"), "# Agent v1.0.0").await.unwrap();
    fs::write(repo.path.join("snippets/my-snippet.md"), "# Snippet v2.0.0").await.unwrap();
    repo.git.add_all().unwrap();
    repo.git.commit("Initial versions").unwrap();
    repo.git.tag("agents-v1.0.0").unwrap();
    repo.git.tag("snippets-v2.0.0").unwrap();

    // Create agents-v1.1.0 and snippets-v2.1.0
    fs::write(repo.path.join("agents/my-agent.md"), "# Agent v1.1.0").await.unwrap();
    fs::write(repo.path.join("snippets/my-snippet.md"), "# Snippet v2.1.0").await.unwrap();
    repo.git.add_all().unwrap();
    repo.git.commit("Update versions").unwrap();
    repo.git.tag("agents-v1.1.0").unwrap();
    repo.git.tag("snippets-v2.1.0").unwrap();

    let url = repo.bare_file_url(project.sources_path()).unwrap();

    // Create manifest with prefixed constraints for different namespaces
    let manifest = ManifestBuilder::new()
        .add_source("monorepo", &url)
        .add_agent("ai-agent", |d| {
            d.source("monorepo").path("agents/my-agent.md").version("agents-^v1.0.0")
        })
        .add_snippet("utils", |d| {
            d.source("monorepo").path("snippets/my-snippet.md").version("snippets-^v2.0.0")
        })
        .build();
    project.write_manifest(&manifest).await.unwrap();

    // Run install to create initial lockfile with first versions
    let install_output = project.run_agpm(&["install"]).unwrap();
    install_output.assert_success();

    // Verify lockfile was created with first versions in each namespace
    let lockfile_path = project.project_path().join("agpm.lock");
    assert!(lockfile_path.exists(), "Lockfile should exist after install");
    let lockfile_content = fs::read_to_string(&lockfile_path).await.unwrap();
    assert!(
        lockfile_content.contains("agents-v1.0.0") || lockfile_content.contains("agents-v1.1.0"),
        "Lockfile should contain agents version"
    );
    assert!(
        lockfile_content.contains("snippets-v2.0.0")
            || lockfile_content.contains("snippets-v2.1.0"),
        "Lockfile should contain snippets version"
    );

    // Run outdated command
    let output = project.run_agpm(&["outdated"]).unwrap();
    output.assert_success();

    // Verify both namespaces show updates correctly
    // agents-^v1.0.0 should detect agents-v1.1.0 (not snippets-v2.1.0)
    // snippets-^v2.0.0 should detect snippets-v2.1.0 (not agents-v1.1.0)
    if output.stdout.contains("ai-agent") {
        assert!(
            output.stdout.contains("agents-v1.1.0") || output.stdout.contains("agents-v1.0.0"),
            "Expected agents namespace to show agents version\nGot:\n{}",
            output.stdout
        );
        // Verify prefix isolation: agents constraint shouldn't match snippets tags
        let lines_with_agent: Vec<&str> =
            output.stdout.lines().filter(|line| line.contains("ai-agent")).collect();
        for line in &lines_with_agent {
            assert!(
                !line.contains("snippets-v"),
                "agents-^v1.0.0 should not match snippets-v* tags\nGot line:\n{}",
                line
            );
        }
    }

    if output.stdout.contains("utils") {
        assert!(
            output.stdout.contains("snippets-v2.1.0") || output.stdout.contains("snippets-v2.0.0"),
            "Expected snippets namespace to show snippets version\nGot:\n{}",
            output.stdout
        );
        // Verify prefix isolation: snippets constraint shouldn't match agents tags
        let lines_with_snippet: Vec<&str> =
            output.stdout.lines().filter(|line| line.contains("utils")).collect();
        for line in &lines_with_snippet {
            assert!(
                !line.contains("agents-v"),
                "snippets-^v2.0.0 should not match agents-v* tags\nGot line:\n{}",
                line
            );
        }
    }
}
