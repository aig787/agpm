//! Simplified integration tests for Git worktree functionality

use ccpm::cache::Cache;
use std::sync::Arc;
use tempfile::TempDir;

/// Helper to create a test git repository
async fn setup_test_repo(path: &std::path::Path, name: &str) -> anyhow::Result<()> {
    // Initialize git repo
    std::process::Command::new("git")
        .args(["init"])
        .current_dir(path)
        .output()?;

    // Configure git
    std::process::Command::new("git")
        .args(["config", "user.email", "test@example.com"])
        .current_dir(path)
        .output()?;

    std::process::Command::new("git")
        .args(["config", "user.name", "Test User"])
        .current_dir(path)
        .output()?;

    // Create initial commit
    std::fs::write(path.join("README.md"), format!("# {}", name))?;
    std::process::Command::new("git")
        .args(["add", "."])
        .current_dir(path)
        .output()?;
    std::process::Command::new("git")
        .args(["commit", "-m", "Initial commit"])
        .current_dir(path)
        .output()?;

    // Create v1.0.0 tag
    std::process::Command::new("git")
        .args(["tag", "v1.0.0"])
        .current_dir(path)
        .output()?;

    // Second commit and tag
    std::fs::write(path.join("file.txt"), "v2 content")?;
    std::process::Command::new("git")
        .args(["add", "."])
        .current_dir(path)
        .output()?;
    std::process::Command::new("git")
        .args(["commit", "-m", "Version 2"])
        .current_dir(path)
        .output()?;
    std::process::Command::new("git")
        .args(["tag", "v2.0.0"])
        .current_dir(path)
        .output()?;

    Ok(())
}

#[tokio::test]
async fn test_worktree_basic_operations() {
    let cache_dir = TempDir::new().unwrap();
    let cache = Cache::with_dir(cache_dir.path().to_path_buf()).unwrap();

    let repo_dir = TempDir::new().unwrap();
    setup_test_repo(repo_dir.path(), "test-repo").await.unwrap();

    let repo_url = format!("file://{}", repo_dir.path().display());

    // Get worktree for v1.0.0
    let worktree1 = cache
        .get_or_clone_source_worktree("test-repo", &repo_url, Some("v1.0.0"))
        .await
        .unwrap();

    assert!(worktree1.exists());
    assert!(worktree1.join(".git").exists());

    // Get worktree for v2.0.0
    let worktree2 = cache
        .get_or_clone_source_worktree("test-repo", &repo_url, Some("v2.0.0"))
        .await
        .unwrap();

    assert!(worktree2.exists());
    assert_ne!(worktree1, worktree2); // Different worktrees

    // Cleanup
    cache.cleanup_worktree(&worktree1).await.unwrap();
    cache.cleanup_worktree(&worktree2).await.unwrap();
}

#[tokio::test]
async fn test_worktree_with_context() {
    let cache_dir = TempDir::new().unwrap();
    let cache = Cache::with_dir(cache_dir.path().to_path_buf()).unwrap();

    let repo_dir = TempDir::new().unwrap();
    setup_test_repo(repo_dir.path(), "test-repo").await.unwrap();

    let repo_url = format!("file://{}", repo_dir.path().display());

    // Test with context for better logging
    let worktree = cache
        .get_or_clone_source_worktree_with_context(
            "test-repo",
            &repo_url,
            Some("v1.0.0"),
            Some("my-dependency"),
        )
        .await
        .unwrap();

    assert!(worktree.exists());

    // Cleanup
    cache.cleanup_worktree(&worktree).await.unwrap();
}

#[tokio::test]
async fn test_worktree_parallel_access() {
    let cache_dir = TempDir::new().unwrap();
    let cache = Arc::new(Cache::with_dir(cache_dir.path().to_path_buf()).unwrap());

    let repo_dir = TempDir::new().unwrap();
    setup_test_repo(repo_dir.path(), "parallel-repo")
        .await
        .unwrap();

    let repo_url = format!("file://{}", repo_dir.path().display());

    // Spawn multiple tasks requesting different versions concurrently
    let mut handles = vec![];
    for i in 0..4 {
        let cache = cache.clone();
        let url = repo_url.clone();
        let version = if i % 2 == 0 { "v1.0.0" } else { "v2.0.0" };

        let handle = tokio::spawn(async move {
            cache
                .get_or_clone_source_worktree(&format!("parallel-repo-{}", i), &url, Some(version))
                .await
        });
        handles.push(handle);
    }

    // All should succeed without conflicts
    let mut worktrees = vec![];
    for handle in handles {
        let result = handle.await.unwrap();
        assert!(result.is_ok());
        worktrees.push(result.unwrap());
    }

    // Verify all worktrees exist
    for worktree in &worktrees {
        assert!(worktree.exists());
    }

    // Cleanup
    for worktree in worktrees {
        cache.cleanup_worktree(&worktree).await.unwrap();
    }
}

#[tokio::test]
async fn test_worktree_cleanup_lifecycle() {
    let cache_dir = TempDir::new().unwrap();
    let cache = Cache::with_dir(cache_dir.path().to_path_buf()).unwrap();

    let repo_dir = TempDir::new().unwrap();
    setup_test_repo(repo_dir.path(), "cleanup-repo")
        .await
        .unwrap();

    let repo_url = format!("file://{}", repo_dir.path().display());

    // Create worktree
    let worktree = cache
        .get_or_clone_source_worktree("cleanup-repo", &repo_url, Some("v1.0.0"))
        .await
        .unwrap();

    assert!(worktree.exists());

    // Clean up worktree
    cache.cleanup_worktree(&worktree).await.unwrap();

    // Verify it's removed
    assert!(!worktree.exists());

    // Cleanup should be idempotent
    cache.cleanup_worktree(&worktree).await.unwrap();
}

#[tokio::test]
async fn test_cleanup_all_worktrees() {
    let cache_dir = TempDir::new().unwrap();
    let cache = Cache::with_dir(cache_dir.path().to_path_buf()).unwrap();

    let repo_dir = TempDir::new().unwrap();
    setup_test_repo(repo_dir.path(), "cleanup-all-repo")
        .await
        .unwrap();

    let repo_url = format!("file://{}", repo_dir.path().display());

    // Create multiple worktrees
    let worktree1 = cache
        .get_or_clone_source_worktree("cleanup-all-1", &repo_url, Some("v1.0.0"))
        .await
        .unwrap();

    let worktree2 = cache
        .get_or_clone_source_worktree("cleanup-all-2", &repo_url, Some("v2.0.0"))
        .await
        .unwrap();

    assert!(worktree1.exists());
    assert!(worktree2.exists());

    // Clean up all worktrees at once
    cache.cleanup_all_worktrees().await.unwrap();

    // Verify all are removed
    assert!(!worktree1.exists());
    assert!(!worktree2.exists());
}
