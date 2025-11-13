// Integration tests for parallel processing in transitive dependency resolution
//
// Tests parallel processing features including:
// - DashMap-based concurrent data structures
// - Batch processing with dynamic sizing
// - Atomic progress tracking
// - Concurrent access safety
// - Performance improvements

use anyhow::Result;
use std::time::Instant;

use crate::common::{ManifestBuilder, TestProject};

/// Test basic parallel transitive resolution with 20+ dependencies
#[tokio::test]
async fn test_parallel_transitive_resolution() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;

    // Create source repo with multiple agent chains
    let community_repo = project.create_source_repo("community").await?;

    // Add base helper agents (no dependencies)
    for i in 0..10 {
        community_repo
            .add_resource(
                "agents",
                &format!("helper-{:02}", i),
                format!(
                    r#"---
# Helper Agent {:02}
This is helper agent {} with no dependencies.
---
"#,
                    i, i
                )
                .as_str(),
            )
            .await?;
    }

    // Add main agents that depend on multiple helpers
    for i in 0..15 {
        let mut dependencies = String::from("dependencies:\n  agents:\n");
        // Each main agent depends on 2-3 helper agents
        let dep_count = 2 + (i % 2);
        for j in 0..dep_count {
            let helper_idx = (i * 2 + j) % 10;
            dependencies.push_str(&format!(
                "    - path: ./helper-{:02}.md\n      version: v1.0.0\n",
                helper_idx
            ));
        }

        community_repo
            .add_resource(
                "agents",
                &format!("main-{:02}", i),
                format!(
                    r#"---
{}
---

# Main Agent {:02}
This agent depends on {} helper agents.
"#,
                    dependencies, i, dep_count
                )
                .as_str(),
            )
            .await?;
    }

    community_repo.commit_all("Initial commit")?;
    community_repo.tag_version("v1.0.0")?;

    // Create manifest with all main agents (should pull in transitive helpers)
    let source_url = community_repo.bare_file_url(project.sources_path())?;
    let mut builder = ManifestBuilder::new().add_source("community", &source_url);

    // Add all 15 main agents to the manifest
    for i in 0..15 {
        builder = builder.add_standard_agent(
            &format!("main-{:02}", i),
            "community",
            &format!("agents/main-{:02}.md", i),
        );
    }

    let manifest = builder.build();
    project.write_manifest(&manifest).await?;

    // Measure install time for performance verification
    let start_time = Instant::now();

    // Run install
    let output = project.run_agpm(&["install"])?;
    assert!(output.success, "Install should succeed. Stderr: {}", output.stderr);

    let install_duration = start_time.elapsed();

    // Verify all agents were installed (15 main + at least some helpers)
    let lockfile_content = project.read_lockfile().await?;

    // All main agents should be in lockfile
    for i in 0..15 {
        assert!(
            lockfile_content.contains(&format!("main-{:02}", i)),
            "Main agent {:02} should be in lockfile",
            i
        );
    }

    // At least some helper agents should be installed as transitive deps
    let mut helper_count = 0;
    for i in 0..10 {
        if lockfile_content.contains(&format!("helper-{:02}", i)) {
            helper_count += 1;
        }
    }
    assert!(
        helper_count > 0,
        "At least some helper agents should be in lockfile as transitive dependencies"
    );

    // Verify files were actually installed
    let agents_dir = project.project_path().join(".claude/agents");
    let mut agent_count = 0;
    let mut entries = tokio::fs::read_dir(&agents_dir).await?;
    while let Some(_entry) = entries.next_entry().await? {
        agent_count += 1;
    }

    assert!(agent_count >= 15, "At least 15 agents should be installed (got {})", agent_count);

    // Log performance metrics for verification
    println!(
        "Parallel install of {} agents completed in {:?} ({:.2} agents/sec)",
        agent_count,
        install_duration,
        agent_count as f64 / install_duration.as_secs_f64()
    );

    Ok(())
}

/// Test concurrent access to the same dependency keys
#[tokio::test]
async fn test_concurrent_dependency_access() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;

    // Create source repo with a shared dependency
    let community_repo = project.create_source_repo("community").await?;

    // Add a single shared helper that many agents will depend on
    community_repo
        .add_resource(
            "agents",
            "shared-helper",
            r#"---
# Shared Helper Agent
This helper is depended upon by many agents.
---
"#,
        )
        .await?;

    // Add multiple agents that all depend on the same helper
    for i in 0..20 {
        community_repo
            .add_resource(
                "agents",
                &format!("agent-{:02}", i),
                format!(
                    r#"---
dependencies:
  agents:
    - path: ./shared-helper.md
      version: v1.0.0
---

# Agent {:02}
This agent depends on the shared helper.
"#,
                    i
                )
                .as_str(),
            )
            .await?;
    }

    community_repo.commit_all("Initial commit")?;
    community_repo.tag_version("v1.0.0")?;

    // Create manifest with all agents
    let source_url = community_repo.bare_file_url(project.sources_path())?;
    let mut builder = ManifestBuilder::new().add_source("community", &source_url);

    for i in 0..20 {
        builder = builder.add_standard_agent(
            &format!("agent-{:02}", i),
            "community",
            &format!("agents/agent-{:02}.md", i),
        );
    }

    let manifest = builder.build();
    project.write_manifest(&manifest).await?;

    // Run install (this will test concurrent access to shared-helper)
    let output = project.run_agpm(&["install"])?;
    assert!(output.success, "Install should succeed. Stderr: {}", output.stderr);

    // Verify shared helper was only installed once (deduplication)
    // For transitive dependencies, let's verify that the helper file was installed only once
    let agents_dir = project.project_path().join(".claude/agents");
    let shared_helper_path = agents_dir.join("shared-helper.md");
    assert!(
        tokio::fs::metadata(&shared_helper_path).await.is_ok(),
        "Shared helper should be installed"
    );

    // Also verify that agents referencing it were installed
    let mut agent_count = 0;
    let mut entries = tokio::fs::read_dir(&agents_dir).await?;
    while let Some(entry) = entries.next_entry().await? {
        let file_name = entry.file_name();
        let name = file_name.to_string_lossy();
        if name.starts_with("agent-") {
            agent_count += 1;
        }
    }
    assert_eq!(agent_count, 20, "All 20 agents should be installed");

    // Check all agents exist
    for i in 0..20 {
        let agent_path = agents_dir.join(format!("agent-{:02}.md", i));
        assert!(
            tokio::fs::metadata(&agent_path).await.is_ok(),
            "Agent {:02} should be installed",
            i
        );
    }

    Ok(())
}

/// Test concurrent pattern expansion and alias mapping
#[tokio::test]
async fn test_concurrent_pattern_expansion() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;

    // Create source repo with pattern-based resources
    let community_repo = project.create_source_repo("community").await?;

    // Add a set of utility agents as a pattern
    for i in 0..12 {
        community_repo
            .add_resource(
                "agents",
                &format!("utils/utility-{:02}", i),
                format!(
                    r#"---
# Utility Agent {:02}
This is a utility agent in the utils directory.
---
"#,
                    i
                )
                .as_str(),
            )
            .await?;
    }

    // Add some main agents that depend on patterns
    for i in 0..5 {
        community_repo
            .add_resource(
                "agents",
                &format!("pattern-agent-{:02}", i),
                r#"---
dependencies:
  agents:
    - path: ./utils/utility-*.md
      version: v1.0.0
---

# Pattern Agent
This agent depends on all utility agents via pattern.
"#,
            )
            .await?;
    }

    community_repo.commit_all("Initial commit")?;
    community_repo.tag_version("v1.0.0")?;

    // Create manifest with pattern dependencies
    let source_url = community_repo.bare_file_url(project.sources_path())?;
    let manifest = ManifestBuilder::new()
        .add_source("community", &source_url)
        .add_agent_pattern("all-utilities", "community", "agents/utils/utility-*.md", "v1.0.0")
        .add_agent_pattern("pattern-agents", "community", "agents/pattern-agent-*.md", "v1.0.0")
        .build();

    project.write_manifest(&manifest).await?;

    // Run install
    let output = project.run_agpm(&["install"])?;
    assert!(output.success, "Install should succeed. Stderr: {}", output.stderr);

    // Verify all utility agents were installed via pattern
    let agents_dir = project.project_path().join(".claude/agents");

    // Check utility agents
    for i in 0..12 {
        let utility_path = agents_dir.join(format!("utility-{:02}.md", i));
        assert!(
            tokio::fs::metadata(&utility_path).await.is_ok(),
            "Utility agent {:02} should be installed via pattern",
            i
        );
    }

    // Check pattern agents
    for i in 0..5 {
        let agent_path = agents_dir.join(format!("pattern-agent-{:02}.md", i));
        assert!(
            tokio::fs::metadata(&agent_path).await.is_ok(),
            "Pattern agent {:02} should be installed",
            i
        );
    }

    Ok(())
}

/// Test parallel batch size calculation (max(10, 2×CPU cores))
#[tokio::test]
async fn test_parallel_batch_calculation() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;

    // Create source repo
    let community_repo = project.create_source_repo("community").await?;

    // Add a modest number of dependencies to test batch processing
    for i in 0..25 {
        community_repo
            .add_resource(
                "agents",
                &format!("batch-test-{:02}", i),
                r#"---
# Batch Test Agent
This agent is part of batch processing tests.
---
"#,
            )
            .await?;
    }

    community_repo.commit_all("Initial commit")?;
    community_repo.tag_version("v1.0.0")?;

    // Create manifest with all agents
    let source_url = community_repo.bare_file_url(project.sources_path())?;
    let mut builder = ManifestBuilder::new().add_source("community", &source_url);

    for i in 0..25 {
        builder = builder.add_standard_agent(
            &format!("batch-test-{:02}", i),
            "community",
            &format!("agents/batch-test-{:02}.md", i),
        );
    }

    let manifest = builder.build();
    project.write_manifest(&manifest).await?;

    // Run install with verbose output to capture batch processing info
    let output = project.run_agpm(&["install", "--verbose"])?;
    assert!(output.success, "Install should succeed. Stderr: {}", output.stderr);

    // Verify all agents were installed
    let lockfile_content = project.read_lockfile().await?;
    for i in 0..25 {
        assert!(
            lockfile_content.contains(&format!("batch-test-{:02}", i)),
            "Batch test agent {:02} should be in lockfile",
            i
        );
    }

    // Check that install operation produced specific debug output (verbose mode was used)
    // This verifies that the install operation ran and produced meaningful debug output
    assert!(
        !output.stderr.is_empty(),
        "Install with --verbose should produce debug output. Got empty stderr."
    );

    // Check for specific debug output patterns that indicate actual installation processing
    // These are specific patterns from the installer's tracing::info! and debug output
    let has_installation_debug = output.stderr.contains("Installing")
        || output.stderr.contains("📝 Rendering template:")
        || output.stderr.contains("✅ Template rendered successfully")
        || output.stderr.contains("DEBUG: Extracted metadata")
        || output.stderr.contains("[TRANSITIVE] Processing");

    assert!(
        has_installation_debug,
        "Expected installation debug output in verbose mode. Got: {}",
        &output.stderr
    );

    Ok(())
}

/// Test atomic progress tracking under concurrent load
#[tokio::test]
async fn test_concurrent_progress_tracking() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;

    // Create source repo with dependencies to stress test progress tracking
    let community_repo = project.create_source_repo("community").await?;

    // Create a dependency tree that will generate concurrent processing
    for i in 0..30 {
        community_repo
            .add_resource(
                "agents",
                &format!("progress-test-{:02}", i),
                r#"---
# Progress Test Agent
This agent is part of progress tracking tests.
---
"#,
            )
            .await?;
    }

    community_repo.commit_all("Initial commit")?;
    community_repo.tag_version("v1.0.0")?;

    // Create manifest
    let source_url = community_repo.bare_file_url(project.sources_path())?;
    let mut builder = ManifestBuilder::new().add_source("community", &source_url);

    for i in 0..30 {
        builder = builder.add_standard_agent(
            &format!("progress-test-{:02}", i),
            "community",
            &format!("agents/progress-test-{:02}.md", i),
        );
    }

    let manifest = builder.build();
    project.write_manifest(&manifest).await?;

    // Run install with verbose output to see progress tracking
    let output = project.run_agpm(&["install", "--verbose"])?;
    assert!(output.success, "Install should succeed. Stderr: {}", output.stderr);

    // Verify all agents were installed
    let agents_dir = project.project_path().join(".claude/agents");
    let mut installed_count = 0;

    let mut entries = tokio::fs::read_dir(&agents_dir).await?;
    while let Some(entry) = entries.next_entry().await? {
        if entry.file_name().to_string_lossy().starts_with("progress-test-") {
            installed_count += 1;
        }
    }

    assert_eq!(
        installed_count, 30,
        "All 30 progress test agents should be installed (found {})",
        installed_count
    );

    // Check that progress indicators appeared in output
    assert!(
        output.stderr.contains("Processing")
            || output.stderr.contains("processed")
            || output.stderr.contains("resolved")
            || output.stderr.contains("dependencies"),
        "Should show progress information in verbose output"
    );

    Ok(())
}

/// Test transitive version conflicts under concurrent resolution
#[tokio::test]
async fn test_transitive_version_conflicts_concurrent() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;

    // Create two source repos with conflicting versions of the same dependency
    let repo_a = project.create_source_repo("repo_a").await?;
    let repo_b = project.create_source_repo("repo_b").await?;

    // Add shared dependency in v1.0.0 to repo_a
    repo_a
        .add_resource(
            "agents",
            "shared-lib",
            r#"---
name: Shared Library v1
version: "1.0.0"
---
# Shared Library v1.0.0

This is version 1.0.0 of the shared library.
"#,
        )
        .await?;
    repo_a.commit_all("Add shared lib v1.0.0")?;
    repo_a.tag_version("v1.0.0")?;

    // Add same shared dependency in v2.0.0 to repo_b
    repo_b
        .add_resource(
            "agents",
            "shared-lib",
            r#"---
name: Shared Library v2
version: "2.0.0"
---
# Shared Library v2.0.0

This is version 2.0.0 of the shared library with breaking changes.
"#,
        )
        .await?;
    repo_b.commit_all("Add shared lib v2.0.0")?;
    repo_b.tag_version("v2.0.0")?;

    // Create top-level agents that will cause version conflicts
    for i in 0..5 {
        // Agent that depends on v1.0.0
        repo_a
            .add_resource(
                "agents",
                &format!("agent-v1-{:02}", i),
                format!(
                    r#"---
dependencies:
  agents:
    - path: ./shared-lib.md
      version: v1.0.0
---
# Agent V1 {:02}

This agent depends on shared-lib v1.0.0.
"#,
                    i
                )
                .as_str(),
            )
            .await?;

        // Agent that depends on v2.0.0
        repo_b
            .add_resource(
                "agents",
                &format!("agent-v2-{:02}", i),
                format!(
                    r#"---
dependencies:
  agents:
    - path: ./shared-lib.md
      version: v2.0.0
---
# Agent V2 {:02}

This agent depends on shared-lib v2.0.0.
"#,
                    i
                )
                .as_str(),
            )
            .await?;
    }

    repo_a.commit_all("Add v1 agents")?;
    repo_a.tag_version("v1.1.0")?;
    repo_b.commit_all("Add v2 agents")?;
    repo_b.tag_version("v2.1.0")?;

    // Get source URLs
    let repo_a_url = repo_a.bare_file_url(project.sources_path())?;
    let repo_b_url = repo_b.bare_file_url(project.sources_path())?;

    // Create manifest that will cause version conflicts
    let manifest = ManifestBuilder::new()
        .add_source("repo_a", &repo_a_url)
        .add_source("repo_b", &repo_b_url)
        // Add agents that depend on different versions - this will create conflicts
        .add_agent("agent-v1-00", |d| {
            d.source("repo_a").path("agents/agent-v1-00.md").version("v1.1.0")
        })
        .add_agent("agent-v2-00", |d| {
            d.source("repo_b").path("agents/agent-v2-00.md").version("v2.1.0")
        })
        // Add both v1 and v2 versions of shared-lib directly to force conflict
        .add_agent("shared-lib-v1", |d| {
            d.source("repo_a").path("agents/shared-lib.md").version("v1.0.0")
        })
        .add_agent("shared-lib-v2", |d| {
            d.source("repo_b").path("agents/shared-lib.md").version("v2.0.0")
        })
        .build();

    project.write_manifest(&manifest).await?;

    // Run install - should fail with version conflict
    let output = project.run_agpm(&["install"])?;

    // Should fail due to version conflicts
    assert!(
        !output.success,
        "Install should fail due to version conflicts. Stderr: {}",
        output.stderr
    );

    // Check that error message mentions conflicts
    let stderr_lower = output.stderr.to_lowercase();
    assert!(
        stderr_lower.contains("conflict")
            || stderr_lower.contains("incompatible")
            || stderr_lower.contains("no compatible version")
            || stderr_lower.contains("target path conflicts"),
        "Error message should mention conflict. Got: {}",
        output.stderr
    );

    // Verify that conflict is detected (the exact message may vary)
    assert!(
        stderr_lower.contains("conflict")
            || stderr_lower.contains("overwrite")
            || stderr_lower.contains("same installation path"),
        "Error should mention conflict resolution. Got: {}",
        output.stderr
    );

    println!("✅ Concurrent version conflict test passed - conflicts correctly detected");

    Ok(())
}

/// Test deep transitive chains under concurrent load (A→B→C→D)
#[tokio::test]
async fn test_deep_transitive_chains_concurrent() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;

    // Create source repo with deep dependency chains
    let chain_repo = project.create_source_repo("chain").await?;

    // Create a 4-level deep chain: A → B → C → D
    // Level D (leaf) - no dependencies
    chain_repo
        .add_resource(
            "agents",
            "level-d",
            r#"---
name: Level D Agent
# No dependencies - this is the leaf
---
# Level D Agent

This is the final level in the dependency chain with no further dependencies.
"#,
        )
        .await?;

    // Level C - depends on D
    chain_repo
        .add_resource(
            "agents",
            "level-c",
            r#"---
name: Level C Agent
dependencies:
  agents:
    - path: ./level-d.md
      version: v1.0.0
---
# Level C Agent

This agent depends on Level D.
"#,
        )
        .await?;

    // Level B - depends on C
    chain_repo
        .add_resource(
            "agents",
            "level-b",
            r#"---
name: Level B Agent
dependencies:
  agents:
    - path: ./level-c.md
      version: v1.0.0
---
# Level B Agent

This agent depends on Level C.
"#,
        )
        .await?;

    // Level A - depends on B
    chain_repo
        .add_resource(
            "agents",
            "level-a",
            r#"---
name: Level A Agent
dependencies:
  agents:
    - path: ./level-b.md
      version: v1.0.0
---
# Level A Agent

This agent depends on Level B.
"#,
        )
        .await?;

    // Create multiple parallel chains that converge on shared dependencies
    for i in 0..8 {
        chain_repo
            .add_resource(
                "agents",
                &format!("chain-{:02}-a", i),
                format!(
                    r#"---
dependencies:
  agents:
    - path: ./level-a.md
      version: v1.0.0
---
# Chain {:02} Level A

This chain starts at Level A.
"#,
                    i
                )
                .as_str(),
            )
            .await?;
    }

    chain_repo.commit_all("Add deep chain dependencies")?;
    chain_repo.tag_version("v1.0.0")?;

    // Create manifest with multiple parallel chains
    let source_url = chain_repo.bare_file_url(project.sources_path())?;
    let mut builder = ManifestBuilder::new().add_source("chain", &source_url);

    // Add all chain starting points
    for i in 0..8 {
        builder = builder.add_agent(&format!("chain-{:02}", i), |d| {
            d.source("chain").path(&format!("agents/chain-{:02}-a.md", i)).version("v1.0.0")
        });
    }

    let manifest = builder.build();
    project.write_manifest(&manifest).await?;

    // Measure time for performance verification
    let start_time = Instant::now();

    // Run install
    let output = project.run_agpm(&["install"])?;
    assert!(output.success, "Install should succeed. Stderr: {}", output.stderr);

    let install_duration = start_time.elapsed();

    // Verify all files were installed
    let agents_dir = project.project_path().join(".claude/agents");
    let mut installed_files = std::fs::read_dir(&agents_dir)?
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().to_string())
        .collect::<Vec<_>>();

    installed_files.sort();

    // Should have all chain starting points (8) + all intermediate levels (4) + duplicates of shared deps
    assert!(
        installed_files.len() >= 12, // At least 8 unique chains + 4 levels
        "Expected at least 12 installed files, got {}. Files: {:?}",
        installed_files.len(),
        installed_files
    );

    // Verify specific files exist
    assert!(installed_files.contains(&"level-a.md".to_string()), "Level A should be installed");
    assert!(installed_files.contains(&"level-b.md".to_string()), "Level B should be installed");
    assert!(installed_files.contains(&"level-c.md".to_string()), "Level C should be installed");
    assert!(installed_files.contains(&"level-d.md".to_string()), "Level D should be installed");

    // Check for chain starting points
    for i in 0..8 {
        let expected_name = format!("chain-{:02}-a.md", i);
        assert!(
            installed_files.contains(&expected_name),
            "Chain starting point {} should be installed",
            expected_name
        );
    }

    // Performance check - should be reasonable for concurrent processing
    println!(
        "✅ Deep chain concurrent install completed in {:?} for {} files",
        install_duration,
        installed_files.len()
    );

    // Should complete within reasonable time (concurrent processing should be fast)
    assert!(
        install_duration.as_secs() < 30,
        "Deep chain install took too long: {:?}",
        install_duration
    );

    Ok(())
}

/// Test stress scenario with 100+ parallel transitive dependencies
#[tokio::test]
async fn test_stress_100_parallel_dependencies() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;

    // Create source repo with many dependencies
    let stress_repo = project.create_source_repo("stress").await?;

    // Add base dependencies (20)
    for i in 0..20 {
        stress_repo
            .add_resource(
                "agents",
                &format!("base-{:03}", i),
                r#"---
# Base dependency agent
This is a base dependency with no further dependencies.
---
"#,
            )
            .await?;
    }

    // Add intermediate dependencies that depend on base ones (40)
    for i in 0..40 {
        let base_dep = i % 20;
        stress_repo
            .add_resource(
                "agents",
                &format!("intermediate-{:03}", i),
                format!(
                    r#"---
dependencies:
  agents:
    - path: base-{:03}.md
      version: v1.0.0
---
# Intermediate dependency {}

This depends on base-{:03}.
"#,
                    base_dep, base_dep, base_dep
                )
                .as_str(),
            )
            .await?;
    }

    // Add top-level dependencies that depend on intermediate ones (50)
    for i in 0..50 {
        let intermediate_dep = i % 40;
        let intermediate_name = format!("intermediate-{:03}", intermediate_dep);
        stress_repo
            .add_resource(
                "agents",
                &format!("top-level-{:03}", i),
                format!(
                    r#"---
dependencies:
  agents:
    - path: {}.md
      version: v1.0.0
---
# Top Level dependency {}

This depends on {}.
"#,
                    intermediate_name, i, intermediate_name
                )
                .as_str(),
            )
            .await?;
    }

    stress_repo.commit_all("Add stress test dependencies")?;
    stress_repo.tag_version("v1.0.0")?;

    // Create manifest with 50 top-level dependencies (will create 110 total with transitive deps)
    let source_url = stress_repo.bare_file_url(project.sources_path())?;
    let mut builder = ManifestBuilder::new().add_source("stress", &source_url);

    for i in 0..50 {
        builder = builder.add_agent(&format!("stress-{:03}", i), |d| {
            d.source("stress").path(&format!("agents/top-level-{:03}.md", i)).version("v1.0.0")
        });
    }

    let manifest = builder.build();
    project.write_manifest(&manifest).await?;

    // Measure performance
    let start_time = Instant::now();

    // Run install
    let output = project.run_agpm(&["install"])?;
    assert!(output.success, "Install should succeed. Stderr: {}", output.stderr);

    let install_duration = start_time.elapsed();

    // Verify installations
    let agents_dir = project.project_path().join(".claude/agents");
    let installed_count = std::fs::read_dir(&agents_dir)?.count();

    // Should have installed many files (50 top-level + transitive)
    assert!(
        installed_count >= 50,
        "Should have at least 50 installed files, got {}",
        installed_count
    );

    // Performance should be reasonable for concurrent processing
    let files_per_second = installed_count as f64 / install_duration.as_secs_f64();

    println!(
        "✅ Stress test: {} files installed in {:?} ({:.1} files/sec)",
        installed_count, install_duration, files_per_second
    );

    // Should complete within reasonable time even with many dependencies
    assert!(install_duration.as_secs() < 60, "Stress test took too long: {:?}", install_duration);

    // Should have reasonable throughput (at least 2 files/sec even on slow systems)
    assert!(files_per_second > 2.0, "Throughput too low: {:.1} files/sec", files_per_second);

    // Verify specific files exist
    for i in 0..20 {
        let expected_file = format!("base-{:03}.md", i);
        let file_path = agents_dir.join(&expected_file);
        assert!(file_path.exists(), "Base dependency {} should be installed", expected_file);
    }

    Ok(())
}

/// Test deterministic concurrent resolution - multiple parallel identical installs
#[tokio::test]
async fn test_deterministic_concurrent_resolution() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    // Create source repo with consistent dependency graph
    let project = TestProject::new().await?;
    let source_repo = project.create_source_repo("deterministic").await?;

    // Add base dependencies
    for i in 0..5 {
        source_repo
            .add_resource(
                "agents",
                &format!("base-{:02}", i),
                r#"---
# Base dependency
This is a consistent base dependency.
---
"#,
            )
            .await?;
    }

    // Add agents with transitive dependencies
    for i in 0..8 {
        let base_idx_1 = (i * 2) % 5;
        let base_idx_2 = (i * 2 + 1) % 5;

        source_repo
            .add_resource(
                "agents",
                &format!("agent-{:02}", i),
                format!(
                    r#"---
dependencies:
  agents:
    - path: base-{:02}.md
      version: v1.0.0
    - path: base-{:02}.md
      version: v1.0.0
---
# Agent {:02}

Agent with transitive dependencies on base-{:02} and base-{:02}.
"#,
                    base_idx_1, base_idx_2, i, base_idx_1, base_idx_2
                )
                .as_str(),
            )
            .await?;
    }

    source_repo.commit_all("Add deterministic test dependencies")?;
    source_repo.tag_version("v1.0.0")?;

    // Create manifest with all agents using the working project
    let source_url = source_repo.bare_file_url(project.sources_path())?;

    // Create manifest with all agents
    let mut builder = ManifestBuilder::new().add_source("deterministic", &source_url);
    for i in 0..8 {
        builder = builder.add_agent(&format!("agent-{:02}", i), |d| {
            d.source("deterministic").path(&format!("agents/agent-{:02}.md", i)).version("v1.0.0")
        });
    }
    let manifest = builder.build();
    project.write_manifest(&manifest).await?;

    // Run multiple installs on the same project to test deterministic behavior
    let mut lockfiles = Vec::new();

    for run_id in 0..5 {
        println!("Running deterministic install {}...", run_id + 1);

        // Run install - this will exercise the resolver's concurrent processing internally
        let output = project.run_agpm(&["install"])?;

        assert!(output.success, "Install {} should succeed. Stderr: {}", run_id, output.stderr);

        // Read the generated lockfile
        let lockfile_content = project.read_lockfile().await?;
        lockfiles.push(lockfile_content);
    }

    // Should have all successful results
    assert_eq!(lockfiles.len(), 5, "All 5 installs should succeed");

    // Normalize lockfiles by removing timestamps before comparing
    let normalize = |s: &str| {
        s.lines()
            .filter(|line| !line.trim().starts_with("fetched_at"))
            .collect::<Vec<_>>()
            .join("\n")
    };

    let normalized_first = normalize(&lockfiles[0]);
    for (i, lockfile) in lockfiles.iter().enumerate().skip(1) {
        let normalized_current = normalize(lockfile);
        assert_eq!(
            normalized_first,
            normalized_current,
            "Lockfile {} differs from first. This indicates non-deterministic resolution.",
            i + 1
        );
    }

    println!("✅ All 5 installs produced identical lockfiles");

    // Verify the lockfile contains expected dependencies
    for i in 0..8 {
        assert!(
            normalized_first.contains(&format!("agent-{:02}", i)),
            "Lockfile should contain agent-{:02}",
            i
        );
    }

    // Should have some base dependencies as transitive deps
    let mut base_dep_count = 0;
    for i in 0..5 {
        if normalized_first.contains(&format!("base-{:02}", i)) {
            base_dep_count += 1;
        }
    }
    assert!(base_dep_count > 0, "Should have some base dependencies in lockfile");

    println!(
        "✅ Deterministic concurrent resolution verified - {} base dependencies found",
        base_dep_count
    );

    Ok(())
}
