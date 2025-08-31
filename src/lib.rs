//! CCPM - Claude Code Package Manager
//!
//! A Git-based package manager for Claude Code resources (agents, snippets, commands,
//! scripts, hooks, and MCP servers) that enables reproducible installations using
//! lockfile-based dependency management, similar to Cargo.
//!
//! # Architecture Overview
//!
//! CCPM follows a manifest/lockfile model where:
//! - `ccpm.toml` defines desired dependencies and their version constraints
//! - `ccpm.lock` records exact resolved versions for reproducible builds
//! - Resources are fetched directly from Git repositories (no central registry)
//! - Pattern-based dependencies enable bulk installation of related resources
//!
//! ## Key Features
//!
//! - **Decentralized**: No central registry - resources come from Git repositories
//! - **Reproducible**: Lockfile ensures identical installations across environments
//! - **Cross-platform**: Works on Windows, macOS, and Linux with proper path handling
//! - **Pattern Matching**: Install multiple resources using glob patterns (e.g., `agents/*.md`)
//! - **MCP Integration**: Native support for Model Context Protocol servers
//! - **Hook System**: Automated Claude Code event handlers
//! - **Security**: Input validation, path traversal prevention, credential isolation
//!
//! # Core Modules
//!
//! ## Core Functionality
//! - [`cache`] - Git repository caching and management for performance
//! - [`cli`] - Command-line interface with comprehensive subcommands
//! - [`config`] - Global (~/.ccpm/config.toml) and project configuration
//! - [`core`] - Core types, error handling, and resource abstractions
//! - [`resolver`] - Dependency resolution, conflict detection, and version matching
//!
//! ## Git Integration
//! - [`git`] - Git operations wrapper using system git command (like Cargo)
//! - [`source`] - Source repository operations and management
//!
//! ## Resource Management
//! - [`lockfile`] - Lockfile generation, parsing, and validation (ccpm.lock)
//! - [`manifest`] - Manifest parsing and validation (ccpm.toml)
//! - [`pattern`] - Pattern-based dependency resolution using glob patterns
//! - [`markdown`] - Markdown file operations and frontmatter extraction
//!
//! ## Resource Types
//! - [`hooks`] - Claude Code hook configuration and settings.local.json management
//! - [`mcp`] - Model Context Protocol server configuration and .mcp.json management
//!
//! ## Supporting Modules
//! - [`models`] - Shared data models for dependency specifications
//! - [`utils`] - Cross-platform utilities, file operations, and path validation
//! - [`version`] - Version constraint parsing, comparison, and resolution
//!
//! # Manifest Format (ccpm.toml)
//!
//! ## Basic Example
//! ```toml
//! # Define source repositories
//! [sources]
//! community = "https://github.com/aig787/ccpm-community.git"
//! official = "https://github.com/example-org/ccpm-official.git"
//!
//! # Install individual resources
//! [agents]
//! code-reviewer = { source = "official", path = "agents/reviewer.md", version = "v1.0.0" }
//! local-helper = "../local-agents/helper.md"  # Local file
//!
//! # Pattern-based dependencies (new feature)
//! ai-agents = { source = "community", path = "agents/ai/*.md", version = "v1.0.0" }
//! all-tools = { source = "community", path = "**/tools/*.md", version = "latest" }
//!
//! [snippets]
//! utils = { source = "community", path = "snippets/utils.md", version = "v2.1.0" }
//!
//! [commands]
//! deploy = { source = "official", path = "commands/deploy.md", version = "v1.0.0" }
//!
//! # MCP servers and hooks
//! [mcp-servers]
//! filesystem = { source = "official", path = "mcp-servers/filesystem.json", version = "v1.0.0" }
//!
//! [hooks]
//! pre-commit = { source = "community", path = "hooks/pre-commit.json", version = "v1.0.0" }
//! ```
//!
//! # Command-Line Usage
//!
//! ## Installation and Management
//! ```bash
//! # Initialize new CCPM project
//! ccpm init
//!
//! # Install all dependencies from ccpm.toml
//! ccpm install
//!
//! # Install with frozen lockfile (CI/production)
//! ccpm install --frozen
//!
//! # Update dependencies within version constraints
//! ccpm update
//!
//! # Update specific dependencies only
//! ccpm update code-reviewer utils
//! ```
//!
//! ## Resource Discovery
//! ```bash
//! # List installed resources
//! ccpm list
//!
//! # List with details and source information
//! ccpm list --details
//!
//! # List specific resource types
//! ccpm list --agents --format json
//! ```
//!
//! ## Project Management
//! ```bash
//! # Validate project configuration
//! ccpm validate --resolve --sources
//!
//! # Add new dependencies
//! ccpm add dep agent official:agents/helper.md@v1.0.0
//!
//! # Manage cache
//! ccpm cache clean
//! ```

// Core functionality modules
pub mod cache;
pub mod cli;
pub mod config;
pub mod core;
pub mod resolver;

// Git integration
pub mod git;
pub mod source;

// Resource management
pub mod lockfile;
pub mod manifest;
pub mod markdown;
pub mod pattern;

// Resource types
pub mod hooks;
pub mod mcp;

// Supporting modules
pub mod installer;
pub mod models;
pub mod utils;
pub mod version;

// test_utils module is available for both unit tests and integration tests
#[cfg(any(test, feature = "test-utils"))]
pub mod test_utils;
