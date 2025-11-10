//! Generic configuration parsing utilities.
//!
//! This module provides generic TOML parsing functionality that can be used
//! with any configuration structure that implements the appropriate serde traits.
//! It includes enhanced error reporting with file path context.
//!
//! # Features
//!
//! - **Generic Parsing**: Works with any `DeserializeOwned` type
//! - **Rich Error Context**: Includes file paths in error messages
//! - **TOML Focus**: Specifically designed for TOML configuration files
//! - **Path Safety**: Handles file system errors gracefully
//!
//! # Design Philosophy
//!
//! The parser is designed to be:
//! - **Simple**: Minimal API surface with clear semantics
//! - **Generic**: Reusable across different configuration types
//! - **Informative**: Provides clear error messages for debugging
//! - **Safe**: Handles file system and parsing errors appropriately
//!
//! # Usage Patterns
//!
//! ## Direct Parsing
//!
//! ```rust,no_run
//! use agpm_cli::config::parse_config;
//! use serde::Deserialize;
//! use std::path::Path;
//!
//! #[derive(Deserialize)]
//! struct MyConfig {
//!     name: String,
//!     version: String,
//! }
//!
//! # fn example() -> anyhow::Result<()> {
//! let config: MyConfig = parse_config(Path::new("config.toml"))?;
//! println!("Config: {} v{}", config.name, config.version);
//! # Ok(())
//! # }
//! ```
//!
//! # Error Handling
//!
//! The parser provides detailed error messages that include:
//!
//! - **File Context**: Which file failed to parse
//! - **Operation Context**: Whether it was a read or parse failure
//! - **Underlying Error**: The specific I/O or TOML parsing error
//!
//! Example error output:
//! ```text
//! Failed to parse config file: /path/to/config.toml
//! Caused by:
//!     invalid TOML value, expected string
//! ```
//!
//! # Integration
//!
//! This parser is used throughout AGPM for:
//!
//! - Generic configuration file parsing
//! - Test fixtures and development tools

use anyhow::{Context, Result};
use std::path::Path;

/// Parse a TOML configuration file into the specified type.
///
/// Generic function that reads a TOML file and deserializes it into any type
/// that implements [`serde::de::DeserializeOwned`]. Provides enhanced error
/// messages that include the file path context.
///
/// # Type Parameters
///
/// - `T`: The target type that implements `DeserializeOwned`
///
/// # Parameters
///
/// - `path`: Path to the TOML configuration file to parse
///
/// # Returns
///
/// The parsed configuration object of type `T`.
///
/// # Examples
///
/// ## Basic Usage
///
/// ```rust,no_run
/// use agpm_cli::config::parse_config;
/// use serde::Deserialize;
/// use std::path::Path;
///
/// #[derive(Deserialize)]
/// struct Config {
///     name: String,
///     port: u16,
/// }
///
/// # fn example() -> anyhow::Result<()> {
/// let config: Config = parse_config(Path::new("server.toml"))?;
/// println!("Starting {} on port {}", config.name, config.port);
/// # Ok(())
/// # }
/// ```
///
/// ## Error Handling
///
/// ```rust,no_run
/// use agpm_cli::config::parse_config;
/// use serde::Deserialize;
/// use std::path::Path;
///
/// #[derive(Deserialize)]
/// struct Config { name: String }
///
/// # fn example() {
/// match parse_config::<Config>(Path::new("missing.toml")) {
///     Ok(config) => println!("Config loaded: {}", config.name),
///     Err(e) => eprintln!("Failed to load config: {}", e),
/// }
/// # }
/// ```
///
/// # Error Conditions
///
/// This function returns an error if:
///
/// ## File System Errors
/// - File does not exist
/// - Insufficient permissions to read the file
/// - I/O errors during file reading
/// - Path is a directory, not a file
///
/// ## Parsing Errors
/// - File contains invalid TOML syntax
/// - TOML structure doesn't match the target type `T`
/// - Required fields are missing
/// - Field types don't match expectations
/// - TOML contains unsupported features for the target type
///
/// # Error Messages
///
/// The function provides two levels of error context:
///
/// 1. **File Operation Context**: "Failed to read config file: /path/to/file.toml"
/// 2. **Parsing Context**: "Failed to parse config file: /path/to/file.toml"
///
/// The underlying error (file system or TOML parsing) is preserved as the cause.
///
/// # Performance
///
/// - Reads the entire file into memory before parsing
/// - TOML parsing is generally fast for typical configuration file sizes
/// - No caching - each call performs a fresh read and parse
///
/// # Thread Safety
///
/// This function is thread-safe and can be called concurrently from multiple threads.
/// Each call operates independently on the file system.
pub fn parse_config<T>(path: &Path) -> Result<T>
where
    T: serde::de::DeserializeOwned,
{
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read config file: {}", path.display()))?;

    let config: T = toml::from_str(&content)
        .with_context(|| format!("Failed to parse config file: {}", path.display()))?;

    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_config() {
        use tempfile::tempdir;

        let temp = tempdir().unwrap();
        let config_path = temp.path().join("test.toml");

        #[derive(serde::Deserialize)]
        struct TestConfig {
            name: String,
            value: i32,
        }

        let toml_content = r#"
            name = "test"
            value = 42
        "#;

        std::fs::write(&config_path, toml_content).unwrap();

        let config: TestConfig = parse_config(&config_path).unwrap();
        assert_eq!(config.name, "test");
        assert_eq!(config.value, 42);
    }

    #[test]
    fn test_parse_config_error() {
        use tempfile::tempdir;

        let temp = tempdir().unwrap();
        let config_path = temp.path().join("invalid.toml");

        #[derive(serde::Deserialize)]
        struct TestConfig {
            #[allow(dead_code)] // Field used by serde for deserialization validation, not accessed directly
            name: String,
        }

        let invalid_toml = "invalid = toml {";
        std::fs::write(&config_path, invalid_toml).unwrap();

        let result: Result<TestConfig> = parse_config(&config_path);
        assert!(result.is_err());
    }
}
