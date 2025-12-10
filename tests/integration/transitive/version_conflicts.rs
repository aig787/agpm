use crate::common::{ManifestBuilder, TestProject};
use anyhow::Result;

/// Regression test for version conflict false positives with pattern + transitive deps.
///
/// Bug scenario (GitHub issue): When combining:
/// 1. Pattern dependency `commands/*.md` with version `commands-^v1.0.0`
/// 2. Commands that declare transitive deps on a snippet with `directives-^v1.0.0`
/// 3. Direct manifest dependency on that snippet with `directives-^v1.0.0`
///
/// BEFORE FIX: Error "Version conflict detected for 'snippets/directives/ai-attribution'"
/// because pattern expansion stored resolved SHA instead of version constraint.
/// The transitive deps showed raw SHA while manifest dep showed the constraint.
///
/// AFTER FIX: Install succeeds - pattern expansion preserves original constraint.
#[tokio::test]
async fn test_pattern_transitive_prefixed_version_no_false_conflict() -> Result<()> {
    let project = TestProject::new().await?;
    let repo = project.create_source_repo("tooling").await?;

    // Add snippet at directives-v1.0.0
    repo.add_resource(
        "snippets/directives",
        "ai-attribution",
        "---\nname: AI Attribution\n---\n# AI Attribution\n",
    )
    .await?;
    repo.commit_all("Add snippet")?;
    repo.tag_version("directives-v1.0.0")?;

    // Add commands that have transitive deps on the snippet WITH explicit version.
    // The bug: When pattern expansion stored SHA instead of constraint, the conflict
    // detector saw the transitive dep's version as the parent's SHA, not this constraint.
    repo.add_resource(
        "commands",
        "commit",
        r#"---
name: Commit
dependencies:
  snippets:
    - path: snippets/directives/ai-attribution.md
      version: directives-^v1.0.0
---
# Commit
"#,
    )
    .await?;

    repo.add_resource(
        "commands",
        "squash",
        r#"---
name: Squash
dependencies:
  snippets:
    - path: snippets/directives/ai-attribution.md
      version: directives-^v1.0.0
---
# Squash
"#,
    )
    .await?;

    repo.commit_all("Add commands")?;
    repo.tag_version("commands-v1.0.0")?;

    let repo_url = repo.bare_file_url(project.sources_path()).await?;

    // Manifest: pattern dep on commands + direct dep on snippet
    let manifest = ManifestBuilder::new()
        .add_source("tooling", &repo_url)
        .add_command("utils", |d| {
            d.source("tooling").path("commands/*.md").version("commands-^v1.0.0")
        })
        .add_snippet("ai-attribution", |d| {
            d.source("tooling")
                .path("snippets/directives/ai-attribution.md")
                .version("directives-^v1.0.0")
        })
        .build();

    project.write_manifest(&manifest).await?;

    // KEY ASSERTION: Before fix, this failed with "Version conflict detected"
    let output = project.run_agpm(&["install"])?;

    assert!(
        output.success,
        "Install should succeed. BEFORE FIX: 'Version conflict detected for ai-attribution'\nstderr: {}\nstdout: {}",
        output.stderr, output.stdout
    );

    assert!(
        !output.stderr.contains("Version conflict"),
        "Should NOT report version conflict. stderr: {}",
        output.stderr
    );

    // Verify Fix 1: Commands should have the version constraint, not SHA
    let lockfile = project.load_lockfile()?;
    for cmd in &lockfile.commands {
        let version = cmd.version.as_ref().expect("command should have version");
        assert!(
            !version.chars().all(|c| c.is_ascii_hexdigit()),
            "Command '{}' version should be constraint 'commands-v1.0.0', not SHA '{}'. Fix 1 not working.",
            cmd.name,
            version
        );
    }

    // Verify Fix 2: The manifest dep should be present in the lockfile.
    // Note: There may be multiple entries if transitive deps use different tools,
    // but the manifest dep (with manifest_alias) should always be present.
    let manifest_snippet =
        lockfile.snippets.iter().find(|s| s.manifest_alias == Some("ai-attribution".to_string()));
    assert!(
        manifest_snippet.is_some(),
        "Should have manifest dep snippet with alias 'ai-attribution'. Got: {:?}",
        lockfile
            .snippets
            .iter()
            .map(|s| (&s.name, &s.manifest_alias, &s.version))
            .collect::<Vec<_>>()
    );

    // The manifest dep should have the correct version constraint
    let snippet = manifest_snippet.unwrap();
    assert_eq!(
        snippet.version,
        Some("directives-v1.0.0".to_string()),
        "Manifest snippet should have version constraint, not SHA"
    );

    Ok(())
}

/// Tests that explicit version specifications in transitive dependencies prevent conflicts
/// when multiple dependency chains (including direct dependencies) all specify the same version.
///
/// Dependency Graph (all from same repository):
/// ```
/// Manifest
/// ├── A (v1.0.0) → B (v1.0.0) → C (v1.0.0)
/// ├── D (v1.0.0) → C (v1.0.0)
/// └── C (v1.0.0)  # Direct dependency
/// ```
///
/// Three paths to C, all requiring v1.0.0:
/// 1. Manifest → A → B → C (transitive)
/// 2. Manifest → D → C (transitive)
/// 3. Manifest → C (direct)
#[tokio::test]
async fn test_explicit_version_specs_prevent_conflicts() -> Result<()> {
    let project = TestProject::new().await?;

    // Create repository with all resources at v1.0.0
    let repo = project.create_source_repo("community").await?;

    // Add resource C (the shared transitive dependency)
    repo.add_resource(
        "agents",
        "resource-c",
        r#"---
name: Resource C
---
# Resource C
This is resource C at v1.0.0.
"#,
    )
    .await?;

    // Add resource B that depends on C
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
This is resource B at v1.0.0, depending on C at v1.0.0.
"#,
    )
    .await?;

    // Add resource D that also depends on C
    repo.add_resource(
        "agents",
        "resource-d",
        r#"---
name: Resource D
dependencies:
  agents:
    - path: ./resource-c.md
      version: v1.0.0
---
# Resource D
This is resource D at v1.0.0, depending on C at v1.0.0.
"#,
    )
    .await?;

    // Add resource A that depends on B (which transitively depends on C)
    repo.add_resource(
        "agents",
        "resource-a",
        r#"---
name: Resource A
dependencies:
  agents:
    - path: ./resource-b.md
      version: v1.0.0
---
# Resource A
This is resource A at v1.0.0, depending on B at v1.0.0.
"#,
    )
    .await?;

    repo.commit_all("Add all resources")?;
    repo.tag_version("v1.0.0")?;

    // Create manifest with direct dependencies on A, D, and C
    let repo_url = repo.bare_file_url(project.sources_path()).await?;

    let manifest = ManifestBuilder::new()
        .add_source("community", &repo_url)
        .add_standard_agent("resource-a", "community", "agents/resource-a.md")
        .add_standard_agent("resource-d", "community", "agents/resource-d.md")
        .add_standard_agent("resource-c", "community", "agents/resource-c.md")
        .build();

    project.write_manifest(&manifest).await?;

    // Run install - should succeed with no conflicts
    let output = project.run_agpm(&["install"])?;

    assert!(
        output.success,
        "Install should succeed with no conflicts. stderr: {}\nstdout: {}",
        output.stderr, output.stdout
    );

    // Verify the lockfile
    let lockfile = project.load_lockfile()?;

    // Should have exactly 4 agent entries: A, B, C, D
    assert_eq!(lockfile.agents.len(), 4, "Should have exactly 4 agents in lockfile");

    // Find each resource in the lockfile
    let resource_a = lockfile
        .agents
        .iter()
        .find(|a| a.name == "agents/resource-a")
        .expect("Resource A should be in lockfile");

    let resource_b = lockfile
        .agents
        .iter()
        .find(|a| a.name == "agents/resource-b")
        .expect("Resource B should be in lockfile");

    let resource_c = lockfile
        .agents
        .iter()
        .find(|a| a.name == "agents/resource-c")
        .expect("Resource C should be in lockfile");

    let resource_d = lockfile
        .agents
        .iter()
        .find(|a| a.name == "agents/resource-d")
        .expect("Resource D should be in lockfile");

    // Verify versions
    assert_eq!(resource_a.version, Some("v1.0.0".to_string()));
    assert_eq!(resource_b.version, Some("v1.0.0".to_string()));
    assert_eq!(resource_c.version, Some("v1.0.0".to_string()));
    assert_eq!(resource_d.version, Some("v1.0.0".to_string()));

    // Verify resource C has manifest_alias (direct dependency wins)
    assert_eq!(
        resource_c.manifest_alias,
        Some("resource-c".to_string()),
        "Resource C should have manifest_alias since it's a direct dependency"
    );

    // Verify resource B is transitive (no manifest_alias)
    assert_eq!(
        resource_b.manifest_alias, None,
        "Resource B should not have manifest_alias since it's transitive"
    );

    // Verify all resources resolved to commits from the same repository
    assert_eq!(
        resource_a.source,
        Some("community".to_string()),
        "Resource A should be from community"
    );
    assert_eq!(
        resource_b.source,
        Some("community".to_string()),
        "Resource B should be from community"
    );
    assert_eq!(
        resource_c.source,
        Some("community".to_string()),
        "Resource C should be from community"
    );
    assert_eq!(
        resource_d.source,
        Some("community".to_string()),
        "Resource D should be from community"
    );

    // Verify that all dependencies point to the same commit (v1.0.0)
    // All resources should have the same resolved_commit since they're all from v1.0.0
    assert!(
        resource_c.resolved_commit.as_ref().map(|c| !c.is_empty()).unwrap_or(false),
        "Resource C should have a resolved commit"
    );

    // All resources should have the same SHA since they're from the same tag
    assert_eq!(
        resource_a.resolved_commit, resource_b.resolved_commit,
        "Resources A and B should have same commit"
    );
    assert_eq!(
        resource_b.resolved_commit, resource_c.resolved_commit,
        "Resources B and C should have same commit"
    );
    assert_eq!(
        resource_c.resolved_commit, resource_d.resolved_commit,
        "Resources C and D should have same commit"
    );

    // Verify there's exactly one entry for C (no duplicates)
    let c_count = lockfile.agents.iter().filter(|a| a.name == "agents/resource-c").count();
    assert_eq!(c_count, 1, "Should have exactly one entry for resource C");

    Ok(())
}
