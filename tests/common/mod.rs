//! Common test utilities and fixtures for CCPM integration tests
//!
//! This module consolidates frequently used test patterns to reduce duplication
//! and improve test maintainability.

// Allow dead code because these utilities are used across different test files
// and not all utilities are used in every test file
#![allow(dead_code)]

use anyhow::{Context, Result};
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
    _temp_dir: TempDir, // Keep alive for RAII cleanup
    project_dir: PathBuf,
    cache_dir: PathBuf,
    sources_dir: PathBuf,
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
            _temp_dir: temp_dir,
            project_dir,
            cache_dir,
            sources_dir,
        })
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

    /// Write a manifest file to the project directory
    pub fn write_manifest(&self, content: &str) -> Result<()> {
        let manifest_path = self.project_dir.join("ccpm.toml");
        fs::write(&manifest_path, content)
            .with_context(|| format!("Failed to write manifest to {:?}", manifest_path))?;
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
