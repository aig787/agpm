//! File format operations for reading and writing structured data files.
//!
//! This module provides convenience functions for working with common file formats:
//! - Plain text files
//! - JSON (with pretty printing option)
//! - TOML (always pretty printed)
//! - YAML
//!
//! All write operations use atomic writes via [`super::atomic::safe_write`] to ensure
//! data integrity.
//!
//! # Examples
//!
//! ```rust,no_run
//! use agpm_cli::utils::fs::formats::{read_json_file, write_json_file};
//! use serde::{Deserialize, Serialize};
//! use std::path::Path;
//!
//! #[derive(Serialize, Deserialize)]
//! struct Config {
//!     name: String,
//!     version: String,
//! }
//!
//! # fn example() -> anyhow::Result<()> {
//! let config = Config {
//!     name: "agpm".to_string(),
//!     version: "1.0.0".to_string(),
//! };
//!
//! // Write with pretty formatting
//! write_json_file(Path::new("config.json"), &config, true)?;
//!
//! // Read back
//! let loaded: Config = read_json_file(Path::new("config.json"))?;
//! # Ok(())
//! # }
//! ```

use crate::core::file_error::{FileOperation, FileResultExt};
use anyhow::{Context, Result};
use std::fs;
use std::path::Path;
use tokio_retry::Retry;
use tokio_retry::strategy::ExponentialBackoff;

/// Reads a text file with proper error handling and context.
///
/// # Arguments
/// * `path` - The path to the file to read
///
/// # Returns
/// The contents of the file as a String
///
/// # Errors
/// Returns an error with context if the file cannot be read
pub fn read_text_file(path: &Path) -> Result<String> {
    Ok(fs::read_to_string(path).with_file_context(
        FileOperation::Read,
        path,
        "reading text file",
        "utils::fs::formats::read_text_file",
    )?)
}

/// Reads a text file asynchronously with retry for filesystem coherency delays.
///
/// Git worktrees can have brief visibility delays after creation, especially
/// under high parallel I/O load. This function uses `tokio-retry` with
/// exponential backoff to handle transient `NotFound` errors.
///
/// # Arguments
/// * `path` - The path to the file to read
///
/// # Returns
/// The contents of the file as a String
///
/// # Errors
/// Returns an error with context if the file cannot be read after all retries
///
/// # Retry Strategy
/// - Initial delay: 10ms
/// - Max delay: 200ms (capped)
/// - Max attempts: 5
/// - Only retries on `NotFound` errors; other errors fail immediately
pub async fn read_text_file_with_retry(path: &Path) -> Result<String> {
    let strategy = ExponentialBackoff::from_millis(10)
        .max_delay(std::time::Duration::from_millis(200))
        .take(5);

    let path_buf = path.to_path_buf();
    let path_for_error = path.to_path_buf();

    Retry::spawn(strategy, || {
        let path = path_buf.clone();
        async move {
            match tokio::fs::read_to_string(&path).await {
                Ok(content) => Ok(content),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    tracing::debug!(
                        target: "fs::retry",
                        "File not found at {}, will retry",
                        path.display()
                    );
                    Err(e)
                }
                // Don't retry other errors (permission denied, etc.)
                Err(e) => {
                    tracing::warn!(
                        target: "fs::retry",
                        "Non-retryable error reading {}: {:?} (kind: {:?})",
                        path.display(),
                        e,
                        e.kind()
                    );
                    Ok(Err(e)?)
                }
            }
        }
    })
    .await
    .map_err(|e| {
        tracing::warn!(
            target: "fs::retry",
            "All retries exhausted for {}: {:?} (kind: {:?})",
            path_for_error.display(),
            e,
            e.kind()
        );
        let file_error = crate::core::file_error::FileOperationError::new(
            crate::core::file_error::FileOperationContext::new(
                FileOperation::Read,
                &path_for_error,
                "reading text file with retry".to_string(),
                "utils::fs::formats::read_text_file_with_retry",
            ),
            e,
        );
        anyhow::Error::from(file_error)
    })
}

/// Writes a text file atomically with proper error handling.
///
/// # Arguments
/// * `path` - The path to write to
/// * `content` - The text content to write
///
/// # Returns
/// Ok(()) on success
///
/// # Errors
/// Returns an error with context if the file cannot be written
pub fn write_text_file(path: &Path, content: &str) -> Result<()> {
    super::atomic::safe_write(path, content)
        .with_context(|| format!("Failed to write file: {}", path.display()))
}

/// Reads and parses a JSON file.
///
/// # Arguments
/// * `path` - The path to the JSON file
///
/// # Type Parameters
/// * `T` - The type to deserialize into (must implement `DeserializeOwned`)
///
/// # Returns
/// The parsed JSON data
///
/// # Errors
/// Returns an error if the file cannot be read or parsed
pub fn read_json_file<T>(path: &Path) -> Result<T>
where
    T: serde::de::DeserializeOwned,
{
    let content = read_text_file(path)?;
    serde_json::from_str(&content)
        .with_context(|| format!("Failed to parse JSON from file: {}", path.display()))
}

/// Writes data as JSON to a file atomically.
///
/// # Arguments
/// * `path` - The path to write to
/// * `data` - The data to serialize
/// * `pretty` - Whether to use pretty formatting
///
/// # Type Parameters
/// * `T` - The type to serialize (must implement Serialize)
///
/// # Returns
/// Ok(()) on success
///
/// # Errors
/// Returns an error if serialization fails or the file cannot be written
pub fn write_json_file<T>(path: &Path, data: &T, pretty: bool) -> Result<()>
where
    T: serde::Serialize,
{
    let json = if pretty {
        serde_json::to_string_pretty(data)?
    } else {
        serde_json::to_string(data)?
    };

    write_text_file(path, &json)
        .with_context(|| format!("Failed to write JSON file: {}", path.display()))
}

/// Reads and parses a TOML file.
///
/// # Arguments
/// * `path` - The path to the TOML file
///
/// # Type Parameters
/// * `T` - The type to deserialize into (must implement `DeserializeOwned`)
///
/// # Returns
/// The parsed TOML data
///
/// # Errors
/// Returns an error if the file cannot be read or parsed
pub fn read_toml_file<T>(path: &Path) -> Result<T>
where
    T: serde::de::DeserializeOwned,
{
    let content = read_text_file(path)?;
    toml::from_str(&content)
        .with_context(|| format!("Failed to parse TOML from file: {}", path.display()))
}

/// Writes data as TOML to a file atomically.
///
/// # Arguments
/// * `path` - The path to write to
/// * `data` - The data to serialize
///
/// # Type Parameters
/// * `T` - The type to serialize (must implement Serialize)
///
/// # Returns
/// Ok(()) on success
///
/// # Errors
/// Returns an error if serialization fails or the file cannot be written
///
/// # Note
/// TOML is always pretty-printed for readability
pub fn write_toml_file<T>(path: &Path, data: &T) -> Result<()>
where
    T: serde::Serialize,
{
    let toml = toml::to_string_pretty(data)
        .with_context(|| format!("Failed to serialize data to TOML for: {}", path.display()))?;

    write_text_file(path, &toml)
        .with_context(|| format!("Failed to write TOML file: {}", path.display()))
}

/// Reads and parses a YAML file.
///
/// # Arguments
/// * `path` - The path to the YAML file
///
/// # Type Parameters
/// * `T` - The type to deserialize into (must implement `DeserializeOwned`)
///
/// # Returns
/// The parsed YAML data
///
/// # Errors
/// Returns an error if the file cannot be read or parsed
pub fn read_yaml_file<T>(path: &Path) -> Result<T>
where
    T: serde::de::DeserializeOwned,
{
    let content = read_text_file(path)?;
    serde_yaml::from_str(&content)
        .with_context(|| format!("Failed to parse YAML from file: {}", path.display()))
}

/// Writes data as YAML to a file atomically.
///
/// # Arguments
/// * `path` - The path to write to
/// * `data` - The data to serialize
///
/// # Type Parameters
/// * `T` - The type to serialize (must implement Serialize)
///
/// # Returns
/// Ok(()) on success
///
/// # Errors
/// Returns an error if serialization fails or the file cannot be written
pub fn write_yaml_file<T>(path: &Path, data: &T) -> Result<()>
where
    T: serde::Serialize,
{
    let yaml = serde_yaml::to_string(data)
        .with_context(|| format!("Failed to serialize data to YAML for: {}", path.display()))?;

    write_text_file(path, &yaml)
        .with_context(|| format!("Failed to write YAML file: {}", path.display()))
}

/// Creates a temporary file with content for testing.
///
/// # Arguments
/// * `prefix` - The prefix for the temp file name
/// * `content` - The content to write to the file
///
/// # Returns
/// A `TempPath` that will delete the file when dropped
///
/// # Errors
/// Returns an error if the temp file cannot be created
pub fn create_temp_file(prefix: &str, content: &str) -> Result<tempfile::TempPath> {
    let temp_file = tempfile::Builder::new().prefix(prefix).suffix(".tmp").tempfile()?;

    let path = temp_file.into_temp_path();
    write_text_file(&path, content)?;

    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};
    use tempfile::tempdir;

    #[derive(Debug, Serialize, Deserialize, PartialEq)]
    struct TestData {
        name: String,
        value: i32,
    }

    #[test]
    fn test_read_write_text_file() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("test.txt");

        write_text_file(&path, "test content").unwrap();
        let content = read_text_file(&path).unwrap();
        assert_eq!(content, "test content");
    }

    #[test]
    fn test_read_write_json_file() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("test.json");

        let data = TestData {
            name: "test".to_string(),
            value: 42,
        };

        write_json_file(&path, &data, true).unwrap();
        let loaded: TestData = read_json_file(&path).unwrap();
        assert_eq!(loaded, data);
    }

    #[test]
    fn test_read_write_json_file_compact() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("test.json");

        let data = TestData {
            name: "test".to_string(),
            value: 42,
        };

        write_json_file(&path, &data, false).unwrap();
        let content = read_text_file(&path).unwrap();
        assert!(!content.contains('\n')); // Compact format
        let loaded: TestData = read_json_file(&path).unwrap();
        assert_eq!(loaded, data);
    }

    #[test]
    fn test_read_write_toml_file() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("test.toml");

        let data = TestData {
            name: "test".to_string(),
            value: 42,
        };

        write_toml_file(&path, &data).unwrap();
        let loaded: TestData = read_toml_file(&path).unwrap();
        assert_eq!(loaded, data);
    }

    #[test]
    fn test_read_write_yaml_file() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("test.yaml");

        let data = TestData {
            name: "test".to_string(),
            value: 42,
        };

        write_yaml_file(&path, &data).unwrap();
        let loaded: TestData = read_yaml_file(&path).unwrap();
        assert_eq!(loaded, data);
    }

    #[test]
    fn test_create_temp_file() {
        let temp_file = create_temp_file("test", "content").unwrap();
        assert!(temp_file.exists());

        let content = read_text_file(&temp_file).unwrap();
        assert_eq!(content, "content");

        let path = temp_file.to_path_buf();
        drop(temp_file);
        assert!(!path.exists()); // Cleaned up after drop
    }

    #[test]
    fn test_read_nonexistent_file() {
        let result = read_text_file(Path::new("/nonexistent/file.txt"));
        assert!(result.is_err());
    }

    #[test]
    fn test_write_creates_parent_directories() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("nested").join("dirs").join("file.txt");

        write_text_file(&path, "content").unwrap();
        assert!(path.exists());
        assert_eq!(read_text_file(&path).unwrap(), "content");
    }

    #[test]
    fn test_json_parse_error() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("invalid.json");

        write_text_file(&path, "not valid json").unwrap();
        let result: Result<TestData> = read_json_file(&path);
        assert!(result.is_err());
    }

    #[test]
    fn test_toml_parse_error() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("invalid.toml");

        write_text_file(&path, "not = valid = toml").unwrap();
        let result: Result<TestData> = read_toml_file(&path);
        assert!(result.is_err());
    }

    #[test]
    fn test_yaml_parse_error() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("invalid.yaml");

        write_text_file(&path, "not: valid: yaml: [").unwrap();
        let result: Result<TestData> = read_yaml_file(&path);
        assert!(result.is_err());
    }
}
