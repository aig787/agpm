//! Chaos tests for conflict tracking under concurrent load.
//!
//! These tests inject random delays and high concurrency to expose
//! race conditions and deadlocks in the conflict tracking code.

use anyhow::Result;
use rand::rngs::SmallRng;
use rand::{Rng, SeedableRng};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tokio::time::Instant;

use crate::common::{TestProject, run_agpm_streaming};

/// Spawns background filesystem churn to create contention during tests.
/// Returns a shutdown handle that stops the churn when dropped.
struct FilesystemChurn {
    shutdown: Arc<AtomicBool>,
    handle: Option<tokio::task::JoinHandle<()>>,
}

impl FilesystemChurn {
    /// Start filesystem churn in the given directory with random delays.
    fn start(base_dir: std::path::PathBuf, min_delay_ms: u64, max_delay_ms: u64) -> Self {
        let shutdown = Arc::new(AtomicBool::new(false));
        let shutdown_clone = Arc::clone(&shutdown);

        // Create a Send-able RNG before spawning
        let mut rng = SmallRng::from_rng(&mut rand::rng());

        let handle = tokio::spawn(async move {
            let mut counter = 0u64;

            while !shutdown_clone.load(Ordering::Relaxed) {
                // Random delay between operations
                let delay = rng.random_range(min_delay_ms..=max_delay_ms);
                tokio::time::sleep(Duration::from_millis(delay)).await;

                // Create a temp file
                let file_path = base_dir.join(format!("churn_{}.tmp", counter));
                if let Ok(()) =
                    tokio::fs::write(&file_path, format!("churn data {}", counter)).await
                {
                    // Random delay before delete
                    let delay = rng.random_range(min_delay_ms..=max_delay_ms);
                    tokio::time::sleep(Duration::from_millis(delay)).await;

                    // Delete the file
                    let _ = tokio::fs::remove_file(&file_path).await;
                }

                counter = counter.wrapping_add(1);
            }
        });

        Self {
            shutdown,
            handle: Some(handle),
        }
    }

    /// Stop the filesystem churn and wait for cleanup.
    async fn stop(mut self) {
        self.shutdown.store(true, Ordering::Relaxed);
        if let Some(handle) = self.handle.take() {
            let _ = handle.await;
        }
    }
}

impl Drop for FilesystemChurn {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Relaxed);
    }
}

/// Chaos test: Pattern dependencies with patches under high parallelism.
///
/// This test creates many pattern-expanded dependencies with patches,
/// runs them with high parallelism, and uses a timeout to detect deadlocks.
/// The pattern expansion + patch application path exercises the conflict
/// tracking code where AB-BA deadlocks can occur.
#[tokio::test]
async fn test_chaos_pattern_patches_high_concurrency() -> Result<()> {
    for iteration in 1..=10 {
        eprintln!("=== Chaos iteration {}/10 ===", iteration);

        let project = TestProject::new().await?;
        let repo = project.create_source_repo("chaos-source").await?;

        // Create many agents that will be pattern-matched
        for i in 0..20 {
            repo.add_resource(
                "agents",
                &format!("chaos-agent-{:02}", i),
                &format!(
                    r#"---
model: default-model
temperature: 0.5
---
# Chaos Agent {:02}

This agent tests concurrent conflict tracking.
"#,
                    i
                ),
            )
            .await?;
        }

        repo.commit_all("Add chaos agents").unwrap();
        repo.tag_version("v1.0.0").unwrap();

        let source_url = repo.bare_file_url(project.sources_path())?;

        // Use pattern dependency with patches - this exercises the full
        // conflict tracking path including variant inputs
        let manifest = format!(
            r#"
[sources]
chaos = "{}"

[agents]
# Pattern dependency - expands to 20 agents processed in parallel
all-chaos = {{ source = "chaos", path = "agents/chaos-*.md", version = "v1.0.0" }}

# Patches applied to pattern-matched resources
[patch.agents.all-chaos]
model = "patched-model"
custom_field = "chaos-test-value"
"#,
            source_url
        );

        project.write_manifest(&manifest).await?;

        // Run with high parallelism and strict timeout
        let start = Instant::now();
        let timeout_duration = Duration::from_secs(30);
        let prefix = format!("chaos-pattern:{}", iteration);

        let install_result = tokio::time::timeout(
            timeout_duration,
            project.run_agpm_async_streaming(&["install", "--max-parallel", "20"], &prefix),
        )
        .await;

        let duration = start.elapsed();

        match install_result {
            Ok(Ok(status)) => {
                assert!(
                    status.success(),
                    "Iteration {}: Install failed with exit code {:?}",
                    iteration,
                    status.code()
                );
                eprintln!("[{}] Iteration {} completed in {:?}", prefix, iteration, duration);
            }
            Ok(Err(e)) => {
                panic!("Iteration {}: Install error: {}", iteration, e);
            }
            Err(_) => {
                panic!(
                    "Iteration {}: DEADLOCK DETECTED - install timed out after {:?}",
                    iteration, timeout_duration
                );
            }
        }

        // Verify all agents were installed with patches
        for i in 0..20 {
            let agent_path =
                project.project_path().join(format!(".claude/agents/agpm/chaos-agent-{:02}.md", i));
            assert!(agent_path.exists(), "Iteration {}: Agent {} not installed", iteration, i);

            // Verify patch was applied
            let content = tokio::fs::read_to_string(&agent_path).await?;
            assert!(
                content.contains("patched-model"),
                "Iteration {}: Patch not applied to agent {}",
                iteration,
                i
            );
        }
    }

    Ok(())
}

/// Chaos test: Multiple pattern dependencies from same source.
///
/// Tests concurrent resolution of multiple overlapping patterns,
/// which stresses the deduplication and conflict tracking logic.
#[tokio::test]
async fn test_chaos_overlapping_patterns() -> Result<()> {
    for iteration in 1..=5 {
        eprintln!("=== Overlapping patterns iteration {}/5 ===", iteration);

        let project = TestProject::new().await?;
        let repo = project.create_source_repo("overlap-source").await?;

        // Create agents with different prefixes
        for prefix in ["alpha", "beta", "gamma"] {
            for i in 0..5 {
                repo.add_resource(
                    "agents",
                    &format!("{}-agent-{}", prefix, i),
                    &format!("# {} Agent {}\n\nTest agent.", prefix, i),
                )
                .await?;
            }
        }

        // Create some agents that match multiple patterns
        for i in 0..3 {
            repo.add_resource(
                "agents",
                &format!("shared-agent-{}", i),
                &format!("# Shared Agent {}\n\nMatches multiple patterns.", i),
            )
            .await?;
        }

        repo.commit_all("Add overlapping agents").unwrap();
        repo.tag_version("v1.0.0").unwrap();

        let source_url = repo.bare_file_url(project.sources_path())?;

        let manifest = format!(
            r#"
[sources]
overlap = "{}"

[agents]
# Multiple patterns that will be resolved concurrently
alpha-agents = {{ source = "overlap", path = "agents/alpha-*.md", version = "v1.0.0" }}
beta-agents = {{ source = "overlap", path = "agents/beta-*.md", version = "v1.0.0" }}
gamma-agents = {{ source = "overlap", path = "agents/gamma-*.md", version = "v1.0.0" }}
shared-agents = {{ source = "overlap", path = "agents/shared-*.md", version = "v1.0.0" }}
all-agents = {{ source = "overlap", path = "agents/*.md", version = "v1.0.0" }}
"#,
            source_url
        );

        project.write_manifest(&manifest).await?;

        let start = Instant::now();
        let timeout_duration = Duration::from_secs(45);
        let prefix = format!("overlap:{}", iteration);

        let install_result = tokio::time::timeout(
            timeout_duration,
            project.run_agpm_async_streaming(&["install", "--max-parallel", "15"], &prefix),
        )
        .await;

        let duration = start.elapsed();

        match install_result {
            Ok(Ok(status)) => {
                assert!(
                    status.success(),
                    "Iteration {}: Install failed with exit code {:?}",
                    iteration,
                    status.code()
                );
                eprintln!("[{}] Iteration {} completed in {:?}", prefix, iteration, duration);
            }
            Ok(Err(e)) => {
                panic!("Iteration {}: Install error: {}", iteration, e);
            }
            Err(_) => {
                panic!(
                    "Iteration {}: DEADLOCK DETECTED - install timed out after {:?}",
                    iteration, timeout_duration
                );
            }
        }
    }

    Ok(())
}

/// Chaos test: Rapid repeated installs to stress caching and state management.
#[tokio::test]
async fn test_chaos_rapid_reinstalls() -> Result<()> {
    let project = TestProject::new().await?;
    let repo = project.create_source_repo("rapid-source").await?;

    for i in 0..10 {
        repo.add_resource(
            "agents",
            &format!("rapid-agent-{}", i),
            &format!("# Rapid Agent {}\n\nTest.", i),
        )
        .await?;
    }

    repo.commit_all("Add rapid agents").unwrap();
    repo.tag_version("v1.0.0").unwrap();

    let source_url = repo.bare_file_url(project.sources_path())?;

    let manifest = format!(
        r#"
[sources]
rapid = "{}"

[agents]
all = {{ source = "rapid", path = "agents/rapid-*.md", version = "v1.0.0" }}

[patch.agents.all]
patched = true
"#,
        source_url
    );

    project.write_manifest(&manifest).await?;

    // Debug: print the project directory and manifest
    eprintln!("[rapid] Project dir: {:?}", project.project_path());
    eprintln!("[rapid] Cache dir: {:?}", project.cache_path());
    eprintln!("[rapid] Manifest:\n{}", manifest);

    // Rapid repeated installs - each should complete quickly
    // Use 60 second timeout to allow lock timeouts (8s in test mode) to trigger and dump state
    let timeout_duration = Duration::from_secs(60);

    for iteration in 1..=20 {
        let start = Instant::now();
        let prefix = format!("rapid:{}", iteration);

        let install_result = tokio::time::timeout(
            timeout_duration,
            project.run_agpm_async_streaming(&["install", "--max-parallel", "10"], &prefix),
        )
        .await;

        let duration = start.elapsed();

        match install_result {
            Ok(Ok(status)) => {
                eprintln!(
                    "[{}] Iteration {}: exit={:?} duration={:?}",
                    prefix,
                    iteration,
                    status.code(),
                    duration
                );
                assert!(
                    status.success(),
                    "Iteration {}: Install failed with exit code {:?}",
                    iteration,
                    status.code()
                );
                if iteration % 5 == 0 {
                    eprintln!(
                        "[{}] Rapid install {}/20 completed in {:?}",
                        prefix, iteration, duration
                    );
                }
            }
            Ok(Err(e)) => {
                panic!("Iteration {}: Install error: {}", iteration, e);
            }
            Err(_) => {
                panic!(
                    "Iteration {}: DEADLOCK DETECTED - install timed out after {:?}.\n\
                     Lock timeouts should have triggered dump_lock_state() before this.",
                    iteration, timeout_duration
                );
            }
        }
    }

    Ok(())
}

/// Chaos test: Install under filesystem contention with random delays.
///
/// This test spawns background tasks that create filesystem churn (random
/// file creation/deletion with delays) while running install operations.
/// This helps expose race conditions that only manifest under I/O latency.
#[tokio::test]
async fn test_chaos_filesystem_contention() -> Result<()> {
    for iteration in 1..=5 {
        eprintln!("=== Filesystem contention iteration {}/5 ===", iteration);

        let project = TestProject::new().await?;
        let repo = project.create_source_repo("contention-source").await?;

        // Create agents
        for i in 0..15 {
            repo.add_resource(
                "agents",
                &format!("contention-agent-{:02}", i),
                &format!(
                    r#"---
model: base-model
---
# Contention Agent {:02}

Tests filesystem contention handling.
"#,
                    i
                ),
            )
            .await?;
        }

        repo.commit_all("Add contention agents").unwrap();
        repo.tag_version("v1.0.0").unwrap();

        let source_url = repo.bare_file_url(project.sources_path())?;

        let manifest = format!(
            r#"
[sources]
contention = "{}"

[agents]
all = {{ source = "contention", path = "agents/contention-*.md", version = "v1.0.0" }}

[patch.agents.all]
model = "patched-model"
"#,
            source_url
        );

        project.write_manifest(&manifest).await?;

        // Start filesystem churn in multiple locations
        let churn_dir = project.project_path().join(".claude");
        tokio::fs::create_dir_all(&churn_dir).await?;

        // Multiple churn tasks with different delay profiles
        let churn_fast = FilesystemChurn::start(churn_dir.clone(), 1, 10);
        let churn_slow = FilesystemChurn::start(churn_dir.clone(), 50, 200);

        let start = Instant::now();
        let timeout_duration = Duration::from_secs(45);
        let prefix = format!("fs-contention:{}", iteration);

        let install_result = tokio::time::timeout(
            timeout_duration,
            project.run_agpm_async_streaming(&["install", "--max-parallel", "15"], &prefix),
        )
        .await;

        // Stop churn before checking results
        churn_fast.stop().await;
        churn_slow.stop().await;

        let duration = start.elapsed();

        match install_result {
            Ok(Ok(status)) => {
                assert!(
                    status.success(),
                    "Iteration {}: Install failed under contention with exit code {:?}",
                    iteration,
                    status.code()
                );
                eprintln!(
                    "[{}] Iteration {} completed under contention in {:?}",
                    prefix, iteration, duration
                );
            }
            Ok(Err(e)) => {
                panic!("Iteration {}: Install error under contention: {}", iteration, e);
            }
            Err(_) => {
                panic!(
                    "Iteration {}: DEADLOCK DETECTED under filesystem contention after {:?}",
                    iteration, timeout_duration
                );
            }
        }

        // Verify installation succeeded
        for i in 0..15 {
            let agent_path = project
                .project_path()
                .join(format!(".claude/agents/agpm/contention-agent-{:02}.md", i));
            assert!(
                agent_path.exists(),
                "Iteration {}: Agent {} not installed under contention",
                iteration,
                i
            );
        }
    }

    Ok(())
}

/// Chaos test: Concurrent installs with staggered delays.
///
/// Spawns multiple install operations with random start delays to maximize
/// the chance of hitting race conditions in lock acquisition.
#[tokio::test]
async fn test_chaos_staggered_concurrent_installs() -> Result<()> {
    let project = TestProject::new().await?;
    let repo = project.create_source_repo("stagger-source").await?;

    for i in 0..8 {
        repo.add_resource(
            "agents",
            &format!("stagger-agent-{}", i),
            &format!("# Stagger Agent {}\n\nTest.", i),
        )
        .await?;
    }

    repo.commit_all("Add stagger agents").unwrap();
    repo.tag_version("v1.0.0").unwrap();

    let source_url = repo.bare_file_url(project.sources_path())?;

    let manifest = format!(
        r#"
[sources]
stagger = "{}"

[agents]
all = {{ source = "stagger", path = "agents/stagger-*.md", version = "v1.0.0" }}
"#,
        source_url
    );

    project.write_manifest(&manifest).await?;

    // Run 10 iterations with concurrent installs per iteration
    for iteration in 1..=10 {
        eprintln!("=== Staggered iteration {}/10 ===", iteration);

        let mut rng = SmallRng::from_rng(&mut rand::rng());

        // Spawn 3 concurrent install processes with staggered delays
        let mut handles = Vec::new();

        for task_id in 0..3 {
            let delay_ms: u64 = rng.random_range(0..50);
            let project_path = project.project_path().to_path_buf();
            let cache_dir = project.cache_path().to_path_buf();
            let prefix = format!("staggered:{}:{}", iteration, task_id);

            let handle = tokio::spawn(async move {
                // Stagger the start
                tokio::time::sleep(Duration::from_millis(delay_ms)).await;

                // Run install with prefixed output streaming
                let status = run_agpm_streaming(
                    &["install", "--max-parallel", "8"],
                    &prefix,
                    &project_path,
                    &cache_dir,
                )
                .await;

                (task_id, delay_ms, status)
            });

            handles.push(handle);
        }

        let timeout_duration = Duration::from_secs(60);

        let results =
            tokio::time::timeout(timeout_duration, futures::future::join_all(handles)).await;

        match results {
            Ok(task_results) => {
                let mut success_count = 0;
                for result in task_results {
                    if let Ok((task_id, delay_ms, Ok(status))) = result {
                        eprintln!(
                            "[staggered:{}:{}] Task {} (delay {}ms): exit={:?}",
                            iteration,
                            task_id,
                            task_id,
                            delay_ms,
                            status.code()
                        );
                        if status.success() {
                            success_count += 1;
                        }
                    }
                }
                // At least one install should succeed
                assert!(success_count >= 1, "Iteration {}: No installs succeeded", iteration);
                eprintln!(
                    "[staggered:{}] Iteration {}: {}/3 concurrent installs succeeded",
                    iteration, iteration, success_count
                );
            }
            Err(_) => {
                // Timeout - with kill_on_drop(true), the child processes will be
                // killed when the JoinHandles are dropped
                panic!(
                    "Iteration {}: DEADLOCK DETECTED in staggered installs after {:?}",
                    iteration, timeout_duration
                );
            }
        }
    }

    Ok(())
}
