//! Configuration management for CCPM
//!
//! This module provides comprehensive configuration management for the Claude Code Package Manager (CCPM).
//! It handles project manifests, global user configuration, and resource metadata with a focus on
//! security, cross-platform compatibility, and reproducible builds.
//!
//! # Architecture Overview
//!
//! CCPM uses a multi-layered configuration architecture:
//!
//! 1. **Global Configuration** (`~/.ccpm/config.toml`) - User-wide settings including authentication
//! 2. **Project Manifest** (`ccpm.toml`) - Project dependencies and sources
//! 3. **Lockfile** (`ccpm.lock`) - Resolved versions for reproducible builds
//! 4. **Resource Metadata** - Agent and snippet configurations embedded in `.md` files
//!
//! # Modules
//!
//! - `agent` - Agent and snippet manifest structures for resource metadata
//! - `global` - Global configuration management with authentication token support
//! - `parser` - Generic TOML parsing utilities with error context
//!
//! # Configuration Files
//!
//! ## Global Configuration (`~/.ccpm/config.toml`)
//!
//! **Location:**
//! - Unix/macOS: `~/.ccpm/config.toml`
//! - Windows: `%LOCALAPPDATA%\ccpm\config.toml`
//!
//! **Purpose:** Store user-wide settings including private repository access tokens.
//! This file is never committed to version control.
//!
//! ```toml
//! # Global sources with authentication tokens
//! [sources]
//! private = "https://oauth2:ghp_xxxxxxxxxxxx@github.com/company/private-ccpm.git"
//! enterprise = "https://token:abc123@gitlab.company.com/ai/resources.git"
//! ```
//!
//! ## Project Manifest (`ccpm.toml`)
//!
//! **Purpose:** Define project dependencies and public sources. Safe for version control.
//!
//! ```toml
//! [sources]
//! community = "https://github.com/aig787/ccpm-community.git"
//!
//! [agents]
//! code-reviewer = { source = "community", path = "agents/code-reviewer.md", version = "v1.2.0" }
//! local-helper = { path = "../local-agents/helper.md" }
//!
//! [snippets]
//! rust-patterns = { source = "community", path = "snippets/rust.md", version = "^2.0" }
//! ```
//!
//! ## Lockfile (`ccpm.lock`)
//!
//! **Purpose:** Pin exact versions for reproducible installations. Auto-generated.
//!
//! ```toml
//! # Auto-generated lockfile - DO NOT EDIT
//! version = 1
//!
//! [[sources]]
//! name = "community"
//! url = "https://github.com/aig787/ccpm-community.git"
//! commit = "abc123..."
//!
//! [[agents]]
//! name = "code-reviewer"
//! source = "community"
//! version = "v1.2.0"
//! resolved_commit = "def456..."
//! checksum = "sha256:..."
//! installed_at = "agents/code-reviewer.md"
//! ```
//!
//! # Security Model
//!
//! ## Credential Isolation
//!
//! - **Global Config**: Contains authentication tokens, never committed
//! - **Project Manifest**: Public sources only, safe for version control
//! - **Source Merging**: Global sources loaded first, project sources can override
//!
//! ## Configuration Priority
//!
//! 1. Environment variables (`CCPM_CONFIG_PATH`, `CCPM_CACHE_DIR`)
//! 2. Global configuration (`~/.ccpm/config.toml`)
//! 3. Project manifest (`ccpm.toml`)
//! 4. Default values
//!
//! # Resource Metadata
//!
//! Agent and snippet files can include TOML frontmatter for metadata:
//!
//! ```markdown
//! +++
//! [metadata]
//! name = "rust-expert"
//! description = "Expert Rust development agent"
//! author = "CCPM Community"
//! license = "MIT"
//! keywords = ["rust", "programming", "expert"]
//!
//! [requirements]
//! ccpm_version = ">=0.1.0"
//! claude_version = "latest"
//! platforms = ["windows", "macos", "linux"]
//!
//! [[requirements.dependencies]]
//! name = "code-formatter"
//! version = "^1.0"
//! type = "snippet"
//! +++
//!
//! # Rust Expert Agent
//!
//! You are an expert Rust developer...
//! ```
//!
//! # Platform Support
//!
//! This module handles cross-platform configuration paths:
//!
//! - **Windows**: Uses `%LOCALAPPDATA%` for configuration
//! - **macOS/Linux**: Uses `$HOME/.ccpm` directory
//! - **Path Separators**: Normalized automatically
//! - **File Permissions**: Handles Windows vs Unix differences
//!
//! # Examples
//!
//! ## Loading Global Configuration
//!
//! ```rust,no_run
//! use ccpm::config::{GlobalConfig, GlobalConfigManager};
//!
//! # async fn example() -> anyhow::Result<()> {
//! // Simple load
//! let global = GlobalConfig::load().await?;
//! println!("Found {} global sources", global.sources.len());
//!
//! // Using manager for caching
//! let mut manager = GlobalConfigManager::new()?;
//! let config = manager.get().await?;
//!
//! // Add authenticated source
//! let config = manager.get_mut().await?;
//! config.add_source(
//!     "private".to_string(),
//!     "https://oauth2:token@github.com/company/repo.git".to_string()
//! );
//! manager.save().await?;
//! # Ok(())
//! # }
//! ```
//!
//! ## Parsing Resource Metadata
//!
//! ```rust,no_run
//! use ccpm::config::{parse_config, AgentManifest};
//! use std::path::Path;
//!
//! # fn example() -> anyhow::Result<()> {
//! // Parse agent manifest from TOML file
//! let agent: AgentManifest = parse_config(Path::new("agent.toml"))?;
//!
//! println!("Agent: {} by {}",
//!          agent.metadata.name,
//!          agent.metadata.author);
//!
//! if let Some(requirements) = &agent.requirements {
//!     println!("Requires CCPM: {:?}", requirements.ccpm_version);
//! }
//! # Ok(())
//! # }
//! ```
//!
//! ## Source Resolution with Authentication
//!
//! ```rust,no_run
//! use ccpm::config::GlobalConfig;
//! use std::collections::HashMap;
//!
//! # async fn example() -> anyhow::Result<()> {
//! let global = GlobalConfig::load().await?;
//!
//! // Project manifest sources (public)
//! let mut local_sources = HashMap::new();
//! local_sources.insert(
//!     "community".to_string(),
//!     "https://github.com/aig787/ccpm-community.git".to_string()
//! );
//!
//! // Merge with global sources (may include auth tokens)
//! let merged = global.merge_sources(&local_sources);
//!
//! // Use merged sources for git operations
//! for (name, url) in &merged {
//!     println!("Source {}: {}", name,
//!              if url.contains("@") { "[authenticated]" } else { url });
//! }
//! # Ok(())
//! # }
//! ```

mod agent;
mod global;
mod parser;

pub use agent::{
    AgentManifest, AgentMetadata, Dependency, Requirements, SnippetContent, SnippetManifest,
    SnippetMetadata,
};
pub use global::{GlobalConfig, GlobalConfigManager};
pub use parser::parse_config;

// Type aliases for cleaner code
pub type AgentConfig = AgentManifest;
pub type SnippetConfig = SnippetManifest;

use anyhow::Result;
use std::path::PathBuf;

/// Get the cache directory for CCPM.
///
/// Returns the directory where CCPM stores cached Git repositories and temporary files.
/// The location follows platform conventions and can be overridden with environment variables.
///
/// # Location Priority
///
/// 1. `CCPM_CACHE_DIR` environment variable (if set)
/// 2. Platform-specific cache directory:
///    - Windows: `%LOCALAPPDATA%\ccpm\cache`
///    - macOS: `~/Library/Caches/ccpm`
///    - Linux: `~/.cache/ccpm`
///
/// # Directory Creation
///
/// The directory is automatically created if it doesn't exist.
///
/// # Examples
///
/// ```rust,no_run
/// use ccpm::config::get_cache_dir;
///
/// # fn example() -> anyhow::Result<()> {
/// let cache = get_cache_dir()?;
/// println!("Cache directory: {}", cache.display());
/// # Ok(())
/// # }
/// ```
///
/// # Errors
///
/// Returns an error if:
/// - The system cache directory cannot be determined
/// - The cache directory cannot be created
/// - Insufficient permissions for directory creation
pub fn get_cache_dir() -> Result<PathBuf> {
    if let Ok(dir) = std::env::var("CCPM_CACHE_DIR") {
        return Ok(PathBuf::from(dir));
    }

    let cache_dir = dirs::cache_dir()
        .ok_or_else(|| anyhow::anyhow!("Could not determine cache directory"))?
        .join("ccpm");

    if !cache_dir.exists() {
        std::fs::create_dir_all(&cache_dir)?;
    }

    Ok(cache_dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_cache_dir() {
        // Save and clear any existing CCPM_CACHE_DIR to ensure we test the default behavior
        let original = std::env::var("CCPM_CACHE_DIR").ok();
        std::env::remove_var("CCPM_CACHE_DIR");

        let dir = get_cache_dir().unwrap();
        assert!(dir.to_string_lossy().contains("ccpm"));

        // Restore original value if it existed
        if let Some(val) = original {
            std::env::set_var("CCPM_CACHE_DIR", val);
        }
    }

    #[test]
    fn test_cache_dir_with_env_var() {
        // NOTE: This test explicitly tests environment variable functionality
        // It uses std::env::set_var which can cause race conditions in parallel test execution.
        // If this test becomes flaky, run with: cargo test -- --test-threads=1

        // Save original value
        let original = std::env::var("CCPM_CACHE_DIR").ok();

        // Set test value
        std::env::set_var("CCPM_CACHE_DIR", "/tmp/test_cache");
        let dir = get_cache_dir().unwrap();
        assert_eq!(dir, PathBuf::from("/tmp/test_cache"));

        // Restore original value
        match original {
            Some(val) => std::env::set_var("CCPM_CACHE_DIR", val),
            None => std::env::remove_var("CCPM_CACHE_DIR"),
        }
    }
}
