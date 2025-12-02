//! Integration test for direct manifest dependencies overriding transitive ones.
//!
//! When a resource appears both as a direct dependency in the manifest (with custom
//! filename, template_vars) and as a transitive dependency of another resource, the
//! direct manifest version should win.

use anyhow::Result;

use crate::common::TestProject;

/// Test that direct manifest dependency overrides transitive dependency.
///
/// Setup:
/// - Parent agent depends transitively on helper agent
/// - Manifest also includes helper agent directly with custom filename
/// - Expected: Only the direct version with custom filename is installed
#[tokio::test]
async fn test_direct_manifest_overrides_transitive() -> Result<()> {
    let test_project = TestProject::new().await?;

    // Create a helper agent that will be both direct and transitive
    let helper_agent = r#"---
title: Helper Agent
---
# Helper Agent

This is a shared helper agent.
"#;

    // Create a parent agent that depends on the helper
    let parent_agent = r#"---
title: Parent Agent
dependencies:
  agents:
    - path: agents/helper.md
---
# Parent Agent

Uses the helper agent transitively.
"#;

    let test_repo = test_project.create_source_repo("test").await?;
    test_repo.add_resource("agents", "helper", helper_agent).await?;
    test_repo.add_resource("agents", "parent", parent_agent).await?;

    test_repo.commit_all("Add agents")?;
    test_repo.tag_version("v1.0.0")?;

    // Create manifest with BOTH:
    // 1. Parent (brings in helper transitively)
    // 2. Helper directly with custom filename
    let test_url = test_repo.bare_file_url(test_project.sources_path()).await?;
    let manifest = format!(
        r#"[sources]
test = "{}"

[agents]
# Parent brings in helper transitively
parent = {{ source = "test", path = "agents/parent.md", version = "v1.0.0" }}

# Direct dependency with custom filename should override transitive
helper-custom = {{ source = "test", path = "agents/helper.md", version = "v1.0.0", filename = "helper-custom.md" }}
"#,
        test_url
    );

    test_project.write_manifest(&manifest).await?;

    // Run install
    let output = test_project.run_agpm(&["install"])?;
    assert!(output.success, "Install should succeed. Stderr: {}", output.stderr);

    // Verify only the custom filename version exists
    let custom_path = test_project.project_path().join(".claude/agents/agpm/helper-custom.md");
    assert!(
        tokio::fs::metadata(&custom_path).await.is_ok(),
        "Custom filename version should exist at {:?}",
        custom_path
    );

    // Verify no duplicate with default name
    let default_path = test_project.project_path().join(".claude/agents/agpm/helper.md");
    assert!(
        tokio::fs::metadata(&default_path).await.is_err(),
        "Should not have duplicate with default name at {:?}",
        default_path
    );

    // Verify lockfile has only one entry
    let lockfile_content = test_project.read_lockfile().await?;

    // Count occurrences of the path - should only appear once
    let path_occurrences = lockfile_content.matches("agents/helper.md").count();
    assert_eq!(path_occurrences, 1, "Path should only appear once in lockfile");

    // Verify the entry has the custom filename
    assert!(lockfile_content.contains("helper-custom.md"), "Lockfile should use custom filename");

    Ok(())
}

/// Test that multiple direct dependencies with same path but different template_vars
/// are kept as separate resources.
#[tokio::test]
async fn test_multiple_variants_of_same_resource() -> Result<()> {
    let test_project = TestProject::new().await?;

    // Create a template agent
    let template_agent = r#"---
title: Language Agent
agpm:
  templating: true
---
# {{ project.language | capitalize }} Agent

Specialized for {{ project.language }}.
"#;

    let test_repo = test_project.create_source_repo("test").await?;
    test_repo.add_resource("agents", "language", template_agent).await?;

    test_repo.commit_all("Add template agent")?;
    test_repo.tag_version("v1.0.0")?;

    // Create manifest with multiple variants of the same resource
    let test_url = test_repo.bare_file_url(test_project.sources_path()).await?;
    let manifest = format!(
        r#"[sources]
test = "{}"

[agents]
lang-python = {{ source = "test", path = "agents/language.md", version = "v1.0.0", filename = "lang-python.md", template_vars = {{ project = {{ language = "python" }} }} }}
lang-rust = {{ source = "test", path = "agents/language.md", version = "v1.0.0", filename = "lang-rust.md", template_vars = {{ project = {{ language = "rust" }} }} }}
lang-typescript = {{ source = "test", path = "agents/language.md", version = "v1.0.0", filename = "lang-typescript.md", template_vars = {{ project = {{ language = "typescript" }} }} }}
"#,
        test_url
    );

    test_project.write_manifest(&manifest).await?;

    // Run install
    let output = test_project.run_agpm(&["install"])?;
    assert!(output.success, "Install should succeed. Stderr: {}", output.stderr);

    // Verify all three variants exist
    let python_path = test_project.project_path().join(".claude/agents/agpm/lang-python.md");
    let rust_path = test_project.project_path().join(".claude/agents/agpm/lang-rust.md");
    let typescript_path =
        test_project.project_path().join(".claude/agents/agpm/lang-typescript.md");

    assert!(
        tokio::fs::metadata(&python_path).await.is_ok(),
        "Python variant should exist at {:?}",
        python_path
    );
    assert!(
        tokio::fs::metadata(&rust_path).await.is_ok(),
        "Rust variant should exist at {:?}",
        rust_path
    );
    assert!(
        tokio::fs::metadata(&typescript_path).await.is_ok(),
        "TypeScript variant should exist at {:?}",
        typescript_path
    );

    // Verify content is different (rendered with different variables)
    let python_content = tokio::fs::read_to_string(&python_path).await?;
    let rust_content = tokio::fs::read_to_string(&rust_path).await?;
    let typescript_content = tokio::fs::read_to_string(&typescript_path).await?;

    assert!(python_content.contains("# Python Agent"), "Python agent should be rendered");
    assert!(rust_content.contains("# Rust Agent"), "Rust agent should be rendered");
    assert!(
        typescript_content.contains("# Typescript Agent"),
        "TypeScript agent should be rendered"
    );

    // Verify lockfile has three separate entries
    let lockfile_content = test_project.read_lockfile().await?;

    // Check each variant is in the lockfile (via manifest_alias)
    assert!(
        lockfile_content.contains(r#"manifest_alias = "lang-python""#),
        "Lockfile should contain lang-python variant"
    );
    assert!(
        lockfile_content.contains(r#"manifest_alias = "lang-rust""#),
        "Lockfile should contain lang-rust variant"
    );
    assert!(
        lockfile_content.contains(r#"manifest_alias = "lang-typescript""#),
        "Lockfile should contain lang-typescript variant"
    );

    Ok(())
}
