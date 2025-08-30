//! Test environment setup and management
//!
//! This module provides a complete test environment for integration and library tests.

use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

use super::fixtures::{LockfileFixture, ManifestFixture, MarkdownFixture};

/// Test environment helper that sets up a complete test project
pub struct TestEnvironment {
    pub temp_dir: TempDir,
    pub project_dir: PathBuf,
    pub sources_dir: PathBuf,
    pub cache_dir: PathBuf,
}

impl TestEnvironment {
    /// Create a new test environment
    pub fn new() -> Result<Self> {
        // Initialize test logging if RUST_LOG is set
        super::init_test_logging();

        let temp_dir = TempDir::new()?;
        let project_dir = temp_dir.path().join("project");
        let sources_dir = temp_dir.path().join("sources");
        let cache_dir = temp_dir.path().join("cache");

        fs::create_dir_all(&project_dir)?;
        fs::create_dir_all(&sources_dir)?;
        fs::create_dir_all(&cache_dir)?;

        Ok(Self {
            temp_dir,
            project_dir,
            sources_dir,
            cache_dir,
        })
    }

    /// Create a test environment with a basic manifest
    pub fn with_basic_manifest() -> Result<Self> {
        let env = Self::new()?;
        ManifestFixture::basic().write_to(&env.project_dir)?;
        Ok(env)
    }

    /// Create a test environment with a basic manifest that uses file:// URLs for testing
    pub fn with_basic_manifest_file_urls() -> Result<Self> {
        let env = Self::new()?;

        // Create a modified manifest that uses file:// URLs
        let manifest_content = format!(
            r#"
[sources]
official = "file://{}/official"
community = "file://{}/community"

[agents]
my-agent = {{ source = "official", path = "agents/my-agent.md", version = "v1.0.0" }}
helper = {{ source = "community", path = "agents/helper.md", version = "v1.0.0" }}

[snippets]
utils = {{ source = "official", path = "snippets/utils.md", version = "v1.0.0" }}
"#,
            env.sources_dir.display(),
            env.sources_dir.display()
        );

        fs::write(env.project_dir.join("ccpm.toml"), manifest_content.trim())?;
        Ok(env)
    }

    /// Create a test environment with manifest and lockfile
    pub fn with_manifest_and_lockfile() -> Result<Self> {
        let env = Self::new()?;
        ManifestFixture::basic().write_to(&env.project_dir)?;
        LockfileFixture::basic().write_to(&env.project_dir)?;
        Ok(env)
    }

    /// Create a test environment with manifest and lockfile using file:// URLs
    pub fn with_manifest_and_lockfile_file_urls() -> Result<Self> {
        let env = Self::new()?;

        // Create a modified manifest that uses file:// URLs
        let manifest_content = format!(
            r#"
[sources]
official = "file://{}/official"
community = "file://{}/community"

[agents]
my-agent = {{ source = "official", path = "agents/my-agent.md", version = "v1.0.0" }}
helper = {{ source = "community", path = "agents/helper.md", version = "v1.0.0" }}

[snippets]
utils = {{ source = "official", path = "snippets/utils.md", version = "v1.0.0" }}
"#,
            env.sources_dir.display(),
            env.sources_dir.display()
        );

        fs::write(env.project_dir.join("ccpm.toml"), manifest_content.trim())?;

        // Create a matching lockfile that uses file:// URLs
        let lockfile_content = format!(
            r#"
# Auto-generated lockfile - DO NOT EDIT
version = 1

[[sources]]
name = "official"
url = "file://{}/official"
commit = "abc123456789abcdef123456789abcdef12345678"
fetched_at = "2024-01-01T00:00:00Z"

[[sources]]
name = "community"
url = "file://{}/community"
commit = "def456789abcdef123456789abcdef123456789ab"
fetched_at = "2024-01-01T00:00:00Z"

[[agents]]
name = "my-agent"
source = "official"
path = "agents/my-agent.md"
version = "v1.0.0"
resolved_commit = "abc123456789abcdef123456789abcdef12345678"
checksum = "sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
installed_at = "agents/my-agent.md"

[[agents]]
name = "helper"
source = "community"
path = "agents/helper.md"
version = "v1.0.0"
resolved_commit = "def456789abcdef123456789abcdef123456789ab"
checksum = "sha256:38b060a751ac96384cd9327eb1b1e36a21fdb71114be07434c0cc7bf63f6e1da"
installed_at = "agents/helper.md"

[[snippets]]
name = "utils"
source = "official"
path = "snippets/utils.md"
version = "v1.0.0"
resolved_commit = "abc123456789abcdef123456789abcdef12345678"
checksum = "sha256:74e6f7298a9c2d168935f58c6b6c5b5ea4c3df6a0b6b8d2e7b2a2b8c3d4e5f6a"
installed_at = "snippets/utils.md"
"#,
            env.sources_dir.display(),
            env.sources_dir.display()
        );

        fs::write(env.project_dir.join("ccpm.lock"), lockfile_content.trim())?;
        Ok(env)
    }

    /// Add a mock git repository to the sources directory
    pub fn add_mock_source(
        &self,
        name: &str,
        _url: &str,
        files: Vec<MarkdownFixture>,
    ) -> Result<PathBuf> {
        let source_dir = self.sources_dir.join(name);
        fs::create_dir_all(&source_dir)?;

        // Initialize as a real git repository
        std::process::Command::new("git")
            .args(["-C", source_dir.to_str().unwrap(), "init"])
            .output()
            .context("Failed to initialize git repository")?;

        // Configure git user for commits (required for git)
        std::process::Command::new("git")
            .args(["-C", source_dir.to_str().unwrap(), "config", "user.email", "test@example.com"])
            .output()?;

        std::process::Command::new("git")
            .args(["-C", source_dir.to_str().unwrap(), "config", "user.name", "Test User"])
            .output()?;

        // Add markdown files
        for file in files {
            file.write_to(&source_dir)?;
        }

        // Add and commit all files
        std::process::Command::new("git")
            .args(["-C", source_dir.to_str().unwrap(), "add", "."])
            .output()?;

        std::process::Command::new("git")
            .args(["-C", source_dir.to_str().unwrap(), "commit", "-m", "Initial commit"])
            .output()?;

        // Create a v1.0.0 tag for testing version resolution
        std::process::Command::new("git")
            .args(["-C", source_dir.to_str().unwrap(), "tag", "v1.0.0"])
            .output()?;

        Ok(source_dir)
    }

    /// Get the project directory path
    #[must_use]
    pub fn project_path(&self) -> &Path {
        &self.project_dir
    }

    /// Get the sources directory path
    #[must_use]
    pub fn sources_path(&self) -> &Path {
        &self.sources_dir
    }

    /// Get the cache directory path
    #[must_use]
    pub fn cache_path(&self) -> &Path {
        &self.cache_dir
    }

    /// Create a file in the project directory
    pub fn create_file(&self, path: impl AsRef<Path>, content: &str) -> Result<PathBuf> {
        let full_path = self.project_dir.join(path);
        if let Some(parent) = full_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&full_path, content)?;
        Ok(full_path)
    }

    /// Read a file from the project directory
    pub fn read_file(&self, path: impl AsRef<Path>) -> Result<String> {
        let full_path = self.project_dir.join(path);
        Ok(fs::read_to_string(full_path)?)
    }

    /// Check if a file exists in the project directory
    pub fn file_exists(&self, path: impl AsRef<Path>) -> bool {
        self.project_dir.join(path).exists()
    }
}
