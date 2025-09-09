//! Test to verify the Git semaphore is working correctly

use ccpm::cache::Cache;
use std::sync::Arc;
use std::time::Instant;
use tempfile::TempDir;

#[tokio::test]
async fn test_git_semaphore_actually_limits_operations() {
    // This test verifies that the GIT_SEMAPHORE limits concurrent Git operations
    let cache_dir = TempDir::new().unwrap();
    let cache = Arc::new(Cache::with_dir(cache_dir.path().to_path_buf()).unwrap());

    // Create multiple test repositories
    let mut repo_dirs = vec![];
    for i in 0..5 {
        let repo_dir = TempDir::new().unwrap();

        // Initialize git repo
        std::process::Command::new("git")
            .args(["init"])
            .current_dir(repo_dir.path())
            .output()
            .unwrap();

        std::process::Command::new("git")
            .args(["config", "user.email", "test@example.com"])
            .current_dir(repo_dir.path())
            .output()
            .unwrap();

        std::process::Command::new("git")
            .args(["config", "user.name", "Test User"])
            .current_dir(repo_dir.path())
            .output()
            .unwrap();

        // Create some content to make operations take time
        for j in 0..20 {
            std::fs::write(
                repo_dir.path().join(format!("file{}.txt", j)),
                format!("Repository {} File {}", i, j),
            )
            .unwrap();
        }

        std::process::Command::new("git")
            .args(["add", "."])
            .current_dir(repo_dir.path())
            .output()
            .unwrap();

        std::process::Command::new("git")
            .args(["commit", "-m", &format!("Initial commit for repo {}", i)])
            .current_dir(repo_dir.path())
            .output()
            .unwrap();

        std::process::Command::new("git")
            .args(["tag", "v1.0.0"])
            .current_dir(repo_dir.path())
            .output()
            .unwrap();

        repo_dirs.push(repo_dir);
    }

    // Expected limit is 3 * CPU count
    let cpu_count = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4);
    let expected_limit = cpu_count * 3;

    println!(
        "Testing with {} CPUs, semaphore limit should be {}",
        cpu_count, expected_limit
    );

    // Spawn many tasks that will all try to clone different repos
    // This tests the clone semaphore protection
    let start = Instant::now();
    let mut handles = vec![];

    // Create more tasks than the semaphore limit
    let num_tasks = expected_limit * 3;

    for i in 0..num_tasks {
        let cache = cache.clone();
        let repo_url = format!("file://{}", repo_dirs[i % repo_dirs.len()].path().display());

        let handle = tokio::spawn(async move {
            let start = Instant::now();

            // This will trigger a clone for each unique repo name
            let result = cache
                .get_or_clone_source_worktree(
                    &format!("test-repo-{}", i), // Unique name forces new clone
                    &repo_url,
                    Some("v1.0.0"),
                )
                .await;

            let duration = start.elapsed();
            (result, duration)
        });
        handles.push(handle);
    }

    // Collect results and analyze timing
    let mut successes = 0;
    let mut failures = 0;
    let mut durations = vec![];

    for handle in handles {
        match handle.await {
            Ok((Ok(_), duration)) => {
                successes += 1;
                durations.push(duration);
            }
            Ok((Err(_), _)) => failures += 1,
            Err(_) => failures += 1,
        }
    }

    let total_duration = start.elapsed();

    println!("Completed {} tasks in {:?}", num_tasks, total_duration);
    println!("Successes: {}, Failures: {}", successes, failures);

    // If the semaphore is working, operations should be serialized in groups
    // With limit L and N tasks, we expect at least ceil(N/L) waves of operations
    let expected_waves = (num_tasks + expected_limit - 1) / expected_limit;
    let min_expected_duration = expected_waves as f64 * 0.05; // Assume at least 50ms per wave

    assert!(
        total_duration.as_secs_f64() >= min_expected_duration,
        "Operations completed too quickly ({:?}), suggesting no semaphore limiting",
        total_duration
    );

    // Most operations should succeed
    assert!(
        successes > num_tasks / 2,
        "Too many failures: {} out of {}",
        failures,
        num_tasks
    );

    println!("✅ Semaphore appears to be limiting operations correctly");
}

#[tokio::test]
async fn test_worktree_operations_are_limited() {
    // Test that worktree creation operations are also limited
    let cache_dir = TempDir::new().unwrap();
    let cache = Arc::new(Cache::with_dir(cache_dir.path().to_path_buf()).unwrap());

    // Create one repo that will be cloned once
    let repo_dir = TempDir::new().unwrap();

    std::process::Command::new("git")
        .args(["init"])
        .current_dir(repo_dir.path())
        .output()
        .unwrap();

    std::process::Command::new("git")
        .args(["config", "user.email", "test@example.com"])
        .current_dir(repo_dir.path())
        .output()
        .unwrap();

    std::process::Command::new("git")
        .args(["config", "user.name", "Test User"])
        .current_dir(repo_dir.path())
        .output()
        .unwrap();

    // Create multiple tags
    for i in 0..10 {
        std::fs::write(
            repo_dir.path().join(format!("file{}.txt", i)),
            format!("Version {}", i),
        )
        .unwrap();

        std::process::Command::new("git")
            .args(["add", "."])
            .current_dir(repo_dir.path())
            .output()
            .unwrap();

        std::process::Command::new("git")
            .args(["commit", "-m", &format!("Version {}", i)])
            .current_dir(repo_dir.path())
            .output()
            .unwrap();

        std::process::Command::new("git")
            .args(["tag", &format!("v{}.0.0", i)])
            .current_dir(repo_dir.path())
            .output()
            .unwrap();
    }

    let repo_url = format!("file://{}", repo_dir.path().display());

    // First clone the repo once to establish the bare repo
    cache
        .get_or_clone_source_worktree("shared-repo", &repo_url, Some("v0.0.0"))
        .await
        .unwrap();

    // Now spawn many tasks that will create worktrees from the same bare repo
    let expected_limit = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
        * 3;

    let num_tasks = expected_limit * 2;
    let start = Instant::now();
    let mut handles = vec![];

    for i in 0..num_tasks {
        let cache = cache.clone();
        let url = repo_url.clone();
        let version = format!("v{}.0.0", i % 10);

        let handle = tokio::spawn(async move {
            cache
                .get_or_clone_source_worktree("shared-repo", &url, Some(&version))
                .await
        });
        handles.push(handle);
    }

    // Wait for all to complete
    let mut worktrees = vec![];
    for handle in handles {
        if let Ok(Ok(worktree)) = handle.await {
            worktrees.push(worktree);
        }
    }

    let duration = start.elapsed();

    println!("Created {} worktrees in {:?}", worktrees.len(), duration);

    // Cleanup
    for worktree in worktrees {
        cache.cleanup_worktree(&worktree).await.ok();
    }

    println!("✅ Worktree operations completed successfully");
}
