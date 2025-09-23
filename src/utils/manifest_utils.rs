//! Utilities for working with manifest files
//!
//! This module provides common functionality for loading and validating
//! manifest files across different commands.

use crate::core::error::ErrorContext;
use crate::manifest::Manifest;
use anyhow::{Context, Result, anyhow};
use std::path::Path;

/// Load a project manifest from the standard location
///
/// This function looks for a `ccpm.toml` file in the given project directory
/// and returns a parsed Manifest. It provides consistent error messages
/// across all commands.
///
/// # Arguments
///
/// * `project_dir` - The project directory to search for ccpm.toml
///
/// # Returns
///
/// * `Ok(Manifest)` - The parsed manifest
/// * `Err` - If the manifest doesn't exist or can't be parsed
///
/// # Example
///
/// ```no_run
/// # use anyhow::Result;
/// # fn example() -> Result<()> {
/// use std::path::Path;
/// use ccpm::utils::manifest_utils::load_project_manifest;
///
/// let manifest = load_project_manifest(Path::new("."))?;
/// # Ok(())
/// # }
/// ```
pub fn load_project_manifest(project_dir: &Path) -> Result<Manifest> {
    let manifest_path = project_dir.join("ccpm.toml");

    if !manifest_path.exists() {
        return Err(
            anyhow!("No ccpm.toml found in {}", project_dir.display()).context(ErrorContext {
                error: crate::core::CcpmError::ManifestNotFound,
                suggestion: Some("Run 'ccpm init' to create a new project".to_string()),
                details: Some(format!("Expected manifest at: {}", manifest_path.display())),
            }),
        );
    }

    Manifest::load(&manifest_path).with_context(|| ErrorContext {
        error: crate::core::CcpmError::ManifestParseError {
            file: manifest_path.display().to_string(),
            reason: "Failed to parse manifest".to_string(),
        },
        suggestion: Some("Check that ccpm.toml is valid TOML syntax".to_string()),
        details: Some(format!("Manifest path: {}", manifest_path.display())),
    })
}

/// Load a manifest from a specific path with validation
///
/// This function loads a manifest from any path and optionally validates
/// that it contains required sections.
///
/// # Arguments
///
/// * `manifest_path` - Path to the manifest file
/// * `require_sources` - Whether to require at least one source
/// * `require_dependencies` - Whether to require at least one dependency
///
/// # Returns
///
/// * `Ok(Manifest)` - The parsed and validated manifest
/// * `Err` - If the manifest can't be loaded or validation fails
pub fn load_and_validate_manifest(
    manifest_path: &Path,
    require_sources: bool,
    require_dependencies: bool,
) -> Result<Manifest> {
    if !manifest_path.exists() {
        return Err(anyhow!(
            "Manifest file not found: {}",
            manifest_path.display()
        ));
    }

    let manifest = Manifest::load(manifest_path)?;

    if require_sources && manifest.sources.is_empty() {
        return Err(
            anyhow!("No sources defined in manifest").context(ErrorContext {
                error: crate::core::CcpmError::ManifestValidationError {
                    reason: "No sources defined in manifest".to_string(),
                },
                suggestion: Some("Add at least one source using 'ccpm add source'".to_string()),
                details: None,
            }),
        );
    }

    if require_dependencies
        && (manifest.agents.is_empty()
            && manifest.snippets.is_empty()
            && manifest.commands.is_empty()
            && manifest.mcp_servers.is_empty())
    {
        return Err(
            anyhow!("No dependencies defined in manifest").context(ErrorContext {
                error: crate::core::CcpmError::ManifestValidationError {
                    reason: "No dependencies defined in manifest".to_string(),
                },
                suggestion: Some("Add dependencies using 'ccpm add dep'".to_string()),
                details: None,
            }),
        );
    }

    Ok(manifest)
}

/// Check if a manifest exists in the project directory
///
/// # Arguments
///
/// * `project_dir` - The project directory to check
///
/// # Returns
///
/// * `true` if ccpm.toml exists, `false` otherwise
pub fn manifest_exists(project_dir: &Path) -> bool {
    project_dir.join("ccpm.toml").exists()
}

/// Get the path to the manifest file
///
/// # Arguments
///
/// * `project_dir` - The project directory
///
/// # Returns
///
/// The path to ccpm.toml in the project directory
pub fn manifest_path(project_dir: &Path) -> std::path::PathBuf {
    project_dir.join("ccpm.toml")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_load_project_manifest_missing() {
        let temp_dir = tempdir().unwrap();
        let result = load_project_manifest(temp_dir.path());
        assert!(result.is_err());
        // The error will contain both the initial message and the context
        let err = result.unwrap_err();
        let err_str = err.to_string();
        assert!(err_str.contains("ccpm.toml") || err_str.contains("Manifest"));
    }

    #[test]
    fn test_load_project_manifest_invalid() {
        let temp_dir = tempdir().unwrap();
        let manifest_path = temp_dir.path().join("ccpm.toml");
        fs::write(&manifest_path, "invalid toml {").unwrap();

        let result = load_project_manifest(temp_dir.path());
        assert!(result.is_err());
        // Just verify it's an error - the specific message format may vary
    }

    #[test]
    fn test_manifest_exists() {
        let temp_dir = tempdir().unwrap();
        assert!(!manifest_exists(temp_dir.path()));

        let manifest_path = temp_dir.path().join("ccpm.toml");
        fs::write(&manifest_path, "[sources]").unwrap();
        assert!(manifest_exists(temp_dir.path()));
    }

    #[test]
    fn test_manifest_path() {
        let temp_dir = tempdir().unwrap();
        let path = manifest_path(temp_dir.path());
        assert_eq!(path, temp_dir.path().join("ccpm.toml"));
    }

    #[test]
    fn test_load_and_validate_manifest_success() {
        let temp_dir = tempdir().unwrap();
        let manifest_path = temp_dir.path().join("ccpm.toml");

        // Create valid manifest with sources and dependencies
        let content = r#"
[sources]
test = "https://github.com/test/repo.git"

[agents]
test-agent = { source = "test", path = "agent.md", version = "v1.0.0" }
"#;
        fs::write(&manifest_path, content).unwrap();

        // Should succeed with both validations
        let result = load_and_validate_manifest(&manifest_path, true, true);
        assert!(result.is_ok());

        let manifest = result.unwrap();
        assert!(!manifest.sources.is_empty());
        assert!(!manifest.agents.is_empty());
    }

    #[test]
    fn test_load_and_validate_manifest_no_sources() {
        let temp_dir = tempdir().unwrap();
        let manifest_path = temp_dir.path().join("ccpm.toml");

        // Create manifest without sources
        let content = r#"
[agents]
test-agent = { path = "../local/agent.md" }
"#;
        fs::write(&manifest_path, content).unwrap();

        // Should fail when requiring sources
        let result = load_and_validate_manifest(&manifest_path, true, false);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("No sources"));

        // Should succeed when not requiring sources
        let result = load_and_validate_manifest(&manifest_path, false, false);
        assert!(result.is_ok());
    }

    #[test]
    fn test_load_and_validate_manifest_no_dependencies() {
        let temp_dir = tempdir().unwrap();
        let manifest_path = temp_dir.path().join("ccpm.toml");

        // Create manifest with only sources
        let content = r#"
[sources]
test = "https://github.com/test/repo.git"
"#;
        fs::write(&manifest_path, content).unwrap();

        // Should fail when requiring dependencies
        let result = load_and_validate_manifest(&manifest_path, false, true);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("No dependencies"));

        // Should succeed when not requiring dependencies
        let result = load_and_validate_manifest(&manifest_path, false, false);
        assert!(result.is_ok());
    }

    #[test]
    fn test_load_and_validate_manifest_nonexistent() {
        let temp_dir = tempdir().unwrap();
        let manifest_path = temp_dir.path().join("nonexistent.toml");

        let result = load_and_validate_manifest(&manifest_path, false, false);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[test]
    fn test_load_and_validate_manifest_with_snippets() {
        let temp_dir = tempdir().unwrap();
        let manifest_path = temp_dir.path().join("ccpm.toml");

        // Create manifest with snippets dependency
        let content = r#"
[sources]
test = "https://github.com/test/repo.git"

[snippets]
test-snippet = { source = "test", path = "snippet.md", version = "v1.0.0" }
"#;
        fs::write(&manifest_path, content).unwrap();

        // Should succeed when requiring dependencies (has snippets)
        let result = load_and_validate_manifest(&manifest_path, true, true);
        assert!(result.is_ok());
    }

    #[test]
    fn test_load_and_validate_manifest_with_commands() {
        let temp_dir = tempdir().unwrap();
        let manifest_path = temp_dir.path().join("ccpm.toml");

        // Create manifest with commands dependency
        let content = r#"
[sources]
test = "https://github.com/test/repo.git"

[commands]
test-command = { source = "test", path = "command.md", version = "v1.0.0" }
"#;
        fs::write(&manifest_path, content).unwrap();

        // Should succeed when requiring dependencies (has commands)
        let result = load_and_validate_manifest(&manifest_path, true, true);
        assert!(result.is_ok());
    }

    #[test]
    fn test_load_and_validate_manifest_with_mcp_servers() {
        let temp_dir = tempdir().unwrap();
        let manifest_path = temp_dir.path().join("ccpm.toml");

        // Create manifest with MCP servers dependency
        let content = r#"
[sources]
test = "https://github.com/test/repo.git"

[mcp-servers]
test-server = "../local/mcp-servers/test-server.json"
"#;
        fs::write(&manifest_path, content).unwrap();

        // Should succeed when requiring dependencies (has MCP servers)
        let result = load_and_validate_manifest(&manifest_path, true, true);
        assert!(result.is_ok());
    }

    #[test]
    fn test_load_project_manifest_valid() {
        let temp_dir = tempdir().unwrap();
        let manifest_path = temp_dir.path().join("ccpm.toml");

        // Create a valid manifest
        let content = r#"
[sources]
test = "https://github.com/test/repo.git"

[agents]
test-agent = { source = "test", path = "agent.md", version = "v1.0.0" }
"#;
        fs::write(&manifest_path, content).unwrap();

        let result = load_project_manifest(temp_dir.path());
        assert!(result.is_ok());

        let manifest = result.unwrap();
        assert_eq!(manifest.sources.len(), 1);
        assert_eq!(manifest.agents.len(), 1);
    }

    #[test]
    fn test_load_and_validate_empty_manifest() {
        let temp_dir = tempdir().unwrap();
        let manifest_path = temp_dir.path().join("ccpm.toml");

        // Create an empty but valid manifest
        let content = "";
        fs::write(&manifest_path, content).unwrap();

        // Should succeed when not requiring anything
        let result = load_and_validate_manifest(&manifest_path, false, false);
        assert!(result.is_ok());

        // Should fail when requiring sources
        let result = load_and_validate_manifest(&manifest_path, true, false);
        assert!(result.is_err());

        // Should fail when requiring dependencies
        let result = load_and_validate_manifest(&manifest_path, false, true);
        assert!(result.is_err());
    }

    #[test]
    fn test_manifest_validation_mixed_dependencies() {
        let temp_dir = tempdir().unwrap();
        let manifest_path = temp_dir.path().join("ccpm.toml");

        // Create manifest with multiple types of dependencies
        let content = r#"
[sources]
source1 = "https://github.com/test/repo1.git"
source2 = "https://github.com/test/repo2.git"

[agents]
agent1 = { source = "source1", path = "agent1.md", version = "v1.0.0" }

[snippets]
snippet1 = { source = "source2", path = "snippet1.md", version = "v2.0.0" }

[commands]
cmd1 = { source = "source1", path = "cmd1.md", version = "v1.0.0" }
"#;
        fs::write(&manifest_path, content).unwrap();

        let result = load_and_validate_manifest(&manifest_path, true, true);
        assert!(result.is_ok());

        let manifest = result.unwrap();
        assert_eq!(manifest.sources.len(), 2);
        assert_eq!(manifest.agents.len(), 1);
        assert_eq!(manifest.snippets.len(), 1);
        assert_eq!(manifest.commands.len(), 1);
    }

    #[test]
    fn test_error_context_in_load_project_manifest() {
        let temp_dir = tempdir().unwrap();

        // Test missing manifest error
        let result = load_project_manifest(temp_dir.path());
        assert!(result.is_err());

        let err_chain = result.unwrap_err();
        let err_str = format!("{:?}", err_chain);

        // Should contain error context with suggestion
        assert!(err_str.contains("ccpm.toml") || err_str.contains("init"));
    }

    #[test]
    fn test_error_context_in_validation() {
        let temp_dir = tempdir().unwrap();
        let manifest_path = temp_dir.path().join("ccpm.toml");

        // Create manifest without sources
        fs::write(&manifest_path, "").unwrap();

        // Test no sources error
        let result = load_and_validate_manifest(&manifest_path, true, false);
        assert!(result.is_err());

        let err_chain = result.unwrap_err();
        let err_str = format!("{:?}", err_chain);
        assert!(err_str.contains("source") || err_str.contains("No sources"));

        // Test no dependencies error
        let result = load_and_validate_manifest(&manifest_path, false, true);
        assert!(result.is_err());

        let err_chain = result.unwrap_err();
        let err_str = format!("{:?}", err_chain);
        assert!(err_str.contains("dependencies") || err_str.contains("No dependencies"));
    }
}
