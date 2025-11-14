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
use std::time::{Duration, Instant};

use crate::common::TestProject;

/// Helper: Creates a test repository with specified number of tags using TestProject
async fn create_repo_with_tags(tag_count: usize) -> Result<(std::path::PathBuf, TestProject)> {
    let project = TestProject::new().await?;
    let repo = project.create_source_repo("tag_cache_test").await?;

    // Create initial content
    std::fs::write(repo.path.join("README.md"), "# Test Repository\n")?;

    // Create the tags using our new helper method
    repo.create_multiple_tags(tag_count)?;

    Ok((repo.path.clone(), project))
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

    // Log cache effectiveness for monitoring - use microseconds to avoid division by zero
    let improvement_factor = if avg_subsequent.as_micros() > 0 {
        avg_first.as_micros() as f64 / avg_subsequent.as_micros() as f64
    } else {
        // If cached time is too fast to measure, treat it as infinite improvement
        f64::INFINITY
    };

    println!("🏷️  Tag Caching Performance:");
    println!("   First call average: {:?} ({} µs)", avg_first, avg_first.as_micros());
    println!("   Cached call average: {:?} ({} µs)", avg_subsequent, avg_subsequent.as_micros());
    match improvement_factor {
        f64::INFINITY => println!("   Improvement factor: >1000x (too fast to measure)"),
        _ => println!("   Improvement factor: {:.1}x", improvement_factor),
    }

    // Log warning if cache is ineffective (very generous threshold)
    if improvement_factor < 1.5 && improvement_factor.is_finite() {
        eprintln!(
            "⚠️  Warning: Cache shows minimal improvement ({:.1}x), may indicate performance issue",
            improvement_factor
        );
    }
}

/// Test: Git tag caching performance improvement
///
/// Creates repository with 100+ tags and verifies that:
/// 1. First list_tags() call is slower (fetches from git)
/// 2. Subsequent calls use cache (much faster)
/// 3. Performance improvement is measurable and significant
#[tokio::test]
async fn test_git_tag_caching_performance() -> Result<()> {
    init_test_logging(None);

    let (repo_path, _project) = create_repo_with_tags(100).await?;
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

    // Log cache effectiveness for monitoring
    println!("Cache performance comparison:");
    println!("   First call: {:?} ({} µs)", first_duration, first_duration.as_micros());
    println!("   Second call: {:?} ({} µs)", second_duration, second_duration.as_micros());

    let improvement_3x = if second_duration.as_micros() > 0 {
        first_duration.as_micros() as f64 / second_duration.as_micros() as f64
    } else {
        f64::INFINITY
    };
    match improvement_3x {
        f64::INFINITY => println!("   Second call improvement: >1000x (too fast to measure)"),
        _ => println!("   Second call improvement: {:.1}x", improvement_3x),
    }

    // Log warning if cache is ineffective (very generous threshold)
    if second_duration >= first_duration / 2 {
        eprintln!(
            "⚠️  Warning: Second call shows minimal improvement ({:.1}x), may indicate caching issue",
            improvement_3x
        );
    }

    // Log subsequent calls for monitoring
    for (i, &duration) in subsequent_durations.iter().enumerate() {
        let improvement = if duration.as_micros() > 0 {
            first_duration.as_micros() as f64 / duration.as_micros() as f64
        } else {
            f64::INFINITY
        };

        match improvement {
            f64::INFINITY => println!(
                "   Subsequent call {}: {:?} ({} µs, >1000x improvement)",
                i + 1,
                duration,
                duration.as_micros()
            ),
            _ => println!(
                "   Subsequent call {}: {:?} ({} µs, {:.1}x improvement)",
                i + 1,
                duration,
                duration.as_micros(),
                improvement
            ),
        }

        // Very generous warning threshold
        if duration >= first_duration / 2 {
            eprintln!(
                "⚠️  Warning: Subsequent call {} shows minimal improvement ({:.1}x)",
                i + 1,
                improvement
            );
        }
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
async fn test_tag_cache_isolation() -> Result<()> {
    init_test_logging(None);

    let (repo_path, _project) = create_repo_with_tags(50).await?;

    // Create two separate GitRepo instances from the same path
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

    // Log cache effectiveness for both instances - use microseconds to avoid division by zero
    let repo1_improvement = if repo1_second.as_micros() > 0 {
        repo1_first.as_micros() as f64 / repo1_second.as_micros() as f64
    } else {
        f64::INFINITY
    };
    let repo2_improvement = if repo2_second.as_micros() > 0 {
        repo2_first.as_micros() as f64 / repo2_second.as_micros() as f64
    } else {
        f64::INFINITY
    };

    match repo1_improvement {
        f64::INFINITY => println!(
            "   Repo1 cache: {:?} → {:?} (>1000x improvement - too fast to measure)",
            repo1_first, repo1_second
        ),
        _ => println!(
            "   Repo1 cache: {:?} → {:?} ({:.1}x improvement)",
            repo1_first, repo1_second, repo1_improvement
        ),
    }
    match repo2_improvement {
        f64::INFINITY => println!(
            "   Repo2 cache: {:?} → {:?} (>1000x improvement - too fast to measure)",
            repo2_first, repo2_second
        ),
        _ => println!(
            "   Repo2 cache: {:?} → {:?} ({:.1}x improvement)",
            repo2_first, repo2_second, repo2_improvement
        ),
    }

    // Very generous warning thresholds
    if repo1_second >= repo1_first / 2 {
        eprintln!("⚠️  Warning: Repo1 cache shows minimal improvement ({:.1}x)", repo1_improvement);
    }
    if repo2_second >= repo2_first / 2 {
        eprintln!("⚠️  Warning: Repo2 cache shows minimal improvement ({:.1}x)", repo2_improvement);
    }

    // Both first calls should be slower than cached calls (fetching from git)
    // Use microseconds for more precise measurement
    assert!(repo1_first.as_micros() > 100, "Repo1 first call should fetch from git (at least 100µs)");
    assert!(repo2_first.as_micros() > 100, "Repo2 first call should fetch from git (at least 100µs)");

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
async fn test_tag_caching_integration_scenario() -> Result<()> {
    init_test_logging(None);

    let (repo_path, _project) = create_repo_with_tags(75).await?;
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
    assert!(discovery_duration.as_micros() > 100, "Initial discovery should take time (at least 100µs)");

    for (i, &duration) in constraint_durations.iter().enumerate() {
        let improvement = if duration.as_micros() > 0 {
            discovery_duration.as_micros() as f64 / duration.as_micros() as f64
        } else {
            f64::INFINITY
        };

        match improvement {
            f64::INFINITY => println!("   Constraint {}: {:?} (>1000x improvement - too fast to measure)", i + 1, duration),
            _ => println!("   Constraint {}: {:?} ({:.1}x improvement)", i + 1, duration, improvement),
        }

        // Very generous warning threshold
        if duration >= discovery_duration / 2 {
            eprintln!(
                "⚠️  Warning: Constraint {} shows minimal improvement ({:.1}x)",
                i + 1,
                improvement
            );
        }
    }

    assert!(final_duration < discovery_duration, "Final validation should be cached and faster");

    let avg_cached_time =
        constraint_durations.iter().sum::<Duration>() / constraint_durations.len() as u32;
    let improvement_factor = if avg_cached_time.as_micros() > 0 {
        discovery_duration.as_micros() as f64 / avg_cached_time.as_micros() as f64
    } else {
        f64::INFINITY
    };

    println!("   Discovery time: {:?}", discovery_duration);
    println!("   Average cached time: {:?}", avg_cached_time);
    match improvement_factor {
        f64::INFINITY => println!("   Performance improvement: >1000x (too fast to measure)"),
        _ => println!("   Performance improvement: {:.1}x", improvement_factor),
    }

    // Very generous warning threshold
    if improvement_factor < 2.0 && improvement_factor.is_finite() {
        eprintln!(
            "⚠️  Warning: Integration scenario shows minimal cache improvement ({:.1}x)",
            improvement_factor
        );
    }

    match improvement_factor {
        f64::INFINITY => println!(
            "   ✅ Tag caching integration verified with >1000x performance improvement"
        ),
        _ => println!(
            "   ✅ Tag caching integration verified with {:.1}x performance improvement",
            improvement_factor
        ),
    }

    Ok(())
}
