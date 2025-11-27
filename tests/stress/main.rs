//! Stress and Performance Test Suite for AGPM
//!
//! This test suite contains stress tests and performance benchmarks that validate
//! AGPM's behavior under high load and extreme conditions. These tests take significantly
//! longer to run than integration tests and are **not executed in CI**.
//!
//! # Purpose
//!
//! Stress tests serve several critical purposes:
//!
//! - **Validate parallelism**: Test concurrent operations with high --max-parallel values
//! - **Find performance regressions**: Catch slowdowns before releases
//! - **Verify resource limits**: Ensure system handles edge cases (500+ dependencies)
//! - **Test cache efficiency**: Validate worktree reuse and fetch optimization
//! - **Measure throughput**: Track installation/update rates over time
//!
//! # When to Run Stress Tests
//!
//! Run stress tests:
//! - Before major releases (v0.x.0)
//! - After significant performance improvements
//! - When debugging performance issues
//! - After changes to parallelism/caching logic
//! - To establish performance baselines on new hardware
//!
//! # Running Stress Tests
//!
//! All stress tests are **parallel-safe** and run concurrently, which helps surface race
//! conditions and deadlocks. Performance is logged via `println!` for manual review rather
//! than asserted, relying on nextest's test timeout to catch hangs. Each test uses isolated
//! temp directories.
//!
//! ## Run All Stress Tests
//!
//! ```bash
//! cargo test --test stress
//! make test-stress
//! ```
//!
//! ## Run with Verbose Output
//!
//! ```bash
//! cargo test --test stress -- --nocapture
//! make test-stress-verbose
//! ```
//!
//! ## Run Specific Test
//!
//! ```bash
//! cargo test --test stress test_heavy_stress_500_dependencies
//! cargo test --test stress parallelism::  # All parallelism tests
//! ```
//!
//! ## Run with Release Optimizations
//!
//! ```bash
//! cargo test --test stress --release
//! ```
//!
//! # Performance Baselines
//!
//! Recorded on M1 MacBook Pro (2024-10-10, AGPM v0.4.3):
//!
//! ## Large Scale Tests (`large_scale.rs`)
//!
//! | Test | Dependencies | Duration | Rate | Notes |
//! |------|--------------|----------|------|-------|
//! | `test_heavy_stress_500_dependencies` | 500 agents | <60s | ~8.3/s | 5 repos, worktree reuse |
//! | `test_heavy_stress_500_updates` | 500 updates | <45s | ~11/s | Update existing installations |
//! | `test_community_repo_500_dependencies` | 500 agents | <90s | ~5.5/s | Real agpm-community repo |
//! | `test_mixed_repos_file_and_https` | 200 mixed | <30s | ~6.7/s | file:// + https:// |
//! | `test_community_repo_parallel_checkout_performance` | Varies | <60s | - | Checkout performance |
//!
//! ## Parallelism Tests (`parallelism.rs`)
//!
//! | Test | Load | Duration | Notes |
//! |------|------|----------|-------|
//! | `test_extreme_parallelism` | 100 agents, --max-parallel 100 | ~5s | System throttling |
//! | `test_rapid_sequential_operations` | 3 agents, repeated | ~3s | Cache reuse |
//! | `test_mixed_parallelism_levels` | 50 agents, varying | ~10s | Different --max-parallel |
//! | `test_parallelism_resource_contention` | 30 agents, parallel | ~8s | Lock contention |
//! | `test_parallelism_graceful_limits` | 20 agents, limits | ~6s | Graceful degradation |
//!
//! # Test Organization
//!
//! - **large_scale.rs**: Tests with hundreds of dependencies (500+)
//! - **parallelism.rs**: Concurrency and --max-parallel flag behavior
//!
//! # Interpreting Results
//!
//! ## Expected Behavior
//!
//! - Installation rate: 5-15 agents/second (depends on size, parallelism)
//! - Update rate: 10-20 updates/second (faster due to cache)
//! - Memory usage: Linear growth with --max-parallel value
//! - No deadlocks or race conditions
//!
//! ## Warning Signs
//!
//! - Installation rate drops below 3/second → investigate cache efficiency
//! - Tests time out (>120s) → check for deadlocks or resource exhaustion
//! - High variance between runs → potential race conditions
//! - Memory usage grows exponentially → memory leak
//!
//! # Contributing
//!
//! When adding new stress tests:
//!
//! 1. Document expected duration and performance baseline
//! 2. Use `#[tokio::test]` (no `#[ignore]` needed - suite is separate)
//! 3. Include test description explaining what it stresses
//! 4. Update performance baselines table in this file
//! 5. Consider adding `#[ignore]` if test is extremely slow (>5 min)

// Shared test utilities (from parent tests/ directory)
#[path = "../common/mod.rs"]
mod common;

// Stress test modules
mod chaos_conflict_tracking;
mod large_scale;
mod parallelism;
mod pattern_performance;
mod template_context_lookup;
mod transitive_depth;
