/// Test to reproduce the transitive dependency conflict with version = "main"
///
/// This test demonstrates the issue where:
/// 1. Direct dependency A uses version = "main"
/// 2. A has transitive dependency on C (no version specified, inherits "main")
/// 3. Direct dependency B uses version = "v1.0.0"
/// 4. B has transitive dependency on C with version = "v1.0.0"
/// 5. Conflict: C is requested at both "main" and "v1.0.0"
use crate::common::{ManifestBuilder, TestProject};
use anyhow::Result;

/// Test that demonstrates version = "main" causing transitive conflicts
#[tokio::test]
async fn test_version_main_causes_transitive_conflict() -> Result<()> {
    let project = TestProject::new().await?;
    let repo = project.create_source_repo("test-repo").await?;

    // Create shared resource C (no dependencies)
    repo.add_resource(
        "agents",
        "resource-c",
        r#"---
name: Resource C
---
# Resource C
Shared dependency.
"#,
    )
    .await?;

    // Create resource A that depends on C (no version specified in dependency)
    repo.add_resource(
        "agents",
        "resource-a",
        r#"---
name: Resource A
dependencies:
  agents:
    - path: ./resource-c.md
---
# Resource A
Depends on C (version inherited from parent).
"#,
    )
    .await?;

    // Create resource B that depends on C with explicit v1.0.0
    repo.add_resource(
        "agents",
        "resource-b",
        r#"---
name: Resource B
dependencies:
  agents:
    - path: ./resource-c.md
      version: v1.0.0
---
# Resource B
Depends on C at v1.0.0.
"#,
    )
    .await?;

    repo.commit_all("Add resources A, B, and C")?;
    repo.tag_version("v1.0.0")?;

    // Update resource C on main (so main != v1.0.0)
    repo.add_resource(
        "agents",
        "resource-c",
        r#"---
name: Resource C
---
# Resource C - UPDATED
This is newer content on main.
"#,
    )
    .await?;

    repo.commit_all("Update C on main")?;

    let repo_url = repo.bare_file_url(project.sources_path()).await?;

    // Create manifest with:
    // - A at "main" (will inherit "main" to C)
    // - B at "v1.0.0" (will request C at v1.0.0)
    let manifest = ManifestBuilder::new()
        .add_source("test-repo", &repo_url)
        .add_agent("resource-a", |d| {
            d.source("test-repo").path("agents/resource-a.md").version("main")
        })
        .add_agent("resource-b", |d| {
            d.source("test-repo").path("agents/resource-b.md").version("v1.0.0")
        })
        .build();

    project.write_manifest(&manifest).await?;

    // Run install
    let output = project.run_agpm(&["install"])?;

    // Check if it succeeded or failed
    if !output.success {
        // Installation failed - check if it's due to the expected conflict
        let combined = format!("{}{}", output.stdout, output.stderr);
        println!("Installation failed as expected. Output: {}", combined);
        assert!(
            combined.contains("conflict") || combined.contains("version"),
            "Error should mention version conflict. Output: {}",
            combined
        );
    } else {
        // Installation succeeded - let's see what actually happened
        println!("Installation succeeded. Let's check the lockfile...");
        let lockfile = project.load_lockfile()?;

        // Find resource C in lockfile
        let resource_c_entries: Vec<_> =
            lockfile.agents.iter().filter(|a| a.name == "agents/resource-c").collect();

        println!("Found {} entries for resource-c in lockfile", resource_c_entries.len());
        for (i, entry) in resource_c_entries.iter().enumerate() {
            println!(
                "  Entry {}: version={:?}, SHA={:?}, manifest_alias={:?}",
                i, entry.version, entry.resolved_commit, entry.manifest_alias
            );
        }

        // The question: Did AGPM deduplicate properly or create multiple entries?
        // If it created 1 entry, which version did it choose?
        assert_eq!(
            resource_c_entries.len(),
            1,
            "Should have exactly 1 entry for resource-c (deduplicated), but found {}",
            resource_c_entries.len()
        );

        // Which version was chosen?
        let chosen_version = &resource_c_entries[0].version;
        println!("Chosen version for resource-c: {:?}", chosen_version);

        // Check the content to see if it's the old v1.0.0 or the updated main
        let c_path = project.project_path().join(".claude/agents/agpm/resource-c.md");
        let c_content = tokio::fs::read_to_string(&c_path).await?;

        println!("Content of resource-c:\n{}", c_content);

        // The BUG: User specified version="main" for A, expecting latest.
        // But AGPM backtracked and chose v1.0.0 to resolve the conflict.
        // This means the user did NOT get the latest from main!
        if c_content.contains("UPDATED") {
            println!("✓ Got latest from main (UPDATED content)");
            panic!("Test setup error: backtracking should have chosen v1.0.0, not main");
        } else {
            println!("✗ Got old v1.0.0 content, NOT latest from main!");
            println!(
                "This is the bug: version='main' didn't get latest because of conflict resolution"
            );

            // This demonstrates the problem: when you use version="main",
            // you expect to get the latest, but backtracking might choose an older version
            // to resolve conflicts.
        }
    }

    Ok(())
}

/// Test that branch = "main" currently behaves the same way (also buggy)
#[tokio::test]
async fn test_branch_main_current_behavior() -> Result<()> {
    let project = TestProject::new().await?;
    let repo = project.create_source_repo("test-repo").await?;

    // Same setup as above
    repo.add_resource(
        "agents",
        "resource-c",
        r#"---
name: Resource C
---
# Resource C
"#,
    )
    .await?;

    repo.commit_all("Add resource C")?;
    repo.tag_version("v1.0.0")?;

    repo.add_resource(
        "agents",
        "resource-a",
        r#"---
dependencies:
  agents:
    - path: ./resource-c.md
---
# Resource A
"#,
    )
    .await?;

    repo.add_resource(
        "agents",
        "resource-b",
        r#"---
dependencies:
  agents:
    - path: ./resource-c.md
      version: v1.0.0
---
# Resource B
"#,
    )
    .await?;

    repo.commit_all("Add A and B")?;

    repo.add_resource(
        "agents",
        "resource-c",
        r#"---
name: Resource C
---
# Resource C - UPDATED
"#,
    )
    .await?;

    repo.commit_all("Update C on main")?;

    let repo_url = repo.bare_file_url(project.sources_path()).await?;

    // Use branch = "main" instead of version = "main"
    // This should NOT inherit "main" to transitive deps
    // Instead, transitive deps should inherit the resolved SHA
    let manifest = format!(
        r#"[sources]
test-repo = "{}"

[agents]
resource-a = {{ source = "test-repo", path = "agents/resource-a.md", branch = "main" }}
resource-b = {{ source = "test-repo", path = "agents/resource-b.md", version = "v1.0.0" }}
"#,
        repo_url
    );

    project.write_manifest(&manifest).await?;

    // Let's see what actually happens with branch = "main"
    let output = project.run_agpm(&["install"])?;

    if !output.success {
        println!("Installation FAILED with branch = main");
        println!("stderr: {}", output.stderr);
        println!("stdout: {}", output.stdout);
    } else {
        println!("Installation SUCCEEDED with branch = main");
        let lockfile = project.load_lockfile()?;

        let resource_c_entries: Vec<_> =
            lockfile.agents.iter().filter(|a| a.name == "agents/resource-c").collect();

        println!("Found {} entries for resource-c", resource_c_entries.len());
        for (i, entry) in resource_c_entries.iter().enumerate() {
            println!("  Entry {}: version={:?}, SHA={:?}", i, entry.version, entry.resolved_commit);
        }

        if let Some(_entry) = resource_c_entries.first() {
            let c_path = project.project_path().join(".claude/agents/agpm/resource-c.md");
            let c_content = tokio::fs::read_to_string(&c_path).await?;

            if c_content.contains("UPDATED") {
                println!("✓ Got latest from main (UPDATED content)");
                println!("Branch field worked correctly!");
            } else {
                println!("✗ Got old v1.0.0 content");
                println!("Branch field has the SAME bug as version field!");
            }
        }
    }

    Ok(())
}
