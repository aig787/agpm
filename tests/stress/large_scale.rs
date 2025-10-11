//! Stress tests for parallel installation with git worktrees

use agpm_cli::cache::Cache;
use agpm_cli::git::command_builder::GitCommand;
use agpm_cli::installer::{ResourceFilter, install_resources};
use agpm_cli::lockfile::{LockFile, LockedResource};
use agpm_cli::manifest::Manifest;
use agpm_cli::test_utils::init_test_logging;
use agpm_cli::utils::progress::MultiPhaseProgress;
use anyhow::Result;
use std::sync::Arc;
use tempfile::TempDir;
use tokio::fs;
use tracing::debug;

/// HEAVY STRESS TEST: Install 500 dependencies in parallel from multiple repos
#[tokio::test]
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
                version: Some(
                    if i % 3 == 0 {
                        "v1.0.0"
                    } else {
                        "v2.0.0"
                    }
                    .to_string(),
                ),
                resolved_commit: None,
                checksum: format!("sha256:r{}a{}", repo_idx, i),
                installed_at: format!(".claude/agents/repo{}_agent_{:03}.md", repo_idx, i),
                dependencies: vec![],
                resource_type: agpm_cli::core::ResourceType::Agent,
                tool: "claude-code".to_string(),
            });
            total_agents += 1;
        }
    }

    let manifest = Manifest::new();
    let progress = Arc::new(MultiPhaseProgress::new(false));

    println!("🚀 Starting heavy stress test with {} agents", total_agents);
    debug!("Starting parallel installation of {} agents", total_agents);
    let start = std::time::Instant::now();

    let (count, _) = install_resources(
        ResourceFilter::All,
        &lockfile,
        &manifest,
        &project_dir,
        cache.clone(),
        false,
        None,
        Some(progress),
    )
    .await?;

    let duration = start.elapsed();
    debug!("Installation completed in {:?}", duration);
    assert_eq!(count, total_agents);

    println!("✅ Successfully installed {} agents in {:?}", total_agents, duration);
    println!("   Average: {:?} per agent", duration / total_agents as u32);
    println!("   Rate: {:.1} agents/second", total_agents as f64 / duration.as_secs_f64());

    // Verify a sample of files
    for repo_idx in 0..5 {
        for i in (0..100).step_by(10) {
            let path =
                project_dir.join(format!(".claude/agents/repo{}_agent_{:03}.md", repo_idx, i));
            assert!(path.exists(), "Agent from repo {} #{} should exist", repo_idx, i);
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
    GitCommand::init().current_dir(path).execute_success().await?;

    // Set default branch to main
    GitCommand::new().args(["checkout", "-b", "main"]).current_dir(path).execute_success().await?;

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
    GitCommand::add(".").current_dir(path).execute_success().await?;

    GitCommand::commit("Initial commit with all agents")
        .current_dir(path)
        .execute_success()
        .await?;

    // Create v1.0.0 tag
    GitCommand::new().args(["tag", "v1.0.0"]).current_dir(path).execute_success().await?;

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

    GitCommand::add(".").current_dir(path).execute_success().await?;

    GitCommand::commit("Update for v2.0.0").current_dir(path).execute_success().await?;

    // Create v2.0.0 tag
    GitCommand::new().args(["tag", "v2.0.0"]).current_dir(path).execute_success().await?;

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

    GitCommand::add(".").current_dir(path).execute_success().await?;

    GitCommand::commit("Update for v3.0.0").current_dir(path).execute_success().await?;

    // Create v3.0.0 tag
    GitCommand::new().args(["tag", "v3.0.0"]).current_dir(path).execute_success().await?;

    Ok(())
}

/// HEAVY STRESS TEST: Update 500 existing dependencies to new versions
#[tokio::test]
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
                dependencies: vec![],
                resource_type: agpm_cli::core::ResourceType::Agent,
                tool: "claude-code".to_string(),
            });
            total_agents += 1;
        }
    }

    let manifest = Manifest::new();
    let progress = Arc::new(MultiPhaseProgress::new(false));

    println!("📦 Installing initial version (v1.0.0) of {} agents", total_agents);
    let start_install = std::time::Instant::now();

    let (count, _) = install_resources(
        ResourceFilter::All,
        &lockfile_v1,
        &manifest,
        &project_dir,
        cache.clone(),
        false,
        None,
        Some(progress.clone()),
    )
    .await?;
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
                dependencies: vec![],
                resource_type: agpm_cli::core::ResourceType::Agent,
                tool: "claude-code".to_string(),
            });
        }
    }

    let progress2 = Arc::new(MultiPhaseProgress::new(false));

    println!("🔄 Updating all {} agents to v2.0.0", total_agents);
    let start_update = std::time::Instant::now();

    let (update_count, _) = install_resources(
        ResourceFilter::All,
        &lockfile_v2,
        &manifest,
        &project_dir,
        cache.clone(),
        false,
        None,
        Some(progress2),
    )
    .await?;

    let update_duration = start_update.elapsed();
    assert_eq!(update_count, total_agents);

    println!("✅ Successfully updated {} agents in {:?}", total_agents, update_duration);
    println!("   Average: {:?} per agent", update_duration / total_agents as u32);
    println!("   Rate: {:.1} agents/second", total_agents as f64 / update_duration.as_secs_f64());

    // Verify files are updated (check a sample)
    for repo_idx in 0..5 {
        for i in (0..5).step_by(1) {
            let path =
                project_dir.join(format!(".claude/agents/repo{}_agent_{:03}.md", repo_idx, i));
            assert!(path.exists(), "Updated agent from repo {} #{} should exist", repo_idx, i);

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

/// MIXED REPOS TEST: Install dependencies from both file:// and https:// repositories
#[tokio::test]
async fn test_mixed_repos_file_and_https() -> Result<()> {
    init_test_logging(None);
    debug!("Starting test_mixed_repos_file_and_https");

    let temp_dir = TempDir::new()?;
    let project_dir = temp_dir.path().join("project");
    fs::create_dir_all(&project_dir).await?;

    // Create 2 local test repositories
    let mut repo_urls = Vec::new();
    for repo_num in 0..2 {
        let repo_dir = temp_dir.path().join(format!("local_repo_{}", repo_num));
        fs::create_dir_all(&repo_dir).await?;
        setup_large_test_repository(&repo_dir, 50).await?;
        repo_urls.push(format!("file://{}", repo_dir.display()));
    }

    // Add the agpm-community GitHub repository
    repo_urls.push("https://github.com/aig787/agpm-community.git".to_string());

    let cache = Cache::with_dir(temp_dir.path().join("cache"))?;
    let mut lockfile = LockFile::new();
    let mut total_resources = 0;

    // Add 50 agents from each local repo (100 total from local)
    for (repo_idx, repo_url) in repo_urls.iter().take(2).enumerate() {
        for i in 0..50 {
            lockfile.agents.push(LockedResource {
                name: format!("local_repo{}_agent_{:03}", repo_idx, i),
                source: Some(format!("local_repo_{}", repo_idx)),
                url: Some(repo_url.clone()),
                path: format!("agents/agent_{:03}.md", i),
                version: Some("v1.0.0".to_string()),
                resolved_commit: None,
                checksum: format!("sha256:lr{}a{}", repo_idx, i),
                installed_at: format!(".claude/agents/local_repo{}_agent_{:03}.md", repo_idx, i),
                dependencies: vec![],
                resource_type: agpm_cli::core::ResourceType::Agent,
                tool: "claude-code".to_string(),
            });
            total_resources += 1;
        }
    }

    // Add real agents from agpm-community repo (from setup_project.sh)
    let community_agents = [
        "agents/awesome-claude-code-subagents/categories/01-core-development/api-designer.md",
        "agents/awesome-claude-code-subagents/categories/01-core-development/backend-developer.md",
        "agents/awesome-claude-code-subagents/categories/01-core-development/frontend-developer.md",
        "agents/awesome-claude-code-subagents/categories/02-language-specialists/python-pro.md",
        "agents/awesome-claude-code-subagents/categories/02-language-specialists/rust-engineer.md",
        "agents/awesome-claude-code-subagents/categories/02-language-specialists/javascript-pro.md",
        "agents/awesome-claude-code-subagents/categories/03-infrastructure/database-administrator.md",
        "agents/awesome-claude-code-subagents/categories/04-quality-security/code-reviewer.md",
        "agents/awesome-claude-code-subagents/categories/04-quality-security/test-automator.md",
        "agents/awesome-claude-code-subagents/categories/04-quality-security/security-auditor.md",
    ];

    for (idx, agent_path) in community_agents.iter().enumerate() {
        lockfile.agents.push(LockedResource {
            name: format!("community_agent_{}", idx),
            source: Some("community".to_string()),
            url: Some("https://github.com/aig787/agpm-community.git".to_string()),
            path: agent_path.to_string(),
            version: None, // Use main branch
            resolved_commit: None,
            checksum: format!("sha256:community_{}", idx),
            installed_at: format!(".claude/agents/community_agent_{}.md", idx),
            dependencies: vec![],
            resource_type: agpm_cli::core::ResourceType::Agent,
            tool: "claude-code".to_string(),
        });
        total_resources += 1;
    }

    let manifest = Manifest::new();
    let progress = Arc::new(MultiPhaseProgress::new(false));

    println!(
        "🌐 Starting mixed repository test: {} local agents + {} community agents",
        total_resources - community_agents.len(),
        community_agents.len()
    );
    let start = std::time::Instant::now();

    let (count, _) = install_resources(
        ResourceFilter::All,
        &lockfile,
        &manifest,
        &project_dir,
        cache.clone(),
        false,
        None,
        Some(progress),
    )
    .await?;

    let duration = start.elapsed();
    assert_eq!(count, total_resources);

    println!("✅ Successfully installed {} resources in {:?}", total_resources, duration);
    println!("   Local file:// repos: {} agents", total_resources - community_agents.len());
    println!("   Remote https:// repo: {} agents", community_agents.len());
    println!("   Average: {:?} per resource", duration / total_resources as u32);

    // Verify local files exist
    for repo_idx in 0..2 {
        for i in (0..50).step_by(10) {
            let path = project_dir
                .join(format!(".claude/agents/local_repo{}_agent_{:03}.md", repo_idx, i));
            assert!(path.exists(), "Local agent from repo {} #{} should exist", repo_idx, i);
        }
    }

    // Verify community files exist
    for idx in 0..community_agents.len() {
        let path = project_dir.join(format!(".claude/agents/community_agent_{}.md", idx));
        assert!(path.exists(), "Community agent #{} should exist", idx);
    }

    Ok(())
}

/// COMMUNITY REPO TEST: Parallel checkout performance from real agpm-community repository
#[tokio::test]
async fn test_community_repo_parallel_checkout_performance() -> Result<()> {
    init_test_logging(None);
    debug!("Starting test_community_repo_parallel_checkout_performance");

    let temp_dir = TempDir::new()?;
    let project_dir = temp_dir.path().join("project");
    fs::create_dir_all(&project_dir).await?;

    let cache = Cache::with_dir(temp_dir.path().join("cache"))?;
    let mut lockfile = LockFile::new();

    // All available agents from the setup_project.sh script
    let community_agents = [
        (
            "api-designer",
            "agents/awesome-claude-code-subagents/categories/01-core-development/api-designer.md",
        ),
        (
            "backend-developer",
            "agents/awesome-claude-code-subagents/categories/01-core-development/backend-developer.md",
        ),
        (
            "frontend-developer",
            "agents/awesome-claude-code-subagents/categories/01-core-development/frontend-developer.md",
        ),
        (
            "python-pro",
            "agents/awesome-claude-code-subagents/categories/02-language-specialists/python-pro.md",
        ),
        (
            "rust-engineer",
            "agents/awesome-claude-code-subagents/categories/02-language-specialists/rust-engineer.md",
        ),
        (
            "javascript-pro",
            "agents/awesome-claude-code-subagents/categories/02-language-specialists/javascript-pro.md",
        ),
        (
            "database-administrator",
            "agents/awesome-claude-code-subagents/categories/03-infrastructure/database-administrator.md",
        ),
        (
            "code-reviewer",
            "agents/awesome-claude-code-subagents/categories/04-quality-security/code-reviewer.md",
        ),
        (
            "test-automator",
            "agents/awesome-claude-code-subagents/categories/04-quality-security/test-automator.md",
        ),
        (
            "security-auditor",
            "agents/awesome-claude-code-subagents/categories/04-quality-security/security-auditor.md",
        ),
        (
            "devops-engineer",
            "agents/awesome-claude-code-subagents/categories/03-infrastructure/devops-engineer.md",
        ),
        (
            "cloud-architect",
            "agents/awesome-claude-code-subagents/categories/03-infrastructure/cloud-architect.md",
        ),
        (
            "documentation-engineer",
            "agents/awesome-claude-code-subagents/categories/06-developer-experience/documentation-engineer.md",
        ),
        (
            "ml-engineer",
            "agents/awesome-claude-code-subagents/categories/05-data-ai/ml-engineer.md",
        ),
        (
            "multi-agent-coordinator",
            "agents/awesome-claude-code-subagents/categories/09-meta-orchestration/multi-agent-coordinator.md",
        ),
    ];

    for (name, path) in community_agents.iter() {
        lockfile.agents.push(LockedResource {
            name: name.to_string(),
            source: Some("community".to_string()),
            url: Some("https://github.com/aig787/agpm-community.git".to_string()),
            path: path.to_string(),
            version: None, // Use main branch
            resolved_commit: None,
            checksum: format!("sha256:community_{}", name),
            installed_at: format!(".claude/agents/{}.md", name),
            dependencies: vec![],
            resource_type: agpm_cli::core::ResourceType::Agent,
            tool: "claude-code".to_string(),
        });
    }

    let total_agents = community_agents.len();
    let manifest = Manifest::new();
    let progress = Arc::new(MultiPhaseProgress::new(false));

    println!("📦 Testing parallel checkout from agpm-community repository");
    println!("   Repository: https://github.com/aig787/agpm-community.git");
    println!("   Agents: {}", total_agents);

    let start = std::time::Instant::now();

    let (count, _) = install_resources(
        ResourceFilter::All,
        &lockfile,
        &manifest,
        &project_dir,
        cache.clone(),
        false,
        None,
        Some(progress),
    )
    .await?;

    let duration = start.elapsed();
    assert_eq!(count, total_agents);

    println!("✅ Successfully installed {} community agents in {:?}", total_agents, duration);
    println!("   Average: {:?} per agent", duration / total_agents as u32);
    println!("   Rate: {:.1} agents/second", total_agents as f64 / duration.as_secs_f64());

    // Verify all community agents were installed
    for (name, _) in community_agents.iter() {
        let path = project_dir.join(format!(".claude/agents/{}.md", name));
        assert!(path.exists(), "Community agent '{}' should exist", name);

        // Verify the file has content (not empty)
        let content = fs::read_to_string(&path).await?;
        assert!(!content.is_empty(), "Community agent '{}' should have content", name);
        assert!(
            content.contains("# ") || content.contains("## "),
            "Community agent '{}' should look like a valid markdown file",
            name
        );
    }

    // Performance assertion - community repo should complete in reasonable time
    assert!(
        duration.as_secs() < 120,
        "{} community agents should install in under 2 minutes, took {:?}",
        total_agents,
        duration
    );

    Ok(())
}

/// COMMUNITY REPO 500 DEPENDENCIES TEST: Install 500 dependencies from community repo with filename collision handling
#[tokio::test]
async fn test_community_repo_500_dependencies() -> Result<()> {
    init_test_logging(None);
    debug!("Starting test_community_repo_500_dependencies");

    let temp_dir = TempDir::new()?;
    let project_dir = temp_dir.path().join("project");
    tokio::fs::create_dir_all(&project_dir).await?;

    let cache = Cache::with_dir(temp_dir.path().join("cache"))?;
    let mut lockfile = LockFile::new();

    // The 15 agents available in agpm-community
    let community_agents = [
        (
            "api-designer",
            "agents/awesome-claude-code-subagents/categories/01-core-development/api-designer.md",
        ),
        (
            "backend-developer",
            "agents/awesome-claude-code-subagents/categories/01-core-development/backend-developer.md",
        ),
        (
            "frontend-developer",
            "agents/awesome-claude-code-subagents/categories/01-core-development/frontend-developer.md",
        ),
        (
            "python-pro",
            "agents/awesome-claude-code-subagents/categories/02-language-specialists/python-pro.md",
        ),
        (
            "rust-engineer",
            "agents/awesome-claude-code-subagents/categories/02-language-specialists/rust-engineer.md",
        ),
        (
            "javascript-pro",
            "agents/awesome-claude-code-subagents/categories/02-language-specialists/javascript-pro.md",
        ),
        (
            "database-administrator",
            "agents/awesome-claude-code-subagents/categories/03-infrastructure/database-administrator.md",
        ),
        (
            "code-reviewer",
            "agents/awesome-claude-code-subagents/categories/04-quality-security/code-reviewer.md",
        ),
        (
            "test-automator",
            "agents/awesome-claude-code-subagents/categories/04-quality-security/test-automator.md",
        ),
        (
            "security-auditor",
            "agents/awesome-claude-code-subagents/categories/04-quality-security/security-auditor.md",
        ),
        (
            "devops-engineer",
            "agents/awesome-claude-code-subagents/categories/03-infrastructure/devops-engineer.md",
        ),
        (
            "cloud-architect",
            "agents/awesome-claude-code-subagents/categories/03-infrastructure/cloud-architect.md",
        ),
        (
            "documentation-engineer",
            "agents/awesome-claude-code-subagents/categories/06-developer-experience/documentation-engineer.md",
        ),
        (
            "ml-engineer",
            "agents/awesome-claude-code-subagents/categories/05-data-ai/ml-engineer.md",
        ),
        (
            "multi-agent-coordinator",
            "agents/awesome-claude-code-subagents/categories/09-meta-orchestration/multi-agent-coordinator.md",
        ),
    ];

    // Create 500 dependencies by cycling through the available agents
    for i in 0..500 {
        let agent_index = i % community_agents.len();
        let (agent_name_base, agent_path) = community_agents[agent_index];

        // Create unique name for each instance to handle collisions
        let unique_agent_name = format!("{}-{:03}", agent_name_base, i);

        // Create unique installed_at path with suffix
        let unique_filename = format!("{}-{:03}.md", agent_name_base, i);

        let resource = LockedResource {
            name: unique_agent_name.clone(),
            source: Some("community".to_string()),
            url: Some("https://github.com/aig787/agpm-community.git".to_string()),
            path: agent_path.to_string(),
            version: Some("main".to_string()),
            resolved_commit: Some("main".to_string()), // Will be resolved during installation
            checksum: "sha256:placeholder".to_string(), // Will be computed during installation
            installed_at: format!(".claude/agents/{}", unique_filename),
            dependencies: vec![],
            resource_type: agpm_cli::core::ResourceType::Agent,
            tool: "claude-code".to_string(),
        };

        lockfile.agents.push(resource);
    }

    // Install all dependencies in parallel
    let start = std::time::Instant::now();
    let manifest = Manifest::new();
    let progress = Arc::new(MultiPhaseProgress::new(false));

    let (_, _) = install_resources(
        ResourceFilter::All,
        &lockfile,
        &manifest,
        &project_dir,
        cache,
        false,
        None,
        Some(progress),
    )
    .await?;

    let duration = start.elapsed();

    println!("Installed 500 community dependencies in {:?}", duration);

    // Verify agents were installed
    let agents_dir = project_dir.join(".claude/agents");
    assert!(agents_dir.exists(), "Agents directory should exist");

    let mut agent_files = tokio::fs::read_dir(&agents_dir).await?;
    let mut count = 0;
    while let Some(entry) = agent_files.next_entry().await? {
        if entry.file_name().to_string_lossy().ends_with(".md") {
            count += 1;
        }
    }

    assert_eq!(count, 500, "Should have installed exactly 500 agent files");

    // Verify a few random agents have valid content
    for i in [0, 100, 250, 499] {
        let agent_name_base = community_agents[i % community_agents.len()].0;
        let unique_filename = format!("{}-{:03}.md", agent_name_base, i);
        let agent_path = agents_dir.join(&unique_filename);
        assert!(agent_path.exists(), "Agent {} should exist", unique_filename);

        let content = tokio::fs::read_to_string(&agent_path).await?;
        assert!(!content.is_empty(), "Agent {} should have content", unique_filename);
        assert!(
            content.contains("# ") || content.contains("## "),
            "Agent '{}' should look like a valid markdown file",
            unique_filename
        );
    }

    // Performance assertion - 500 dependencies should complete in reasonable time
    assert!(
        duration.as_secs() < 300,
        "500 community dependencies should install in under 5 minutes, took {:?}",
        duration
    );

    Ok(())
}
