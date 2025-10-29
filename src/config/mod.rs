//! Configuration management for AGPM
//!
//! This module provides comprehensive configuration management for the `AGent` Package Manager (AGPM).
//! It handles project manifests, global user configuration, and resource metadata with a focus on
//! security, cross-platform compatibility, and reproducible builds.
//!
//! # Architecture Overview
//!
//! AGPM uses a multi-layered configuration architecture:
//!
//! 1. **Global Configuration** (`~/.agpm/config.toml`) - User-wide settings including authentication
//! 2. **Project Manifest** (`agpm.toml`) - Project dependencies and sources
//! 3. **Lockfile** (`agpm.lock`) - Resolved versions for reproducible builds
//! 4. **Resource Metadata** - Agent and snippet configurations embedded in `.md` files
//!
//! # Modules
//!
//! - `global` - Global configuration management with authentication token support
//! - `parser` - Generic TOML parsing utilities with error context
//!
//! # Configuration Files
//!
//! ## Global Configuration (`~/.agpm/config.toml`)
//!
//! **Location:**
//! - Unix/macOS: `~/.agpm/config.toml`
//! - Windows: `%LOCALAPPDATA%\agpm\config.toml`
//!
//! **Purpose:** Store user-wide settings including private repository access tokens.
//! This file is never committed to version control.
//!
//! ```toml
//! # Global sources with authentication tokens
//! [sources]
//! private = "https://oauth2:ghp_xxxxxxxxxxxx@github.com/company/private-agpm.git"
//! enterprise = "https://token:abc123@gitlab.company.com/ai/resources.git"
//! ```
//!
//! ## Project Manifest (`agpm.toml`)
//!
//! **Purpose:** Define project dependencies and public sources. Safe for version control.
//!
//! ```toml
//! [sources]
//! community = "https://github.com/aig787/agpm-community.git"
//!
//! [agents]
//! code-reviewer = { source = "community", path = "agents/code-reviewer.md", version = "v1.2.0" }
//! local-helper = { path = "../local-agents/helper.md" }
//!
//! [snippets]
//! rust-patterns = { source = "community", path = "snippets/rust.md", version = "^2.0" }
//! ```
//!
//! ## Lockfile (`agpm.lock`)
//!
//! **Purpose:** Pin exact versions for reproducible installations. Auto-generated.
//!
//! ```toml
//! # Auto-generated lockfile - DO NOT EDIT
//! version = 1
//!
//! [[sources]]
//! name = "community"
//! url = "https://github.com/aig787/agpm-community.git"
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
//! 1. Environment variables (`AGPM_CONFIG_PATH`, `AGPM_CACHE_DIR`)
//! 2. Global configuration (`~/.agpm/config.toml`)
//! 3. Project manifest (`agpm.toml`)
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
//! author = "AGPM Community"
//! license = "MIT"
//! keywords = ["rust", "programming", "expert"]
//!
//! [requirements]
//! agpm_version = ">=0.1.0"
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
//! - **macOS/Linux**: Uses `$HOME/.agpm` directory
//! - **Path Separators**: Normalized automatically
//! - **File Permissions**: Handles Windows vs Unix differences
//!
//! # Examples
//!
//! ## Loading Global Configuration
//!
//! ```rust,no_run
//! use agpm_cli::config::{GlobalConfig, GlobalConfigManager};
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
//! ## Source Resolution with Authentication
//!
//! ```rust,no_run
//! use agpm_cli::config::GlobalConfig;
//! use std::collections::HashMap;
//!
//! # async fn example() -> anyhow::Result<()> {
//! let global = GlobalConfig::load().await?;
//!
//! // Project manifest sources (public)
//! let mut local_sources = HashMap::new();
//! local_sources.insert(
//!     "community".to_string(),
//!     "https://github.com/aig787/agpm-community.git".to_string()
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

mod global;
mod parser;

pub use global::{GlobalConfig, GlobalConfigManager};
pub use parser::parse_config;

use crate::core::file_error::{FileOperation, FileResultExt};
use anyhow::Result;
use std::path::PathBuf;

/// Get the cache directory for AGPM.
///
/// Returns the directory where AGPM stores cached Git repositories and temporary files.
/// The location follows platform conventions and can be overridden with environment variables.
///
/// # Location Priority
///
/// 1. `AGPM_CACHE_DIR` environment variable (if set)
/// 2. Platform-specific cache directory:
///    - Windows: `%LOCALAPPDATA%\agpm\cache`
///    - macOS/Linux: `~/.agpm/cache`
///
/// # Directory Creation
///
/// The directory is automatically created if it doesn't exist.
///
/// # Examples
///
/// ```rust,no_run
/// use agpm_cli::config::get_cache_dir;
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
    // Check for environment variable override first (essential for testing)
    if let Ok(dir) = std::env::var("AGPM_CACHE_DIR") {
        return Ok(PathBuf::from(dir));
    }

    // Use consistent directory structure with rest of AGPM
    let cache_dir = if cfg!(target_os = "windows") {
        dirs::data_local_dir()
            .ok_or_else(|| anyhow::anyhow!("Unable to determine local data directory"))?
            .join("agpm")
            .join("cache")
    } else {
        dirs::home_dir()
            .ok_or_else(|| anyhow::anyhow!("Unable to determine home directory"))?
            .join(".agpm")
            .join("cache")
    };

    if !cache_dir.exists() {
        std::fs::create_dir_all(&cache_dir).with_file_context(
            FileOperation::CreateDir,
            &cache_dir,
            "creating cache directory",
            "config::get_cache_dir",
        )?;
    }

    Ok(cache_dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_cache_dir() {
        // Test that we get a valid cache dir
        let dir = get_cache_dir().unwrap();
        assert!(dir.to_string_lossy().contains("agpm"));
    }
}
