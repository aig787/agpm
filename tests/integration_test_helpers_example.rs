//! Example test demonstrating the use of common test helpers
//! This shows how to use TestProject, TestSourceRepo, and assertion helpers

use anyhow::Result;

mod common;
use common::{DirAssert, FileAssert, TestProject};

/// Example test showing TestProject usage
#[test]
fn test_using_test_project_helper() -> Result<()> {
    ccpm::test_utils::init_test_logging(None);
    // Create a test project with all necessary directories
    let project = TestProject::new()?;

    // Write a manifest file
    project.write_manifest(
        r#"
[sources]
example = "https://github.com/example/repo.git"

[agents]
test-agent = { source = "example", path = "agents/test.md", version = "v1.0.0" }
"#,
    )?;

    // Create a local resource
    project.create_local_resource("agents/local-agent.md", "# Local Agent\n\nTest content")?;

    // Verify the project structure was created correctly
    DirAssert::exists(project.project_path());
    DirAssert::exists(project.cache_path());
    DirAssert::exists(project.sources_path());

    // Verify manifest was written
    FileAssert::exists(project.project_path().join("ccpm.toml"));
    FileAssert::contains(project.project_path().join("ccpm.toml"), "test-agent");

    // Verify local resource was created
    FileAssert::exists(project.project_path().join("agents/local-agent.md"));
    FileAssert::contains(
        project.project_path().join("agents/local-agent.md"),
        "# Local Agent",
    );

    // Run a CCPM command (validate in this case)
    let output = project.run_ccpm(&["validate"])?;
    output.assert_success();
    output.assert_stdout_contains("Valid ccpm.toml");

    Ok(())
}

/// Example test showing TestSourceRepo usage
#[test]
fn test_using_test_source_repo_helper() -> Result<()> {
    ccpm::test_utils::init_test_logging(None);
    let project = TestProject::new()?;

    // Create a source repository with standard resources
    let source_repo = project.create_source_repo("test-source")?;
    source_repo.create_standard_resources()?;
    source_repo.commit_all("Initial commit")?;
    source_repo.tag_version("v1.0.0")?;

    // Add custom resource
    source_repo.add_resource("agents", "custom", "# Custom Agent\nSpecial content")?;
    source_repo.commit_all("Add custom agent")?;
    source_repo.tag_version("v1.1.0")?;

    // Get the file URL for the bare repository (handles bare clone automatically)
    let repo_url = source_repo.bare_file_url(project.sources_path())?;

    // Create manifest using the test repository
    let manifest_content = format!(
        r#"
[sources]
test = "{}"

[agents]
standard = {{ source = "test", path = "agents/test-agent.md", version = "v1.0.0" }}
custom = {{ source = "test", path = "agents/custom.md", version = "v1.1.0" }}
"#,
        repo_url
    );

    project.write_manifest(&manifest_content)?;

    // Run install command
    let output = project.run_ccpm(&["install"])?;
    output.assert_success();

    // Verify resources were installed
    let agents_dir = project.project_path().join(".claude/agents");
    DirAssert::exists(&agents_dir);
    DirAssert::contains_file(&agents_dir, "standard.md");
    DirAssert::contains_file(&agents_dir, "custom.md");

    // Verify lockfile was created
    FileAssert::exists(project.project_path().join("ccpm.lock"));

    Ok(())
}

/// Example test showing FileAssert and DirAssert usage
#[test]
fn test_assertion_helpers() -> Result<()> {
    ccpm::test_utils::init_test_logging(None);
    let project = TestProject::new()?;

    // Create some test files and directories
    let test_file = project.project_path().join("test.txt");
    std::fs::write(&test_file, "Hello, World!")?;

    let test_dir = project.project_path().join("test-dir");
    std::fs::create_dir(&test_dir)?;
    std::fs::write(test_dir.join("nested.txt"), "Nested content")?;

    // Use FileAssert helpers
    FileAssert::exists(&test_file);
    FileAssert::contains(&test_file, "Hello");
    FileAssert::equals(&test_file, "Hello, World!");

    // Use DirAssert helpers
    DirAssert::exists(&test_dir);
    DirAssert::contains_file(&test_dir, "nested.txt");

    // Test negative assertions
    FileAssert::not_exists(project.project_path().join("nonexistent.txt"));

    // Create empty directory and verify it's empty
    let empty_dir = project.project_path().join("empty");
    std::fs::create_dir(&empty_dir)?;
    DirAssert::is_empty(&empty_dir);

    Ok(())
}
