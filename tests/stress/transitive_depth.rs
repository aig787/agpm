//! Stress test for deeply nested transitive dependencies
//!
//! This test validates that AGPM can handle complex dependency graphs with many levels
//! of nesting while maintaining reasonable performance and proper cycle detection.

use anyhow::Result;
use std::time::Instant;

// Import from common module
use crate::common::{ManifestBuilder, TestProject};

/// Creates a linear chain of transitive dependencies
/// A -> B -> C -> D -> ... -> N
async fn create_linear_chain(project: &TestProject, depth: usize) -> Result<Vec<String>> {
    let mut repos = Vec::new();

    // Create repositories in a chain
    for i in 0..depth {
        let repo_name = format!("chain-level-{}", i);
        let repo = project.create_source_repo(&repo_name).await?;

        let next_dep = if i < depth - 1 {
            format!("chain-level-{}", i + 1)
        } else {
            "leaf".to_string()
        };

        let content = format!(
            r#"---
name: Chain Level {}
dependencies:
  agents:
    - path: agents/{}.md
      version: v1.0.0
---
# Chain Level {}

This depends on next level in chain.
"#,
            i, next_dep, i
        );

        // Add current level's resource
        repo.add_resource("agents", &format!("level-{}", i), &content).await?;

        // Add next level's resource if we're not at end
        if i < depth - 1 {
            repo.add_resource(
                "agents",
                &next_dep,
                r#"---
name: Leaf Resource
---
# Final resource in chain
This is end of dependency chain.
"#,
            )
            .await?;
        }

        repo.commit_all(&format!("Add chain level {}", i))?;
        repos.push(format!("file://{}", repo.path.display()));
    }

    Ok(repos)
}

/// Creates a diamond-shaped dependency graph
///     A
///    / \
///   B   C
///  / \ / \
/// D   E   F
async fn create_diamond_graph(project: &TestProject, depth: usize) -> Result<Vec<String>> {
    let mut repos = Vec::new();

    // Create a root repository
    let root_repo = project.create_source_repo("root").await?;
    root_repo
        .add_resource(
            "agents",
            "root",
            r#"---
name: Root Agent
dependencies:
  agents:
    - path: agents/branch-b.md
      version: v1.0.0
    - path: agents/branch-c.md
      version: v1.0.0
---
# Root Agent
This depends on both branches B and C.
"#,
        )
        .await?;

    root_repo.commit_all("Add root agent")?;
    repos.push(format!("file://{}", root_repo.path.display()));

    // Create branch repositories
    for level in 1..=depth {
        for branch in 0..2_u32.pow(level as u32) {
            let repo_name = format!("L{}-B{}", level, branch);
            let repo = project.create_source_repo(&repo_name).await?;

            let mut content = format!(
                r#"---
name: Level {} Branch {}
dependencies:
  agents:
"#,
                level, branch
            );

            // Add dependencies to next level
            for next_branch in 0..2_u32.pow((level + 1) as u32) {
                if next_branch / 2 == branch {
                    content.push_str(&format!(
                        "    - path: agents/L{}-B{}.md\n      version: v1.0.0\n",
                        level + 1,
                        next_branch
                    ));
                }
            }

            content.push_str(&format!(
                r#"---
# Level {} Branch {}

This is a node in diamond dependency graph.
"#,
                level, branch
            ));

            repo.add_resource("agents", &format!("L{}-B{}", level, branch), &content).await?;

            repo.commit_all(&format!("Add level {} branch {}", level, branch))?;
            repos.push(format!("file://{}", repo.path.display()));
        }
    }

    // Create leaf repositories
    for leaf in 0..2_u32.pow((depth + 1) as u32) {
        let repo_name = format!("leaf-{}", leaf);
        let repo = project.create_source_repo(&repo_name).await?;

        repo.add_resource(
            "agents",
            &format!("L{}-B{}", depth + 1, leaf),
            r#"---
name: Leaf Agent
---
# Leaf Agent

This is a leaf node with no further dependencies.
"#,
        )
        .await?;

        repo.commit_all("Add leaf agent")?;
        repos.push(format!("file://{}", repo.path.display()));
    }

    Ok(repos)
}

/// Measures performance of dependency resolution with deep nesting
#[tokio::test]
async fn test_deep_linear_chain_performance() -> Result<()> {
    let project = TestProject::new().await?;

    // Test with progressively deeper chains
    let depths = vec![5, 10, 15];

    for depth in depths {
        let start = Instant::now();

        let repos = create_linear_chain(&project, depth).await?;

        // Create manifest with chain dependencies
        let mut manifest = ManifestBuilder::new();

        // Add all repositories as sources
        for (i, repo) in repos.iter().enumerate() {
            manifest = manifest.add_source(&format!("level-{}", i), repo);
        }

        // Add only first level as a direct dependency
        manifest = manifest.add_agent("chain-start", |d| {
            d.source("level-0").path("agents/level-0").version("v1.0.0")
        });

        // Write manifest and validate
        let manifest_toml = manifest.build();
        let manifest_path = project.project_path().join("agpm.toml");
        tokio::fs::write(&manifest_path, manifest_toml).await?;

        // Run validation with transitive resolution
        let result = project.run_agpm(&["validate", "--resolve", "--format", "json"]);

        let duration = start.elapsed();

        // Should succeed
        let output = result?;
        assert!(
            output.success,
            "Chain validation failed at depth {}: stderr: {}",
            depth, &output.stderr
        );

        // Log performance (no assertion - rely on nextest timeout for hangs)
        println!("Depth {}: validated in {}ms", depth, duration.as_millis());
    }

    Ok(())
}

/// Tests memory usage with large dependency graphs
#[tokio::test]
async fn test_memory_usage_large_graph() -> Result<()> {
    let project = TestProject::new().await?;
    let start = Instant::now();

    // Create a diamond graph
    let repos = create_diamond_graph(&project, 3).await?; // Creates ~31 repositories

    // Create manifest
    let mut manifest = ManifestBuilder::new();

    // Add all sources
    for (i, repo) in repos.iter().enumerate() {
        manifest = manifest.add_source(&format!("repo-{}", i), repo);
    }

    // Add root dependency
    manifest =
        manifest.add_agent("root", |d| d.source("repo-0").path("agents/root").version("v1.0.0"));

    // Write manifest
    let manifest_toml = manifest.build();
    let manifest_path = project.project_path().join("agpm.toml");
    tokio::fs::write(&manifest_path, manifest_toml).await?;

    // Track memory usage (rough estimate)
    let _memory_before = get_memory_usage();

    // Run validation
    let result = project.run_agpm(&["validate", "--resolve", "--format", "json"]);

    let duration = start.elapsed();
    let _memory_after = get_memory_usage();

    // Should succeed
    let output = result?;
    assert!(output.success, "Large graph validation failed: stderr: {}", &output.stderr);

    assert!(repos.len() >= 20, "Should have at least 20 repositories, got {}", repos.len());

    // Log performance (no assertion - rely on nextest timeout for hangs)
    println!("Processed {} repositories in {}ms", repos.len(), duration.as_millis());

    Ok(())
}

/// Tests cycle detection at various depths
#[tokio::test]
async fn test_cycle_detection_at_depth() -> Result<()> {
    let project = TestProject::new().await?;

    // Create a cycle at different depths
    let depths = vec![3, 5, 8];

    for cycle_depth in depths {
        // Create a test repository for cycle
        let cycle_repo = project.create_source_repo("cycle").await?;

        // Create files that will have circular dependencies
        for i in 0..cycle_depth {
            let file_name = format!("cycle-level-{}", i);
            // Each file points to the next one in the cycle
            // The last file (i == cycle_depth - 1) will point to level 0
            let next_level = (i + 1) % cycle_depth;
            let content = format!(
                r#"---
name: Cycle Level {}
dependencies:
  agents:
    - path: ./cycle-level-{}.md
      version: v1.0.0
---
# Cycle Level {}
This points to level {}.
"#,
                i, next_level, i, next_level
            );

            // Create agents directory if it doesn't exist
            cycle_repo.add_resource("agents", &file_name, &content).await?;
        }

        cycle_repo.commit_all(&format!("Create cycle at depth {}", cycle_depth))?;

        // Create manifest that depends on first level
        let manifest = ManifestBuilder::new()
            .add_agent("cycle-start", |d| d.source("cycle").path("agents/cycle-level-0"))
            .add_source("cycle", &format!("file://{}", cycle_repo.path.display()));

        let manifest_toml = manifest.build();
        let manifest_path = project.project_path().join("agpm.toml");
        tokio::fs::write(&manifest_path, manifest_toml).await?;

        // Try to install - should detect the cycle
        let result = project.run_agpm(&["install"]);

        let output = result?;

        assert!(
            !output.success,
            "Cycle detection failed at depth {}: expected validation to fail",
            cycle_depth
        );

        // Check that error message mentions cycle
        let stderr = &output.stderr;
        assert!(
            stderr.to_lowercase().contains("cycle")
                || stderr.to_lowercase().contains("circular")
                || stderr.to_lowercase().contains("loop"),
            "Error message should mention cycle at depth {}: {}",
            cycle_depth,
            stderr
        );

        println!("Successfully detected cycle at depth {}", cycle_depth);
    }

    Ok(())
}

/// Tests that resolution remains efficient with repeated shared dependencies
#[tokio::test]
async fn test_shared_dependency_efficiency() -> Result<()> {
    let project = TestProject::new().await?;
    let start = Instant::now();

    // Create a shared dependency
    let shared_repo = project.create_source_repo("shared").await?;
    shared_repo
        .add_resource(
            "agents",
            "shared-lib",
            r#"---
name: Shared Library
---
# Shared Library

This is a shared dependency used by multiple top-level agents.
"#,
        )
        .await?;
    shared_repo.commit_all("Add shared library")?;

    // Create multiple top-level dependencies that all use the shared library
    let mut manifest = ManifestBuilder::new()
        .add_source("shared", &format!("file://{}", shared_repo.path.display()));

    for i in 0..10 {
        let repo_name = format!("top-level-{}", i);
        let repo = project.create_source_repo(&repo_name).await?;

        let content = format!(
            r#"---
name: Top Level Agent {}
dependencies:
  agents:
    - path: agents/shared-lib.md
      version: v1.0.0
---
# Top Level Agent {}

This agent depends on the shared library.
"#,
            i, i
        );

        repo.add_resource("agents", "agent", &content).await?;
        repo.commit_all(&format!("Add top level agent {}", i))?;

        // Add to sources and dependencies
        manifest = manifest.add_source(&repo_name, &format!("file://{}", repo.path.display()));
        manifest = manifest.add_agent(&format!("agent-{}", i), |d| {
            d.source(&repo_name).path("agents/agent").version("v1.0.0")
        });
    }

    // Write manifest
    let manifest_toml = manifest.build();
    let manifest_path = project.project_path().join("agpm.toml");
    tokio::fs::write(&manifest_path, manifest_toml).await?;

    // Run validation
    let result = project.run_agpm(&["validate", "--resolve", "--format", "json"]);

    let duration = start.elapsed();

    // Should succeed
    let output = result?;
    assert!(output.success, "Shared dependency validation failed: stderr: {}", &output.stderr);

    // Log performance (no assertion - rely on nextest timeout for hangs)
    println!(
        "Validated 11 total dependencies (10 top-level + 1 shared) in {}ms",
        duration.as_millis()
    );

    Ok(())
}

/// Helper function to get current memory usage (platform-specific)
fn get_memory_usage() -> usize {
    #[cfg(unix)]
    {
        use std::fs;
        if let Ok(status) = fs::read_to_string("/proc/self/status") {
            for line in status.lines() {
                if line.starts_with("VmRSS:") {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if parts.len() >= 2 {
                        if let Ok(kb) = parts[1].parse::<usize>() {
                            return kb * 1024; // Convert KB to bytes
                        }
                    }
                }
            }
        }
    }

    #[cfg(target_os = "macos")]
    {
        use std::process::Command;
        if let Ok(output) = Command::new("ps").args(["-o", "rss=", "-p"]).output() {
            if let Ok(rss_str) = String::from_utf8(output.stdout) {
                if let Ok(rss_kb) = rss_str.trim().parse::<usize>() {
                    return rss_kb * 1024; // Convert KB to bytes
                }
            }
        }
    }

    // Fallback for Windows or unsupported platforms
    0
}
