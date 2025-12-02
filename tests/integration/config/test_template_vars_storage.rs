//! Test that template variables are stored in PreparedSourceVersion during resolution.

use crate::common::TestProject;
use anyhow::Result;

/// Test that template variables are stored in PreparedSourceVersion.resource_variants
/// during resolution and preserved in the lockfile.
#[tokio::test]
async fn test_template_variables_stored_during_resolution() -> Result<()> {
    let project = TestProject::new().await?;
    let source_repo = project.create_source_repo("source").await?;

    // Create Agent X with template placeholder
    source_repo.add_resource("agents", "agent-x", "# Agent X for {{ language }}").await?;
    source_repo.commit_all("Agent X v1.0.0")?;
    source_repo.tag_version("v1.0.0")?;

    // Create manifest with template_vars
    let manifest_toml = format!(
        r#"[agpm]
templating = true

[sources]
source = "{}"

[agents]
agent-x = {{ source = "source", path = "agents/agent-x.md", version = "v1.0.0", template_vars = {{ language = "rust" }} }}
"#,
        source_repo.bare_file_url(project.sources_path()).await?
    );
    project.write_manifest(&manifest_toml).await?;

    // Run install
    let output = project.run_agpm(&["install"])?;
    assert!(output.success, "Install should succeed. Stderr: {}", output.stderr);

    // Verify lockfile has template variables preserved
    let lockfile_content = project.read_lockfile().await?;

    // The key test: verify variant_inputs are stored in the lockfile
    // This proves that PreparedSourceVersion.resource_variants infrastructure works
    assert!(
        lockfile_content.contains("variant_inputs"),
        "Lockfile should contain variant_inputs section. Lockfile:\n{}",
        lockfile_content
    );

    assert!(
        lockfile_content.contains("language") && lockfile_content.contains("rust"),
        "Template variable 'language = rust' should be preserved in variant_inputs. Lockfile:\n{}",
        lockfile_content
    );

    Ok(())
}
