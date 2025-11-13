//! Integration tests for Git tag caching in version resolver
//!
//! Tests per-instance tag caching feature (v0.4.11+) that caches
//! tags after first `list_tags()` call to improve performance.
//!
//! Key optimizations tested:
//! - Per-instance tag caching using OnceLock<Vec<String>>
//! - Performance improvement from cached tag access
//! - Cache isolation between GitRepo instances
//! - Integration with VersionResolutionService

use agpm_cli::git::GitRepo;
use agpm_cli::test_utils::init_test_logging;
use anyhow::Result;
use serial_test::serial;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use tempfile::TempDir;
use tokio::fs;

/// Helper: Creates a test repository with specified number of tags
async fn create_repo_with_tags(
    base_dir: &Path,
    repo_name: &str,
    tag_count: usize,
) -> Result<PathBuf> {
    let repo_path = base_dir.join(repo_name);
    fs::create_dir_all(&repo_path).await?;

    // Initialize git repository
    let git = |args: &[&str]| {
        let mut cmd = tokio::process::Command::new("git");
        cmd.args(args).current_dir(&repo_path);
        cmd
    };

    // Initial setup
    git(&["init", "--bare"]).output().await?;
    git(&["config", "user.email", "test@example.com"]).output().await?;
    git(&["config", "user.name", "Test User"]).output().await?;

    // Create tags in a temporary worktree
    let temp_worktree = base_dir.join(format!("{}_temp", repo_name));
    fs::create_dir_all(&temp_worktree).await?;

    // Clone bare repo to worktree temporarily
    git(&["clone", &repo_path.to_string_lossy(), &temp_worktree.to_string_lossy()])
        .output()
        .await?;

    // Create initial commit
    let readme_path = temp_worktree.join("README.md");
    fs::write(&readme_path, "# Test Repository\n").await?;
    let git_worktree = |args: &[&str]| {
        let mut cmd = tokio::process::Command::new("git");
        cmd.args(args).current_dir(&temp_worktree);
        cmd
    };
    git_worktree(&["add", "README.md"]).output().await?;
    git_worktree(&["commit", "-m", "Initial commit"]).output().await?;

    // Create tags
    for i in 0..tag_count {
        let tag = format!("v{}.0.0", i + 1);

        // Update README for each tag
        fs::write(&readme_path, format!("# Test Repository - Tag {}\n", tag)).await?;
        git_worktree(&["add", "README.md"]).output().await?;
        git_worktree(&["commit", "-m", &format!("Version {}", tag)]).output().await?;

        // Create tag
        git_worktree(&["tag", "-a", &tag, "-m", &format!("Release {}", tag)]).output().await?;
    }

    // Push tags to bare repository
    git_worktree(&["push", "origin", "--tags"]).output().await?;

    // Clean up temporary worktree
    fs::remove_dir_all(&temp_worktree).await?;

    Ok(repo_path)
}

/// Helper: Measures performance of tag listing
async fn measure_tag_listing_performance(repo: &GitRepo, num_calls: usize) -> Vec<Duration> {
    let mut durations = Vec::new();

    for _ in 0..num_calls {
        let start = Instant::now();
        let _ = repo.list_tags().await;
        let duration = start.elapsed();
        durations.push(duration);
    }

    durations
}

/// Helper: Asserts that caching provides meaningful performance improvement
fn assert_cache_effectiveness(first_times: &[Duration], subsequent_times: &[Duration]) {
    if first_times.is_empty() || subsequent_times.is_empty() {
        return;
    }

    let avg_first = first_times.iter().sum::<Duration>() / first_times.len() as u32;
    let avg_subsequent = subsequent_times.iter().sum::<Duration>() / subsequent_times.len() as u32;

    // Cache should provide at least 5x improvement
    let improvement_factor = avg_first.as_millis() as f64 / avg_subsequent.as_millis() as f64;

    println!("🏷️  Tag Caching Performance:");
    println!("   First call average: {:?} ({})", avg_first, avg_first.as_millis());
    println!("   Cached call average: {:?} ({})", avg_subsequent, avg_subsequent.as_millis());
    println!("   Improvement factor: {:.1}x", improvement_factor);

    assert!(
        improvement_factor > 5.0,
        "Cache should provide at least 5x performance improvement, got {:.1}x",
        improvement_factor
    );
}

/// Test: Git tag caching performance improvement
///
/// Creates repository with 100+ tags and verifies that:
/// 1. First list_tags() call is slower (fetches from git)
/// 2. Subsequent calls use cache (much faster)
/// 3. Performance improvement is measurable and significant
#[tokio::test]
#[serial]
async fn test_git_tag_caching_performance() -> Result<()> {
    init_test_logging(None);

    let temp_dir = TempDir::new()?;
    let repo_path = create_repo_with_tags(temp_dir.path(), "tag_cache_test", 100).await?;

    let repo = GitRepo::new(&repo_path);

    println!("🚀 Testing tag caching performance with 100 tags");

    // First call - should fetch tags (slow)
    let first_duration = {
        let start = Instant::now();
        let tags = repo.list_tags().await?;
        let duration = start.elapsed();

        println!("   First call: {:?} (found {} tags)", duration, tags.len());
        assert_eq!(tags.len(), 100, "Should have 100 tags");

        duration
    };

    // Second call - should use cache (fast)
    let second_duration = {
        let start = Instant::now();
        let tags = repo.list_tags().await?;
        let duration = start.elapsed();

        println!("   Second call: {:?} (found {} tags)", duration, tags.len());
        assert_eq!(tags.len(), 100, "Should still have 100 tags");

        duration
    };

    // Multiple subsequent calls to verify consistency
    let subsequent_durations = measure_tag_listing_performance(&repo, 5).await;

    // Verify cache effectiveness
    assert!(second_duration < first_duration, "Cached call should be faster than first call");
    assert!(second_duration.as_millis() < 50, "Cached call should be very fast (<50ms)");

    // All subsequent calls should be equally fast
    for (i, &duration) in subsequent_durations.iter().enumerate() {
        assert!(duration < first_duration, "Subsequent call {} should be cached and faster", i + 1);
        assert!(duration.as_millis() < 50, "Cached call {} should be very fast (<50ms)", i + 1);
    }

    let first_times = vec![first_duration];
    let subsequent_times =
        std::iter::once(second_duration).chain(subsequent_durations).collect::<Vec<_>>();
    assert_cache_effectiveness(&first_times, &subsequent_times);

    Ok(())
}

/// Test: Tag cache isolation between GitRepo instances
///
/// Verifies that:
/// 1. Separate GitRepo instances maintain independent caches
/// 2. Cache in one instance doesn't affect other
/// 3. Each instance shows performance benefits on its own second calls
#[tokio::test]
#[serial]
async fn test_tag_cache_isolation() -> Result<()> {
    init_test_logging(None);

    let temp_dir = TempDir::new()?;
    let repo_path = create_repo_with_tags(temp_dir.path(), "isolation_test", 50).await?;

    // Create two separate GitRepo instances
    let repo1 = GitRepo::new(&repo_path);
    let repo2 = GitRepo::new(&repo_path);

    println!("🔒 Testing tag cache isolation between instances");

    // First call on repo1 - should fetch tags
    let repo1_first = {
        let start = Instant::now();
        let tags = repo1.list_tags().await?;
        let duration = start.elapsed();

        println!("   Repo1 first call: {:?} ({} tags)", duration, tags.len());
        assert_eq!(tags.len(), 50);
        duration
    };

    // First call on repo2 - should also fetch tags (independent cache)
    let repo2_first = {
        let start = Instant::now();
        let tags = repo2.list_tags().await?;
        let duration = start.elapsed();

        println!("   Repo2 first call: {:?} ({} tags)", duration, tags.len());
        assert_eq!(tags.len(), 50);
        duration
    };

    // Second call on repo1 - should use cache
    let repo1_second = {
        let start = Instant::now();
        let tags = repo1.list_tags().await?;
        let duration = start.elapsed();

        println!("   Repo1 second call: {:?} ({} tags)", duration, tags.len());
        assert_eq!(tags.len(), 50);
        duration
    };

    // Second call on repo2 - should also use cache independently
    let repo2_second = {
        let start = Instant::now();
        let tags = repo2.list_tags().await?;
        let duration = start.elapsed();

        println!("   Repo2 second call: {:?} ({} tags)", duration, tags.len());
        assert_eq!(tags.len(), 50);
        duration
    };

    // Verify cache isolation and effectiveness
    assert!(repo1_second < repo1_first, "Repo1 should cache tags");
    assert!(repo2_second < repo2_first, "Repo2 should cache tags independently");

    // Both instances should have fast cached calls
    assert!(repo1_second.as_millis() < 50, "Repo1 cached call should be fast");
    assert!(repo2_second.as_millis() < 50, "Repo2 cached call should be fast");

    // Both first calls should be slower than cached calls (fetching from git)
    assert!(repo1_first.as_millis() > 1, "Repo1 first call should fetch from git");
    assert!(repo2_first.as_millis() > 1, "Repo2 first call should fetch from git");

    // Verify cache isolation by checking that both instances get correct tags
    let tags1_cached = repo1.list_tags().await?;
    let tags2_cached = repo2.list_tags().await?;
    assert_eq!(tags1_cached, tags2_cached, "Both instances should return same tags");

    println!("   ✅ Cache isolation verified - each instance maintains independent cache");

    Ok(())
}

/// Test: Tag caching integration with higher-level resolver operations
///
/// Tests that tag caching benefit when using GitRepo directly
/// in a context similar to version resolution.
#[tokio::test]
#[serial]
async fn test_tag_caching_integration_scenario() -> Result<()> {
    init_test_logging(None);

    let temp_dir = TempDir::new()?;
    let repo_path = create_repo_with_tags(temp_dir.path(), "integration_test", 75).await?;

    let repo = GitRepo::new(&repo_path);

    println!("🔗 Testing tag caching integration scenario");

    // Simulate version resolution workflow: multiple tag list operations
    // like what would happen during constraint resolution

    // Phase 1: Initial discovery (slow - should fetch tags)
    let discovery_duration = {
        let start = Instant::now();
        let all_tags = repo.list_tags().await?;
        let duration = start.elapsed();

        println!("   Initial tag discovery: {:?} (found {} tags)", duration, all_tags.len());
        assert_eq!(all_tags.len(), 75);
        duration
    };

    // Phase 2: Multiple constraint resolutions (fast - should use cache)
    let constraint_checks = vec!["^1.0.0", "^10.0.0", "^25.0.0", "^50.0.0", "^75.0.0"];
    let mut constraint_durations = Vec::new();

    for constraint in constraint_checks {
        let start = Instant::now();
        let tags = repo.list_tags().await?;
        let duration = start.elapsed();

        // Simulate constraint resolution (find matching tags)
        let constraint_clean = constraint.trim_start_matches('^');
        let matching_tags: Vec<_> = tags
            .iter()
            .filter(|tag| {
                // For "^1.0.0" constraint, we want to match "v1.0.0", "v1.0.1", etc.
                // So we append 'v' to the constraint and check for prefix match
                let search_pattern = if constraint_clean.ends_with(".0.0") {
                    // Semantic version constraint like "^1.0.0" -> look for "v1.0.0"
                    constraint_clean.replace("1.0.0", "1.0.")
                } else {
                    constraint_clean.to_string()
                };
                tag.starts_with(&format!("v{}", search_pattern))
            })
            .collect();

        constraint_durations.push(duration);
        println!(
            "   Constraint {}: {:?} (found {} matches)",
            constraint,
            duration,
            matching_tags.len()
        );
        if matching_tags.is_empty() {
            // Debug output to help understand the issue
            println!("   Available tags (first 5): {:?}", &tags.iter().take(5).collect::<Vec<_>>());
            println!("   Search pattern: v{}", constraint_clean.replace("1.0.0", "1.0."));
        }
        assert!(
            !matching_tags.is_empty(),
            "Should find matches for {} (looking for prefix: {})",
            constraint,
            constraint_clean
        );
    }

    // Phase 3: Final validation (still fast)
    let final_duration = {
        let start = Instant::now();
        let tags = repo.list_tags().await?;
        let duration = start.elapsed();

        println!("   Final validation: {:?} (verified {} tags)", duration, tags.len());
        assert_eq!(tags.len(), 75);
        duration
    };

    // Verify caching effectiveness
    assert!(discovery_duration.as_millis() > 1, "Initial discovery should take time");

    for (i, &duration) in constraint_durations.iter().enumerate() {
        assert!(
            duration < discovery_duration,
            "Constraint check {} should be cached and faster",
            i + 1
        );
        assert!(duration.as_millis() < 50, "Constraint check {} should be very fast", i + 1);
    }

    assert!(final_duration < discovery_duration, "Final validation should be cached and faster");

    let avg_cached_time =
        constraint_durations.iter().sum::<Duration>() / constraint_durations.len() as u32;
    let improvement_factor =
        discovery_duration.as_millis() as f64 / avg_cached_time.as_millis() as f64;

    println!("   Discovery time: {:?}", discovery_duration);
    println!("   Average cached time: {:?}", avg_cached_time);
    println!("   Performance improvement: {:.1}x", improvement_factor);

    assert!(
        improvement_factor > 5.0,
        "Caching should provide at least 5x performance improvement, got {:.1}x",
        improvement_factor
    );

    println!(
        "   ✅ Tag caching integration verified with {:.1}x performance improvement",
        improvement_factor
    );

    Ok(())
}
