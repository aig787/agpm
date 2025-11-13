// Stress tests for parallel processing in transitive dependency resolution
//
// Tests performance and scalability features:
// - Deep transitive chains (A→B→C→D)
// - 100+ parallel dependencies
// - Deterministic concurrent resolution
// - Performance under load

use anyhow::Result;
use std::time::Instant;

use crate::common::{ManifestBuilder, TestProject};

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

/// Test parallel batch processing with varying batch sizes
#[tokio::test]
async fn test_parallel_batch_processing() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;

    // Create source repo with many agents to test batch processing
    let batch_repo = project.create_source_repo("batch").await?;

    // Add agents with different complexity levels
    for i in 0..60 {
        batch_repo
            .add_resource(
                "agents",
                &format!("batch-agent-{:02}", i),
                format!(
                    r#"---
# Batch Agent {:02}
This is batch agent {} with medium complexity.
---
"#,
                    i, i
                )
                .as_str(),
            )
            .await?;
    }

    batch_repo.commit_all("Add batch test agents")?;
    batch_repo.tag_version("v1.0.0")?;

    // Create manifest with all agents
    let source_url = batch_repo.bare_file_url(project.sources_path())?;
    let mut builder = ManifestBuilder::new().add_source("batch", &source_url);

    for i in 0..60 {
        builder = builder.add_standard_agent(
            &format!("batch-agent-{:02}", i),
            "batch",
            &format!("agents/batch-agent-{:02}.md", i),
        );
    }

    let manifest = builder.build();
    project.write_manifest(&manifest).await?;

    // Run install
    let output = project.run_agpm(&["install"])?;
    assert!(output.success, "Install should succeed. Stderr: {}", output.stderr);

    // Verify all agents were installed
    let lockfile_content = project.read_lockfile().await?;
    for i in 0..60 {
        assert!(
            lockfile_content.contains(&format!("batch-agent-{:02}", i)),
            "Batch agent {:02} should be in lockfile",
            i
        );
    }

    // Verify files exist
    let agents_dir = project.project_path().join(".claude/agents");
    let mut installed_count = 0;
    let mut entries = tokio::fs::read_dir(&agents_dir).await?;
    while let Some(entry) = entries.next_entry().await? {
        if entry.file_name().to_string_lossy().starts_with("batch-agent-") {
            installed_count += 1;
        }
    }

    assert_eq!(installed_count, 60, "All 60 batch agents should be installed");

    println!("✅ Batch processing test completed successfully");

    Ok(())
}
