//! CCPM - Claude Code Package Manager
//!
//! A Git-based package manager for Claude Code resources (agents, snippets, and more)
//! that enables reproducible installations using lockfile-based dependency management,
//! similar to Cargo.
//!
//! # Architecture
//!
//! CCPM follows a manifest/lockfile model where:
//! - `ccpm.toml` defines desired dependencies and their version constraints
//! - `ccpm.lock` records exact resolved versions for reproducible builds
//! - Resources are fetched directly from Git repositories (no central registry)
//!
//! # Core Modules
//!
//! - [`cache`] - Git repository caching and management
//! - [`cli`] - Command-line interface implementation
//! - [`config`] - Global and project configuration
//! - [`core`] - Core types, error handling, and resource traits
//! - [`git`] - Git operations wrapper using system git command
//! - [`hooks`] - Hook configuration management for Claude Code automation
//! - [`lockfile`] - Lockfile generation, parsing, and validation
//! - [`manifest`] - Manifest (ccpm.toml) parsing and validation
//! - [`markdown`] - Markdown file operations and metadata extraction
//! - [`models`] - Shared data models for dependency specifications
//! - [`resolver`] - Dependency resolution and conflict detection
//! - [`source`] - Source repository operations and management
//! - [`utils`] - Cross-platform utilities and helpers
//! - [`version`] - Version constraint parsing and matching
//!
//! # Example
//!
//! ```toml
//! # ccpm.toml
//! [sources]
//! community = "https://github.com/aig787/ccpm-community.git"
//!
//! [agents]
//! example-agent = { source = "community", path = "agents/example.md", version = "v1.0.0" }
//! ```
//!
//! # Usage
//!
//! ```bash
//! # Install dependencies from ccpm.toml
//! ccpm install
//!
//! # Update dependencies within version constraints
//! ccpm update
//!
//! # List installed resources
//! ccpm list
//! ```

pub mod cache;
pub mod cli;
pub mod config;
pub mod core;
pub mod git;
pub mod hooks;
pub mod installer;
pub mod lockfile;
pub mod manifest;
pub mod markdown;
pub mod mcp;
pub mod models;
pub mod resolver;
pub mod source;
pub mod utils;
pub mod version;

// test_utils module is available for both unit tests and integration tests
#[cfg(any(test, feature = "test-utils"))]
pub mod test_utils;
