//! Test environment builder for simplified test setup
//!
//! This module provides a fluent builder API for creating test environments,
//! reducing boilerplate in tests.

use anyhow::Result;
use std::path::PathBuf;
use tempfile::TempDir;

use crate::cache::Cache;
use crate::lockfile::LockFile;
use crate::manifest::Manifest;

/// A builder for creating test environments with a fluent API
pub struct TestEnvironmentBuilder {
    temp_dir: TempDir,
    project_dir: PathBuf,
    cache_dir: PathBuf,
    manifest: Option<Manifest>,
    lockfile: Option<LockFile>,
    files: Vec<(String, String)>,
}

impl TestEnvironmentBuilder {
    /// Create a new test environment builder
    pub fn new() -> Result<Self> {
        let temp_dir = TempDir::new()?;
        let project_dir = temp_dir.path().to_path_buf();
        let cache_dir = temp_dir.path().join("cache");

        Ok(Self {
            temp_dir,
            project_dir,
            cache_dir,
            manifest: None,
            lockfile: None,
            files: Vec::new(),
        })
    }

    /// Set a custom project directory within the temp directory
    pub fn with_project_dir(mut self, name: &str) -> Self {
        self.project_dir = self.temp_dir.path().join(name);
        self
    }

    /// Add a manifest to the test environment
    pub fn with_manifest(mut self, manifest: Manifest) -> Self {
        self.manifest = Some(manifest);
        self
    }

    /// Add a lockfile to the test environment
    pub fn with_lockfile(mut self, lockfile: LockFile) -> Self {
        self.lockfile = Some(lockfile);
        self
    }

    /// Add a file to be created in the test environment
    pub fn with_file(mut self, path: impl Into<String>, content: impl Into<String>) -> Self {
        self.files.push((path.into(), content.into()));
        self
    }

    /// Add multiple files to be created in the test environment
    pub fn with_files(mut self, files: Vec<(&str, &str)>) -> Self {
        for (path, content) in files {
            self.files.push((path.to_string(), content.to_string()));
        }
        self
    }

    /// Build the test environment
    pub fn build(self) -> Result<TestEnvironment> {
        // Create directories
        std::fs::create_dir_all(&self.project_dir)?;
        std::fs::create_dir_all(&self.cache_dir)?;

        // Write manifest if provided
        let manifest_path = self.project_dir.join("agpm.toml");
        if let Some(manifest) = &self.manifest {
            let content = toml::to_string_pretty(manifest)?;
            std::fs::write(&manifest_path, content)?;
        }

        // Write lockfile if provided
        let lockfile_path = self.project_dir.join("agpm.lock");
        if let Some(lockfile) = &self.lockfile {
            lockfile.save(&lockfile_path)?;
        }

        // Create additional files
        for (path, content) in &self.files {
            let full_path = self.project_dir.join(path);
            if let Some(parent) = full_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(full_path, content)?;
        }

        // Create cache
        let cache = Cache::with_dir(self.cache_dir.clone())?;

        Ok(TestEnvironment {
            _temp_dir: self.temp_dir,
            project_dir: self.project_dir,
            cache,
            manifest_path,
            lockfile_path,
        })
    }
}

/// A built test environment
pub struct TestEnvironment {
    _temp_dir: TempDir, // Keep temp dir alive
    pub project_dir: PathBuf,
    pub cache: Cache,
    pub manifest_path: PathBuf,
    pub lockfile_path: PathBuf,
}

impl TestEnvironment {
    /// Create a new test environment builder
    pub fn builder() -> Result<TestEnvironmentBuilder> {
        TestEnvironmentBuilder::new()
    }

    /// Create a basic test environment with defaults
    pub fn new() -> Result<Self> {
        TestEnvironmentBuilder::new()?.build()
    }

    /// Create a test environment with a manifest
    pub fn with_manifest(manifest: Manifest) -> Result<Self> {
        TestEnvironmentBuilder::new()?
            .with_manifest(manifest)
            .build()
    }

    /// Load the manifest from the test environment
    pub fn load_manifest(&self) -> Result<Manifest> {
        Manifest::load(&self.manifest_path)
    }

    /// Load the lockfile from the test environment
    pub fn load_lockfile(&self) -> Result<Option<LockFile>> {
        if self.lockfile_path.exists() {
            Ok(Some(LockFile::load(&self.lockfile_path)?))
        } else {
            Ok(None)
        }
    }

    /// Save a lockfile to the test environment
    pub fn save_lockfile(&self, lockfile: &LockFile) -> Result<()> {
        lockfile.save(&self.lockfile_path)
    }

    /// Check if a file exists in the project directory
    pub fn file_exists(&self, path: impl AsRef<std::path::Path>) -> bool {
        self.project_dir.join(path).exists()
    }

    /// Read a file from the project directory
    pub fn read_file(&self, path: impl AsRef<std::path::Path>) -> Result<String> {
        Ok(std::fs::read_to_string(self.project_dir.join(path))?)
    }

    /// Write a file to the project directory
    pub fn write_file(
        &self,
        path: impl AsRef<std::path::Path>,
        content: impl AsRef<str>,
    ) -> Result<()> {
        let full_path = self.project_dir.join(path);
        if let Some(parent) = full_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(full_path, content.as_ref())?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builder_creates_environment() {
        let env = TestEnvironment::builder()
            .unwrap()
            .with_file("test.txt", "test content")
            .with_file("dir/nested.txt", "nested content")
            .build()
            .unwrap();

        assert!(env.file_exists("test.txt"));
        assert!(env.file_exists("dir/nested.txt"));
        assert_eq!(env.read_file("test.txt").unwrap(), "test content");
        assert_eq!(env.read_file("dir/nested.txt").unwrap(), "nested content");
    }

    #[test]
    fn test_builder_with_manifest() {
        let manifest = Manifest::default();
        let env = TestEnvironment::with_manifest(manifest).unwrap();

        assert!(env.file_exists("agpm.toml"));
        let loaded = env.load_manifest().unwrap();
        assert_eq!(loaded.sources.len(), 0);
    }

    #[test]
    fn test_builder_with_multiple_files() {
        let env = TestEnvironment::builder()
            .unwrap()
            .with_files(vec![
                ("file1.txt", "content1"),
                ("file2.txt", "content2"),
                ("dir/file3.txt", "content3"),
            ])
            .build()
            .unwrap();

        assert!(env.file_exists("file1.txt"));
        assert!(env.file_exists("file2.txt"));
        assert!(env.file_exists("dir/file3.txt"));
    }
}
