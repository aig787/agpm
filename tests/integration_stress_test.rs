//! Stress tests for parallel installation with git worktrees

use anyhow::Result;
use ccpm::cache::Cache;
use ccpm::git::command_builder::GitCommand;
use ccpm::installer::install_resources_parallel;
use ccpm::lockfile::{LockFile, LockedResource};
use ccpm::manifest::Manifest;
use ccpm::test_utils::init_test_logging;
use ccpm::utils::progress::ProgressBar;
use tempfile::TempDir;
use tokio::fs;
use tracing::debug;

/// HEAVY STRESS TEST: Install 500 dependencies in parallel from multiple repos
#[tokio::test]
#[ignore] // Run with --ignored flag for heavy stress testing
async fn test_heavy_stress_500_dependencies() -> Result<()> {
    init_test_logging(None);
    debug!("Starting test_heavy_stress_500_dependencies");

    let temp_dir = TempDir::new()?;
    let project_dir = temp_dir.path().join("project");
    fs::create_dir_all(&project_dir).await?;

    // Create 5 test repositories with 100 agents each
    let mut repo_urls = Vec::new();
    for repo_num in 0..5 {
        let repo_dir = temp_dir.path().join(format!("repo_{}", repo_num));
        fs::create_dir_all(&repo_dir).await?;
        setup_large_test_repository(&repo_dir, 100).await?;
        repo_urls.push(format!("file://{}", repo_dir.display()));
    }

    // Create cache
    let cache = Cache::with_dir(temp_dir.path().join("cache"))?;

    // Create lockfile with 500 agents total (100 from each repo)
    let mut lockfile = LockFile::new();
    let mut total_agents = 0;

    for (repo_idx, repo_url) in repo_urls.iter().enumerate() {
        for i in 0..100 {
            lockfile.agents.push(LockedResource {
                name: format!("repo{}_agent_{:03}", repo_idx, i),
                source: Some(format!("repo_{}", repo_idx)),
                url: Some(repo_url.clone()),
                path: format!("agents/agent_{:03}.md", i),
                version: Some(if i % 3 == 0 { "v1.0.0" } else { "v2.0.0" }.to_string()),
                resolved_commit: None,
                checksum: format!("sha256:r{}a{}", repo_idx, i),
                installed_at: format!(".claude/agents/repo{}_agent_{:03}.md", repo_idx, i),
            });
            total_agents += 1;
        }
    }

    let manifest = Manifest::new();
    let pb = ProgressBar::new(total_agents as u64);
    pb.set_message("Installing 500 agents from 5 repositories");

    println!("🚀 Starting heavy stress test with {} agents", total_agents);
    debug!("Starting parallel installation of {} agents", total_agents);
    let start = std::time::Instant::now();

    let count = install_resources_parallel(&lockfile, &manifest, &project_dir, &pb, &cache).await?;

    let duration = start.elapsed();
    debug!("Installation completed in {:?}", duration);
    assert_eq!(count, total_agents);

    println!(
        "✅ Successfully installed {} agents in {:?}",
        total_agents, duration
    );
    println!("   Average: {:?} per agent", duration / total_agents as u32);
    println!(
        "   Rate: {:.1} agents/second",
        total_agents as f64 / duration.as_secs_f64()
    );

    // Verify a sample of files
    for repo_idx in 0..5 {
        for i in (0..100).step_by(10) {
            let path =
                project_dir.join(format!(".claude/agents/repo{}_agent_{:03}.md", repo_idx, i));
            assert!(
                path.exists(),
                "Agent from repo {} #{} should exist",
                repo_idx,
                i
            );
        }
    }

    // Don't clean up worktrees - they're reusable now
    // cache.cleanup_all_worktrees().await?;

    // Performance assertion - even 500 agents should complete reasonably
    assert!(
        duration.as_secs() < 60,
        "500 agents should install in under 60 seconds, took {:?}",
        duration
    );

    // Give the system a moment to clean up resources before next test
    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

    Ok(())
}

/// Helper to create a large test repository with many files and multiple tags
async fn setup_large_test_repository(path: &std::path::PathBuf, num_files: usize) -> Result<()> {
    // Initialize repository
    GitCommand::init()
        .current_dir(path)
        .execute_success()
        .await?;

    // Set default branch to main
    GitCommand::new()
        .args(["checkout", "-b", "main"])
        .current_dir(path)
        .execute_success()
        .await?;

    // Configure git
    GitCommand::new()
        .args(["config", "user.email", "test@example.com"])
        .current_dir(path)
        .execute_success()
        .await?;

    GitCommand::new()
        .args(["config", "user.name", "Test User"])
        .current_dir(path)
        .execute_success()
        .await?;

    // Create agents directory
    let agents_dir = path.join("agents");
    fs::create_dir_all(&agents_dir).await?;

    // Create initial batch of files
    for i in 0..num_files {
        let agent_path = agents_dir.join(format!("agent_{:03}.md", i));
        let content = format!(
            "# Agent {}

\
            This is test agent number {}.

\
            ## Features
\
            - Feature 1 with detailed description
\
            - Feature 2 with implementation notes
\
            - Feature 3 with examples

\
            ## Configuration
\
            ```json
\
            {{
\
              \"id\": {},
\
              \"enabled\": true,
\
              \"priority\": {}
\
            }}
\
            ```
",
            i,
            i,
            i,
            i % 10
        );
        fs::write(&agent_path, content).await?;
    }

    // Initial commit
    GitCommand::add(".")
        .current_dir(path)
        .execute_success()
        .await?;

    GitCommand::commit("Initial commit with all agents")
        .current_dir(path)
        .execute_success()
        .await?;

    // Create v1.0.0 tag
    GitCommand::new()
        .args(["tag", "v1.0.0"])
        .current_dir(path)
        .execute_success()
        .await?;

    // Make some changes for v2.0.0
    for i in 0..5 {
        let agent_path = agents_dir.join(format!("agent_{:03}.md", i));
        let content = fs::read_to_string(&agent_path).await?;
        fs::write(
            &agent_path,
            format!(
                "{}
## Updated in v2.0.0
",
                content
            ),
        )
        .await?;
    }

    GitCommand::add(".")
        .current_dir(path)
        .execute_success()
        .await?;

    GitCommand::commit("Update for v2.0.0")
        .current_dir(path)
        .execute_success()
        .await?;

    // Create v2.0.0 tag
    GitCommand::new()
        .args(["tag", "v2.0.0"])
        .current_dir(path)
        .execute_success()
        .await?;

    // More changes for v3.0.0
    for i in 5..10 {
        let agent_path = agents_dir.join(format!("agent_{:03}.md", i));
        let content = fs::read_to_string(&agent_path).await?;
        fs::write(
            &agent_path,
            format!(
                "{}
## Updated in v3.0.0
",
                content
            ),
        )
        .await?;
    }

    GitCommand::add(".")
        .current_dir(path)
        .execute_success()
        .await?;

    GitCommand::commit("Update for v3.0.0")
        .current_dir(path)
        .execute_success()
        .await?;

    // Create v3.0.0 tag
    GitCommand::new()
        .args(["tag", "v3.0.0"])
        .current_dir(path)
        .execute_success()
        .await?;

    Ok(())
}

/// HEAVY STRESS TEST: Update 500 existing dependencies to new versions
#[tokio::test]
#[ignore] // Run with --ignored flag for heavy stress testing
async fn test_heavy_stress_500_updates() -> Result<()> {
    init_test_logging(None);
    debug!("Starting test_heavy_stress_500_updates");

    let temp_dir = TempDir::new()?;
    let project_dir = temp_dir.path().join("project");
    fs::create_dir_all(&project_dir).await?;

    // Create 5 test repositories with 100 agents each
    let mut repo_urls = Vec::new();
    for repo_num in 0..5 {
        let repo_dir = temp_dir.path().join(format!("repo_{}", repo_num));
        fs::create_dir_all(&repo_dir).await?;
        setup_large_test_repository(&repo_dir, 100).await?;
        repo_urls.push(format!("file://{}", repo_dir.display()));
    }

    // Create cache
    let cache = Cache::with_dir(temp_dir.path().join("cache"))?;

    // First install: Create lockfile with 500 agents at v1.0.0
    let mut lockfile_v1 = LockFile::new();
    let mut total_agents = 0;

    for (repo_idx, repo_url) in repo_urls.iter().enumerate() {
        for i in 0..100 {
            lockfile_v1.agents.push(LockedResource {
                name: format!("repo{}_agent_{:03}", repo_idx, i),
                source: Some(format!("repo_{}", repo_idx)),
                url: Some(repo_url.clone()),
                path: format!("agents/agent_{:03}.md", i),
                version: Some("v1.0.0".to_string()),
                resolved_commit: None,
                checksum: format!("sha256:r{}a{}v1", repo_idx, i),
                installed_at: format!(".claude/agents/repo{}_agent_{:03}.md", repo_idx, i),
            });
            total_agents += 1;
        }
    }

    let manifest = Manifest::new();
    let pb = ProgressBar::new(total_agents as u64);
    pb.set_message("Initial installation of 500 agents");

    println!(
        "📦 Installing initial version (v1.0.0) of {} agents",
        total_agents
    );
    let start_install = std::time::Instant::now();

    let count =
        install_resources_parallel(&lockfile_v1, &manifest, &project_dir, &pb, &cache).await?;
    assert_eq!(count, total_agents);

    let install_duration = start_install.elapsed();
    println!("   Initial install took {:?}", install_duration);

    // Don't clean up worktrees between installs - they're reusable
    // cache.cleanup_all_worktrees().await?;

    // Now update: Create lockfile with all 500 agents at v2.0.0
    let mut lockfile_v2 = LockFile::new();

    for (repo_idx, repo_url) in repo_urls.iter().enumerate() {
        for i in 0..100 {
            lockfile_v2.agents.push(LockedResource {
                name: format!("repo{}_agent_{:03}", repo_idx, i),
                source: Some(format!("repo_{}", repo_idx)),
                url: Some(repo_url.clone()),
                path: format!("agents/agent_{:03}.md", i),
                version: Some("v2.0.0".to_string()),
                resolved_commit: None,
                checksum: format!("sha256:r{}a{}v2", repo_idx, i),
                installed_at: format!(".claude/agents/repo{}_agent_{:03}.md", repo_idx, i),
            });
        }
    }

    let pb2 = ProgressBar::new(total_agents as u64);
    pb2.set_message("Updating 500 agents from v1.0.0 to v2.0.0");

    println!("🔄 Updating all {} agents to v2.0.0", total_agents);
    let start_update = std::time::Instant::now();

    let update_count =
        install_resources_parallel(&lockfile_v2, &manifest, &project_dir, &pb2, &cache).await?;

    let update_duration = start_update.elapsed();
    assert_eq!(update_count, total_agents);

    println!(
        "✅ Successfully updated {} agents in {:?}",
        total_agents, update_duration
    );
    println!(
        "   Average: {:?} per agent",
        update_duration / total_agents as u32
    );
    println!(
        "   Rate: {:.1} agents/second",
        total_agents as f64 / update_duration.as_secs_f64()
    );

    // Verify files are updated (check a sample)
    for repo_idx in 0..5 {
        for i in (0..5).step_by(1) {
            let path =
                project_dir.join(format!(".claude/agents/repo{}_agent_{:03}.md", repo_idx, i));
            assert!(
                path.exists(),
                "Updated agent from repo {} #{} should exist",
                repo_idx,
                i
            );

            // For the first 5 agents of each repo, they should have v2.0.0 content
            let content = fs::read_to_string(&path).await?;
            assert!(
                content.contains("Updated in v2.0.0"),
                "Agent repo {} #{} should have v2.0.0 content",
                repo_idx,
                i
            );
        }
    }

    // Don't clean up worktrees - they're reusable now
    // cache.cleanup_all_worktrees().await?;

    // Performance assertion - updates should also complete reasonably
    assert!(
        update_duration.as_secs() < 60,
        "500 agent updates should complete in under 60 seconds, took {:?}",
        update_duration
    );

    // Give the system a moment to clean up resources
    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

    Ok(())
}
