//! Common test utilities and fixtures for CCPM integration tests
//!
//! This module consolidates frequently used test patterns to reduce duplication
//! and improve test maintainability.

use anyhow::{Context, Result};
use ccpm::manifest::Manifest;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::TempDir;

/// Git command builder for tests
pub struct TestGit {
    repo_path: PathBuf,
}

impl TestGit {
    /// Create a new TestGit instance for the given repository path
    pub fn new(repo_path: impl Into<PathBuf>) -> Self {
        Self {
            repo_path: repo_path.into(),
        }
    }

    /// Initialize a new git repository
    pub fn init(&self) -> Result<()> {
        Command::new("git")
            .arg("init")
            .current_dir(&self.repo_path)
            .output()
            .context("Failed to initialize git repository")?;
        Ok(())
    }

    /// Configure git user for tests
    pub fn config_user(&self) -> Result<()> {
        Command::new("git")
            .args(["config", "user.email", "test@ccpm.example"])
            .current_dir(&self.repo_path)
            .output()
            .context("Failed to configure git user email")?;

        Command::new("git")
            .args(["config", "user.name", "Test User"])
            .current_dir(&self.repo_path)
            .output()
            .context("Failed to configure git user name")?;
        Ok(())
    }

    /// Add all files to staging
    pub fn add_all(&self) -> Result<()> {
        Command::new("git")
            .args(["add", "."])
            .current_dir(&self.repo_path)
            .output()
            .context("Failed to add files to git")?;
        Ok(())
    }

    /// Create a commit with the given message
    pub fn commit(&self, message: &str) -> Result<()> {
        Command::new("git")
            .args(["commit", "-m", message])
            .current_dir(&self.repo_path)
            .output()
            .context("Failed to create git commit")?;
        Ok(())
    }

    /// Create a tag
    pub fn tag(&self, tag_name: &str) -> Result<()> {
        Command::new("git")
            .args(["tag", tag_name])
            .current_dir(&self.repo_path)
            .output()
            .context(format!("Failed to create tag: {}", tag_name))?;
        Ok(())
    }

    /// Create and checkout a branch
    pub fn create_branch(&self, branch_name: &str) -> Result<()> {
        Command::new("git")
            .args(["checkout", "-b", branch_name])
            .current_dir(&self.repo_path)
            .output()
            .context(format!("Failed to create branch: {}", branch_name))?;
        Ok(())
    }

    /// Get the current commit hash
    pub fn get_commit_hash(&self) -> Result<String> {
        let output = Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(&self.repo_path)
            .output()
            .context("Failed to get commit hash")?;

        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }
}

/// Test project builder for creating test environments
pub struct TestProject {
    temp_dir: TempDir,
    project_dir: PathBuf,
    cache_dir: PathBuf,
    sources_dir: PathBuf,
    manifest: Option<Manifest>,
}

impl TestProject {
    /// Create a new test project with default structure
    pub fn new() -> Result<Self> {
        let temp_dir = TempDir::new()?;
        let project_dir = temp_dir.path().join("project");
        let cache_dir = temp_dir.path().join(".ccpm").join("cache");
        let sources_dir = temp_dir.path().join("sources");

        fs::create_dir_all(&project_dir)?;
        fs::create_dir_all(&cache_dir)?;
        fs::create_dir_all(&sources_dir)?;

        Ok(Self {
            temp_dir,
            project_dir,
            cache_dir,
            sources_dir,
            manifest: None,
        })
    }

    /// Set up test environment variables
    pub fn setup_env(&self) -> Result<()> {
        // Note: This should be used with care in tests to avoid race conditions
        // Consider passing env vars to Command instances instead
        std::env::set_var("CCPM_CACHE_DIR", &self.cache_dir);
        std::env::set_var("CCPM_TEST_MODE", "true");
        Ok(())
    }

    /// Get the project directory path
    pub fn project_path(&self) -> &Path {
        &self.project_dir
    }

    /// Get the cache directory path
    pub fn cache_path(&self) -> &Path {
        &self.cache_dir
    }

    /// Get the sources directory path
    pub fn sources_path(&self) -> &Path {
        &self.sources_dir
    }

    /// Get the temp directory path
    pub fn temp_path(&self) -> &Path {
        self.temp_dir.path()
    }

    /// Write a manifest file to the project directory
    pub fn write_manifest(&self, content: &str) -> Result<()> {
        let manifest_path = self.project_dir.join("ccpm.toml");
        fs::write(&manifest_path, content)
            .with_context(|| format!("Failed to write manifest to {:?}", manifest_path))?;
        Ok(())
    }

    /// Write a lockfile to the project directory
    pub fn write_lockfile(&self, content: &str) -> Result<()> {
        let lockfile_path = self.project_dir.join("ccpm.lock");
        fs::write(&lockfile_path, content)
            .with_context(|| format!("Failed to write lockfile to {:?}", lockfile_path))?;
        Ok(())
    }

    /// Create a local resource file
    pub fn create_local_resource(&self, path: &str, content: &str) -> Result<()> {
        let resource_path = self.project_dir.join(path);
        if let Some(parent) = resource_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&resource_path, content)?;
        Ok(())
    }

    /// Create a source repository with the given name
    pub fn create_source_repo(&self, name: &str) -> Result<TestSourceRepo> {
        let source_dir = self.sources_dir.join(name);
        fs::create_dir_all(&source_dir)?;

        let git = TestGit::new(&source_dir);
        git.init()?;
        git.config_user()?;

        Ok(TestSourceRepo {
            name: name.to_string(),
            path: source_dir,
            git,
        })
    }

    /// Run a CCPM command in the project directory
    pub fn run_ccpm(&self, args: &[&str]) -> Result<CommandOutput> {
        let ccpm_binary = env!("CARGO_BIN_EXE_ccpm");
        let output = Command::new(ccpm_binary)
            .args(args)
            .current_dir(&self.project_dir)
            .env("CCPM_CACHE_DIR", &self.cache_dir)
            .env("CCPM_TEST_MODE", "true")
            .env("NO_COLOR", "1")
            .output()
            .context("Failed to run ccpm command")?;

        Ok(CommandOutput {
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            success: output.status.success(),
            code: output.status.code(),
        })
    }
}

/// Test source repository helper
pub struct TestSourceRepo {
    pub name: String,
    pub path: PathBuf,
    pub git: TestGit,
}

impl TestSourceRepo {
    /// Add a resource file to the repository
    pub fn add_resource(&self, resource_type: &str, name: &str, content: &str) -> Result<()> {
        let resource_dir = self.path.join(resource_type);
        fs::create_dir_all(&resource_dir)?;

        let file_path = resource_dir.join(format!("{}.md", name));
        fs::write(&file_path, content)?;
        Ok(())
    }

    /// Create standard test resources
    pub fn create_standard_resources(&self) -> Result<()> {
        self.add_resource("agents", "test-agent", "# Test Agent\n\nA test agent")?;
        self.add_resource(
            "snippets",
            "test-snippet",
            "# Test Snippet\n\nA test snippet",
        )?;
        self.add_resource(
            "commands",
            "test-command",
            "# Test Command\n\nA test command",
        )?;
        Ok(())
    }

    /// Commit all changes with a message
    pub fn commit_all(&self, message: &str) -> Result<()> {
        self.git.add_all()?;
        self.git.commit(message)?;
        Ok(())
    }

    /// Create a version tag
    pub fn tag_version(&self, version: &str) -> Result<()> {
        self.git.tag(version)?;
        Ok(())
    }

    /// Get the file:// URL for this repository
    pub fn file_url(&self) -> String {
        let path_str = self.path.display().to_string().replace('\\', "/");
        format!("file://{}", path_str)
    }
}

/// Command output helper
pub struct CommandOutput {
    pub stdout: String,
    pub stderr: String,
    pub success: bool,
    pub code: Option<i32>,
}

impl CommandOutput {
    /// Assert the command succeeded
    pub fn assert_success(&self) -> &Self {
        assert!(
            self.success,
            "Command failed with code {:?}\nStderr: {}",
            self.code, self.stderr
        );
        self
    }

    /// Assert the command failed
    pub fn assert_failure(&self) -> &Self {
        assert!(
            !self.success,
            "Command unexpectedly succeeded\nStdout: {}",
            self.stdout
        );
        self
    }

    /// Assert stdout contains the given text
    pub fn assert_stdout_contains(&self, text: &str) -> &Self {
        assert!(
            self.stdout.contains(text),
            "Expected stdout to contain '{}'\nActual stdout: {}",
            text,
            self.stdout
        );
        self
    }

    /// Assert stderr contains the given text
    pub fn assert_stderr_contains(&self, text: &str) -> &Self {
        assert!(
            self.stderr.contains(text),
            "Expected stderr to contain '{}'\nActual stderr: {}",
            text,
            self.stderr
        );
        self
    }
}

/// File assertion helpers
pub struct FileAssert;

impl FileAssert {
    /// Assert a file exists
    pub fn exists(path: impl AsRef<Path>) {
        let path = path.as_ref();
        assert!(path.exists(), "Expected file to exist: {}", path.display());
    }

    /// Assert a file does not exist
    pub fn not_exists(path: impl AsRef<Path>) {
        let path = path.as_ref();
        assert!(
            !path.exists(),
            "Expected file to not exist: {}",
            path.display()
        );
    }

    /// Assert a file contains specific content
    pub fn contains(path: impl AsRef<Path>, expected: &str) {
        let path = path.as_ref();
        let content = fs::read_to_string(path)
            .unwrap_or_else(|e| panic!("Failed to read file {}: {}", path.display(), e));
        assert!(
            content.contains(expected),
            "Expected file {} to contain '{}'\nActual content: {}",
            path.display(),
            expected,
            content
        );
    }

    /// Assert a file has exact content
    pub fn equals(path: impl AsRef<Path>, expected: &str) {
        let path = path.as_ref();
        let content = fs::read_to_string(path)
            .unwrap_or_else(|e| panic!("Failed to read file {}: {}", path.display(), e));
        assert_eq!(
            content,
            expected,
            "File {} content mismatch",
            path.display()
        );
    }
}

/// Directory assertion helpers
pub struct DirAssert;

impl DirAssert {
    /// Assert a directory exists
    pub fn exists(path: impl AsRef<Path>) {
        let path = path.as_ref();
        assert!(
            path.is_dir(),
            "Expected directory to exist: {}",
            path.display()
        );
    }

    /// Assert a directory contains a file
    pub fn contains_file(dir: impl AsRef<Path>, file_name: &str) {
        let path = dir.as_ref().join(file_name);
        assert!(
            path.exists(),
            "Expected directory {} to contain file '{}'",
            dir.as_ref().display(),
            file_name
        );
    }

    /// Assert a directory is empty
    pub fn is_empty(path: impl AsRef<Path>) {
        let path = path.as_ref();
        let entries = fs::read_dir(path)
            .unwrap_or_else(|e| panic!("Failed to read directory {}: {}", path.display(), e))
            .count();
        assert_eq!(
            entries,
            0,
            "Expected directory {} to be empty, but it contains {} entries",
            path.display(),
            entries
        );
    }
}

/// Common test manifest templates
pub mod manifests {
    /// Basic manifest with standard dependencies
    pub const BASIC: &str = r#"
[sources]
official = "https://github.com/example/official.git"
community = "https://github.com/example/community.git"

[agents]
my-agent = { source = "official", path = "agents/my-agent.md", version = "v1.0.0" }
helper = { source = "community", path = "agents/helper.md", version = "^1.2.0" }

[snippets]
utils = { source = "official", path = "snippets/utils.md", version = "v1.0.0" }
"#;

    /// Manifest with local dependencies
    pub const WITH_LOCAL: &str = r#"
[sources]
official = "https://github.com/example/official.git"

[agents]
remote-agent = { source = "official", path = "agents/test.md", version = "v1.0.0" }
local-agent = { path = "./agents/local.md" }

[snippets]
local-snippet = { path = "./snippets/local.md" }
"#;

    /// Manifest with MCP servers
    pub const WITH_MCP: &str = r#"
[sources]
official = "https://github.com/example/official.git"

[agents]
my-agent = { source = "official", path = "agents/test.md", version = "v1.0.0" }

[mcp-servers]
filesystem = { command = "npx", args = ["-y", "@modelcontextprotocol/server-filesystem"] }
postgres = { command = "mcp-postgres", args = ["--connection", "${DATABASE_URL}"] }
"#;

    /// Empty but valid manifest
    pub const EMPTY: &str = "";

    /// Invalid TOML syntax
    pub const INVALID_SYNTAX: &str = r#"
[sources
official = "https://github.com/example/official.git"
"#;
}

/// Common lockfile templates
pub mod lockfiles {
    /// Basic lockfile matching BASIC manifest
    pub const BASIC: &str = r#"
version = 1

[[sources]]
name = "official"
url = "https://github.com/example/official.git"
commit = "abc123def456"

[[sources]]
name = "community"
url = "https://github.com/example/community.git"
commit = "789xyz012345"

[[agents]]
name = "my-agent"
source = "official"
path = "agents/my-agent.md"
version = "v1.0.0"
resolved_commit = "abc123def456"
installed_at = ".claude/agents/my-agent.md"

[[agents]]
name = "helper"
source = "community"
path = "agents/helper.md"
version = "^1.2.0"
resolved_version = "v1.2.5"
resolved_commit = "789xyz012345"
installed_at = ".claude/agents/helper.md"

[[snippets]]
name = "utils"
source = "official"
path = "snippets/utils.md"
version = "v1.0.0"
resolved_commit = "abc123def456"
installed_at = "snippets/utils.md"
"#;

    /// Empty lockfile
    pub const EMPTY: &str = "version = 1\n";
}

/// Test-specific environment variable guard
/// Automatically restores original value when dropped
pub struct EnvGuard {
    key: String,
    original: Option<String>,
}

impl EnvGuard {
    /// Set an environment variable and return a guard
    pub fn set(key: impl Into<String>, value: impl Into<String>) -> Self {
        let key = key.into();
        let original = std::env::var(&key).ok();
        std::env::set_var(&key, value.into());
        Self { key, original }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        match &self.original {
            Some(value) => std::env::set_var(&self.key, value),
            None => std::env::remove_var(&self.key),
        }
    }
}

/// Initialize test logging
pub fn init_test_logging() {
    // Simple test logging initialization
    // You can enhance this with env_logger if needed
    std::env::set_var("RUST_LOG", "debug");
}
