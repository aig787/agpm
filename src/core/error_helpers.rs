//! Error handling helper functions and utilities
//!
//! This module provides common error handling patterns used throughout CCPM,
//! reducing boilerplate and ensuring consistent error messages.

use anyhow::{Context, Result};
use std::path::Path;

use crate::manifest::Manifest;
use crate::markdown::MarkdownFile;

/// Common file operations with consistent error handling
pub trait FileOperations {
    /// Read a file with appropriate error context
    fn read_file_with_context(path: impl AsRef<Path>) -> Result<String> {
        let path = path.as_ref();
        std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read file: {}", path.display()))
    }

    /// Write to a file with appropriate error context
    fn write_file_with_context(path: impl AsRef<Path>, content: impl AsRef<str>) -> Result<()> {
        let path = path.as_ref();
        std::fs::write(path, content.as_ref())
            .with_context(|| format!("Failed to write file: {}", path.display()))
    }

    /// Create a directory with appropriate error context
    fn create_dir_with_context(path: impl AsRef<Path>) -> Result<()> {
        let path = path.as_ref();
        std::fs::create_dir_all(path)
            .with_context(|| format!("Failed to create directory: {}", path.display()))
    }

    /// Read a file as bytes with appropriate error context
    fn read_bytes_with_context(path: impl AsRef<Path>) -> Result<Vec<u8>> {
        let path = path.as_ref();
        std::fs::read(path).with_context(|| format!("Failed to read file: {}", path.display()))
    }

    /// Write bytes to a file with appropriate error context
    fn write_bytes_with_context(path: impl AsRef<Path>, content: impl AsRef<[u8]>) -> Result<()> {
        let path = path.as_ref();
        std::fs::write(path, content.as_ref())
            .with_context(|| format!("Failed to write file: {}", path.display()))
    }

    /// Copy a file with appropriate error context
    fn copy_file_with_context(from: impl AsRef<Path>, to: impl AsRef<Path>) -> Result<u64> {
        let from = from.as_ref();
        let to = to.as_ref();
        std::fs::copy(from, to).with_context(|| {
            format!(
                "Failed to copy file from {} to {}",
                from.display(),
                to.display()
            )
        })
    }

    /// Remove a file with appropriate error context
    fn remove_file_with_context(path: impl AsRef<Path>) -> Result<()> {
        let path = path.as_ref();
        std::fs::remove_file(path)
            .with_context(|| format!("Failed to remove file: {}", path.display()))
    }

    /// Remove a directory recursively with appropriate error context
    fn remove_dir_all_with_context(path: impl AsRef<Path>) -> Result<()> {
        let path = path.as_ref();
        std::fs::remove_dir_all(path)
            .with_context(|| format!("Failed to remove directory: {}", path.display()))
    }

    /// Check if a path exists, returning error if checking fails
    fn check_exists_with_context(path: impl AsRef<Path>) -> Result<bool> {
        let path = path.as_ref();
        path.try_exists()
            .with_context(|| format!("Failed to check if path exists: {}", path.display()))
    }
}

/// Implement FileOperations for a unit struct to enable trait usage
pub struct FileOps;
impl FileOperations for FileOps {}

/// Common manifest operations with consistent error handling
pub trait ManifestOperations {
    /// Load a manifest with appropriate error context
    fn load_manifest_with_context(path: impl AsRef<Path>) -> Result<Manifest> {
        let path = path.as_ref();
        Manifest::load(path)
            .with_context(|| format!("Failed to parse manifest file: {}", path.display()))
    }

    /// Save a manifest with appropriate error context
    fn save_manifest_with_context(manifest: &Manifest, path: impl AsRef<Path>) -> Result<()> {
        let path = path.as_ref();
        let content =
            toml::to_string_pretty(manifest).with_context(|| "Failed to serialize manifest")?;
        FileOps::write_file_with_context(path, content)
    }
}

/// Implement ManifestOperations for a unit struct to enable trait usage
pub struct ManifestOps;
impl ManifestOperations for ManifestOps {}

/// Common markdown operations with consistent error handling
pub trait MarkdownOperations {
    /// Parse markdown content with appropriate error context
    fn parse_markdown_with_context(
        content: impl AsRef<str>,
        path: impl AsRef<Path>,
    ) -> Result<MarkdownFile> {
        let path = path.as_ref();
        MarkdownFile::parse(content.as_ref())
            .with_context(|| format!("Invalid markdown file: {}", path.display()))
    }

    /// Read and parse a markdown file with appropriate error context
    fn read_markdown_with_context(path: impl AsRef<Path>) -> Result<MarkdownFile> {
        let path = path.as_ref();
        let content = FileOps::read_file_with_context(path)?;
        Self::parse_markdown_with_context(content, path)
    }
}

/// Implement MarkdownOperations for a unit struct to enable trait usage
pub struct MarkdownOps;
impl MarkdownOperations for MarkdownOps {}

/// Common lockfile operations with consistent error handling
pub trait LockfileOperations {
    /// Load a lockfile with appropriate error context
    fn load_lockfile_with_context(path: impl AsRef<Path>) -> Result<crate::lockfile::LockFile> {
        let path = path.as_ref();
        crate::lockfile::LockFile::load(path)
            .with_context(|| format!("Failed to load lockfile: {}", path.display()))
    }

    /// Save a lockfile with appropriate error context
    fn save_lockfile_with_context(
        lockfile: &crate::lockfile::LockFile,
        path: impl AsRef<Path>,
    ) -> Result<()> {
        let path = path.as_ref();
        lockfile
            .save(path)
            .with_context(|| format!("Failed to save lockfile: {}", path.display()))
    }
}

/// Implement LockfileOperations for a unit struct to enable trait usage
pub struct LockfileOps;
impl LockfileOperations for LockfileOps {}

/// Common JSON operations with consistent error handling
pub trait JsonOperations {
    /// Read and parse a JSON file with appropriate error context
    fn read_json_with_context<T: serde::de::DeserializeOwned>(path: impl AsRef<Path>) -> Result<T> {
        let path = path.as_ref();
        let content = FileOps::read_file_with_context(path)?;
        serde_json::from_str(&content)
            .with_context(|| format!("Failed to parse JSON file: {}", path.display()))
    }

    /// Serialize and write a JSON file with appropriate error context
    fn write_json_with_context<T: serde::Serialize>(
        value: &T,
        path: impl AsRef<Path>,
    ) -> Result<()> {
        let path = path.as_ref();
        let content =
            serde_json::to_string_pretty(value).with_context(|| "Failed to serialize to JSON")?;
        FileOps::write_file_with_context(path, content)
    }
}

/// Implement JsonOperations for a unit struct to enable trait usage
pub struct JsonOps;
impl JsonOperations for JsonOps {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_file_operations() {
        let temp = TempDir::new().unwrap();
        let file_path = temp.path().join("test.txt");

        // Test write and read
        FileOps::write_file_with_context(&file_path, "test content").unwrap();
        let content = FileOps::read_file_with_context(&file_path).unwrap();
        assert_eq!(content, "test content");

        // Test exists check
        assert!(FileOps::check_exists_with_context(&file_path).unwrap());

        // Test remove
        FileOps::remove_file_with_context(&file_path).unwrap();
        assert!(!FileOps::check_exists_with_context(&file_path).unwrap());
    }

    #[test]
    fn test_directory_operations() {
        let temp = TempDir::new().unwrap();
        let dir_path = temp.path().join("test_dir").join("nested");

        // Test create nested directories
        FileOps::create_dir_with_context(&dir_path).unwrap();
        assert!(dir_path.exists());

        // Test remove directory tree
        let parent = temp.path().join("test_dir");
        FileOps::remove_dir_all_with_context(&parent).unwrap();
        assert!(!parent.exists());
    }

    #[test]
    fn test_json_operations() {
        let temp = TempDir::new().unwrap();
        let json_path = temp.path().join("test.json");

        #[derive(serde::Serialize, serde::Deserialize, PartialEq, Debug)]
        struct TestStruct {
            field: String,
            number: i32,
        }

        let test_data = TestStruct {
            field: "test".to_string(),
            number: 42,
        };

        // Test write and read
        JsonOps::write_json_with_context(&test_data, &json_path).unwrap();
        let loaded: TestStruct = JsonOps::read_json_with_context(&json_path).unwrap();
        assert_eq!(loaded, test_data);
    }

    #[test]
    fn test_read_bytes_with_context() {
        let temp = TempDir::new().unwrap();
        let file_path = temp.path().join("test_bytes.bin");
        let test_bytes = b"binary\x00\x01\x02\x03data";

        // Write bytes directly using std::fs
        fs::write(&file_path, test_bytes).unwrap();

        // Test reading bytes with context
        let read_bytes = FileOps::read_bytes_with_context(&file_path).unwrap();
        assert_eq!(read_bytes, test_bytes);

        // Test error case - non-existent file
        let missing_path = temp.path().join("missing.bin");
        let result = FileOps::read_bytes_with_context(&missing_path);
        assert!(result.is_err());
        let error_msg = result.unwrap_err().to_string();
        assert!(error_msg.contains("Failed to read file"));
        assert!(error_msg.contains("missing.bin"));
    }

    #[test]
    fn test_write_bytes_with_context() {
        let temp = TempDir::new().unwrap();
        let file_path = temp.path().join("test_write_bytes.bin");
        let test_bytes = b"binary\x00\x01\x02\x03data";

        // Test writing bytes with context
        FileOps::write_bytes_with_context(&file_path, test_bytes).unwrap();

        // Verify content
        let read_bytes = fs::read(&file_path).unwrap();
        assert_eq!(read_bytes, test_bytes);

        // Test error case - invalid path (readonly parent)
        let readonly_dir = temp.path().join("readonly");
        fs::create_dir(&readonly_dir).unwrap();
        let mut perms = fs::metadata(&readonly_dir).unwrap().permissions();
        perms.set_readonly(true);
        fs::set_permissions(&readonly_dir, perms).unwrap();

        let readonly_file = readonly_dir.join("test.bin");
        let result = FileOps::write_bytes_with_context(&readonly_file, test_bytes);

        // Reset permissions for cleanup
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = fs::Permissions::from_mode(0o755);
            fs::set_permissions(&readonly_dir, perms).unwrap();
        }
        #[cfg(not(unix))]
        {
            let mut perms = fs::metadata(&readonly_dir).unwrap().permissions();
            #[allow(clippy::permissions_set_readonly_false)]
            perms.set_readonly(false);
            fs::set_permissions(&readonly_dir, perms).unwrap();
        }

        // On some systems, writing to readonly directories might still work,
        // so we just check that the function doesn't panic
        if let Err(err) = result {
            let error_msg = err.to_string();
            assert!(error_msg.contains("Failed to write file"));
        }
    }

    #[test]
    fn test_copy_file_with_context() {
        let temp = TempDir::new().unwrap();
        let source_path = temp.path().join("source.txt");
        let dest_path = temp.path().join("destination.txt");
        let test_content = "file copy test content";

        // Create source file
        fs::write(&source_path, test_content).unwrap();

        // Test copying file with context
        let bytes_copied = FileOps::copy_file_with_context(&source_path, &dest_path).unwrap();
        assert_eq!(bytes_copied, test_content.len() as u64);

        // Verify destination content
        let copied_content = fs::read_to_string(&dest_path).unwrap();
        assert_eq!(copied_content, test_content);

        // Test error case - source file doesn't exist
        let missing_source = temp.path().join("missing_source.txt");
        let another_dest = temp.path().join("another_dest.txt");
        let result = FileOps::copy_file_with_context(&missing_source, &another_dest);
        assert!(result.is_err());
        let error_msg = result.unwrap_err().to_string();
        assert!(error_msg.contains("Failed to copy file"));
        assert!(error_msg.contains("missing_source.txt"));

        // Test error case - destination directory doesn't exist
        let nonexistent_dest = temp.path().join("nonexistent").join("dest.txt");
        let result = FileOps::copy_file_with_context(&source_path, &nonexistent_dest);
        assert!(result.is_err());
        let error_msg = result.unwrap_err().to_string();
        assert!(error_msg.contains("Failed to copy file"));
    }

    #[test]
    fn test_manifest_operations_load() {
        let temp = TempDir::new().unwrap();
        let manifest_path = temp.path().join("ccpm.toml");

        // Create a valid manifest file
        let manifest_content = r#"
[sources]
test = "https://github.com/test/test.git"

[agents]
test-agent = { source = "test", path = "agents/test.md", version = "v1.0.0" }
"#;
        fs::write(&manifest_path, manifest_content).unwrap();

        // Test loading manifest with context
        let manifest = ManifestOps::load_manifest_with_context(&manifest_path).unwrap();
        assert!(manifest.sources.contains_key("test"));
        assert!(manifest.agents.contains_key("test-agent"));

        // Test error case - non-existent file
        let missing_path = temp.path().join("missing.toml");
        let result = ManifestOps::load_manifest_with_context(&missing_path);
        assert!(result.is_err());
        let error_msg = result.unwrap_err().to_string();
        assert!(error_msg.contains("Failed to parse manifest file"));
        assert!(error_msg.contains("missing.toml"));

        // Test error case - invalid TOML
        let invalid_manifest_path = temp.path().join("invalid.toml");
        let invalid_content = "this is not valid toml [[[";
        fs::write(&invalid_manifest_path, invalid_content).unwrap();
        let result = ManifestOps::load_manifest_with_context(&invalid_manifest_path);
        assert!(result.is_err());
        let error_msg = result.unwrap_err().to_string();
        assert!(error_msg.contains("Failed to parse manifest file"));
    }

    #[test]
    fn test_manifest_operations_save() {
        let temp = TempDir::new().unwrap();
        let manifest_path = temp.path().join("test_save.toml");

        // Create a manifest to save
        let manifest = crate::manifest::Manifest::new();

        // Test saving manifest with context
        ManifestOps::save_manifest_with_context(&manifest, &manifest_path).unwrap();
        assert!(manifest_path.exists());

        // Verify the saved content can be loaded back
        let loaded_manifest = ManifestOps::load_manifest_with_context(&manifest_path).unwrap();
        assert_eq!(manifest.sources.len(), loaded_manifest.sources.len());
        assert_eq!(manifest.agents.len(), loaded_manifest.agents.len());
    }

    #[test]
    fn test_markdown_operations_parse() {
        let temp = TempDir::new().unwrap();
        let md_path = temp.path().join("test.md");

        // Test parsing valid markdown with frontmatter
        let markdown_content = r#"---
title: "Test Agent"
version: "1.0.0"
---

# Test Agent

This is a test agent.
"#;

        let markdown =
            MarkdownOps::parse_markdown_with_context(markdown_content, &md_path).unwrap();
        assert_eq!(
            markdown.content.trim(),
            "# Test Agent\n\nThis is a test agent."
        );
        assert!(markdown.get_title().is_some());
        assert_eq!(markdown.get_title().unwrap(), "Test Agent");

        // Test parsing markdown without frontmatter
        let simple_content = "# Simple Agent\n\nThis is simple.";
        let simple_markdown =
            MarkdownOps::parse_markdown_with_context(simple_content, &md_path).unwrap();
        assert_eq!(
            simple_markdown.content.trim(),
            "# Simple Agent\n\nThis is simple."
        );
        // get_title() should extract title from the # heading
        assert_eq!(simple_markdown.get_title().unwrap(), "Simple Agent");

        // Test parsing markdown without frontmatter or headings
        let plain_content = "This is plain content without headings.";
        let plain_markdown =
            MarkdownOps::parse_markdown_with_context(plain_content, &md_path).unwrap();
        assert_eq!(
            plain_markdown.content.trim(),
            "This is plain content without headings."
        );
        assert!(plain_markdown.get_title().is_none());

        // Test error case - invalid YAML frontmatter
        let invalid_content = r#"---
title: "Test Agent
invalid yaml here
---

# Test Agent
"#;
        let result = MarkdownOps::parse_markdown_with_context(invalid_content, &md_path);
        assert!(result.is_err());
        let error_msg = result.unwrap_err().to_string();
        assert!(error_msg.contains("Invalid markdown file"));
        assert!(error_msg.contains("test.md"));
    }

    #[test]
    fn test_markdown_operations_read() {
        let temp = TempDir::new().unwrap();
        let md_path = temp.path().join("test_read.md");

        // Create a markdown file
        let markdown_content = r#"---
title: "Test Agent"
version: "1.0.0"
---

# Test Agent

This is a test agent for reading.
"#;
        fs::write(&md_path, markdown_content).unwrap();

        // Test reading markdown with context
        let markdown = MarkdownOps::read_markdown_with_context(&md_path).unwrap();
        assert_eq!(markdown.get_title().unwrap(), "Test Agent");
        assert!(
            markdown
                .content
                .contains("This is a test agent for reading")
        );

        // Test error case - non-existent file
        let missing_path = temp.path().join("missing.md");
        let result = MarkdownOps::read_markdown_with_context(&missing_path);
        assert!(result.is_err());
        let error_msg = result.unwrap_err().to_string();
        assert!(error_msg.contains("Failed to read file"));
        assert!(error_msg.contains("missing.md"));
    }

    #[test]
    fn test_lockfile_operations_load() {
        let temp = TempDir::new().unwrap();
        let lockfile_path = temp.path().join("ccpm.lock");

        // Test loading non-existent lockfile (should create new)
        let lockfile = LockfileOps::load_lockfile_with_context(&lockfile_path).unwrap();
        assert_eq!(lockfile.version, 1);
        assert!(lockfile.sources.is_empty());

        // Create a valid lockfile
        let lockfile_content = r#"# Auto-generated lockfile - DO NOT EDIT
version = 1

[[sources]]
name = "test"
url = "https://github.com/test/test.git"
commit = "abc123"
fetched_at = "2024-01-01T00:00:00Z"
"#;
        fs::write(&lockfile_path, lockfile_content).unwrap();

        // Test loading existing lockfile
        let loaded_lockfile = LockfileOps::load_lockfile_with_context(&lockfile_path).unwrap();
        assert_eq!(loaded_lockfile.version, 1);
        assert!(!loaded_lockfile.sources.is_empty());

        // Test error case - invalid lockfile format
        let invalid_lockfile_path = temp.path().join("invalid.lock");
        let invalid_content = "this is not valid toml [[[";
        fs::write(&invalid_lockfile_path, invalid_content).unwrap();
        let result = LockfileOps::load_lockfile_with_context(&invalid_lockfile_path);
        assert!(result.is_err());
        let error_msg = result.unwrap_err().to_string();
        assert!(error_msg.contains("Failed to load lockfile"));
        assert!(error_msg.contains("invalid.lock"));
    }

    #[test]
    fn test_lockfile_operations_save() {
        let temp = TempDir::new().unwrap();
        let lockfile_path = temp.path().join("test_save.lock");

        // Create a lockfile to save
        let lockfile = crate::lockfile::LockFile::new();

        // Test saving lockfile with context
        LockfileOps::save_lockfile_with_context(&lockfile, &lockfile_path).unwrap();
        assert!(lockfile_path.exists());

        // Verify the saved content
        let content = fs::read_to_string(&lockfile_path).unwrap();
        assert!(content.contains("Auto-generated lockfile"));
        assert!(content.contains("version = 1"));

        // Verify it can be loaded back
        let loaded_lockfile = LockfileOps::load_lockfile_with_context(&lockfile_path).unwrap();
        assert_eq!(lockfile.version, loaded_lockfile.version);
    }

    #[test]
    fn test_json_operations_error_cases() {
        let temp = TempDir::new().unwrap();

        // Test read error - non-existent file
        let missing_json = temp.path().join("missing.json");
        let result: Result<serde_json::Value> = JsonOps::read_json_with_context(&missing_json);
        assert!(result.is_err());
        let error_msg = result.unwrap_err().to_string();
        assert!(error_msg.contains("Failed to read file"));
        assert!(error_msg.contains("missing.json"));

        // Test parse error - invalid JSON
        let invalid_json_path = temp.path().join("invalid.json");
        let invalid_json = r#"{ "field": "value" invalid json }"#;
        fs::write(&invalid_json_path, invalid_json).unwrap();

        let result: Result<serde_json::Value> = JsonOps::read_json_with_context(&invalid_json_path);
        assert!(result.is_err());
        let error_msg = result.unwrap_err().to_string();
        assert!(error_msg.contains("Failed to parse JSON file"));
        assert!(error_msg.contains("invalid.json"));

        // Test write error with unserializable data
        // Note: Most standard types are serializable, so this test verifies the
        // error context path is working correctly by testing the success case
        let json_path = temp.path().join("test_write_error.json");
        let test_data = serde_json::json!({"test": "value"});

        JsonOps::write_json_with_context(&test_data, &json_path).unwrap();
        assert!(json_path.exists());

        let loaded: serde_json::Value = JsonOps::read_json_with_context(&json_path).unwrap();
        assert_eq!(loaded, test_data);
    }

    #[test]
    fn test_file_operations_error_contexts() {
        let temp = TempDir::new().unwrap();

        // Test read_file_with_context error
        let missing_file = temp.path().join("missing.txt");
        let result = FileOps::read_file_with_context(&missing_file);
        assert!(result.is_err());
        let error_msg = result.unwrap_err().to_string();
        assert!(error_msg.contains("Failed to read file"));
        assert!(error_msg.contains("missing.txt"));

        // Test write_file_with_context error
        let readonly_dir = temp.path().join("readonly");
        fs::create_dir(&readonly_dir).unwrap();
        let mut perms = fs::metadata(&readonly_dir).unwrap().permissions();
        perms.set_readonly(true);
        fs::set_permissions(&readonly_dir, perms).unwrap();

        let readonly_file = readonly_dir.join("test.txt");
        let result = FileOps::write_file_with_context(&readonly_file, "test");

        // Reset permissions for cleanup
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = fs::Permissions::from_mode(0o755);
            fs::set_permissions(&readonly_dir, perms).unwrap();
        }
        #[cfg(not(unix))]
        {
            let mut perms = fs::metadata(&readonly_dir).unwrap().permissions();
            #[allow(clippy::permissions_set_readonly_false)]
            perms.set_readonly(false);
            fs::set_permissions(&readonly_dir, perms).unwrap();
        }

        if let Err(err) = result {
            let error_msg = err.to_string();
            assert!(error_msg.contains("Failed to write file"));
        }

        // Test create_dir_with_context error (trying to create in non-existent parent)
        // This should work on most systems, so we test the success case
        let nested_dir = temp.path().join("nested").join("deep");
        FileOps::create_dir_with_context(&nested_dir).unwrap();
        assert!(nested_dir.exists());

        // Test remove_file_with_context error
        let nonexistent_file = temp.path().join("nonexistent.txt");
        let result = FileOps::remove_file_with_context(&nonexistent_file);
        assert!(result.is_err());
        let error_msg = result.unwrap_err().to_string();
        assert!(error_msg.contains("Failed to remove file"));
        assert!(error_msg.contains("nonexistent.txt"));

        // Test check_exists_with_context success and error cases
        let existing_file = temp.path().join("existing.txt");
        fs::write(&existing_file, "test").unwrap();
        assert!(FileOps::check_exists_with_context(&existing_file).unwrap());
        assert!(!FileOps::check_exists_with_context(&nonexistent_file).unwrap());
    }
}
