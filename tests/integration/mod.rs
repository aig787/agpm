//! Integration test suite for AGPM
//!
//! This test suite contains comprehensive end-to-end integration tests that verify
//! the complete functionality of AGPM commands and workflows. These tests run relatively
//! quickly and are executed in CI on every commit.
//!
//! # Running Integration Tests
//!
//! ```bash
//! cargo test --test integration
//! cargo nextest run --test integration
//! ```
//!
//! # Test Organization
//!
//! Tests are organized by functionality area:
//! - **cache_behavior**: Cache and worktree management
//! - **conflict_detection**: Version conflict detection
//! - **content_filter**: Content filter (`{{ 'path' | content }}`) functionality
//! - **cross_platform**: Cross-platform compatibility (Windows, macOS, Linux)
//! - **deploy**: Deployment and installation workflows
//! - **deps_refresh**: Dependency refresh and update logic
//! - **error_scenarios**: Error handling and edge cases
//! - **file_url**: file:// URL support
//! - **gitignore**: .gitignore management
//! - **hooks**: Claude Code hooks integration
//! - **incremental_add**: Incremental dependency addition
//! - **install_field**: Install field and content embedding functionality
//! - **list**: List command functionality
//! - **lockfile_staleness**: Lockfile staleness detection
//! - **max_parallel_flag**: --max-parallel flag behavior
//! - **multi_artifact**: Multiple artifact types
//! - **multi_resource**: Multiple resource management
//! - **outdated**: Outdated dependency detection
//! - **patch_integration**: Patch/override functionality
//! - **pattern**: Pattern-based dependency installation
//! - **transitive**: Transitive dependency resolution
//! - **tree**: Dependency tree visualization
//! - **upd_progress**: Update progress reporting
//! - **upgrade**: Self-upgrade functionality
//! - **validate**: Validation command
//! - **versioning**: Version constraint handling

// Shared test utilities (from parent tests/ directory)
#[path = "../common/mod.rs"]
mod common;
#[path = "../fixtures/mod.rs"]
mod fixtures;

// Test configuration (used by versioning tests)
mod test_config;

// Integration tests
mod cache_behavior;
mod conflict_detection;
mod content_filter;
mod cross_platform;
mod deploy;
mod deps_refresh;
mod error_scenarios;
mod file_url;
mod gitignore;
mod hooks;
mod incremental_add;
mod install_field;
mod list;
mod lockfile_stability;
mod lockfile_staleness;
mod max_parallel_flag;
mod multi_artifact;
mod multi_resource;
mod outdated;
mod patch_integration;
mod pattern;
mod prefixed_versions;
mod project_template_vars;
mod templating;
mod tool_enable_disable;
mod transitive;
mod tree;
mod upd_progress;
mod upgrade;
mod validate;
mod versioning;
