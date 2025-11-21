//! Integration-style tests for resource service helpers (previously under tests/unit).

use anyhow::Result;
use std::path::Path;
use tempfile::TempDir;

use agpm_cli::resolver::resource_service::ResourceFetchingService;

#[test]
fn canonicalize_with_context_success() -> Result<(), Box<dyn std::error::Error>> {
    let temp_dir = TempDir::new()?;
    let test_file = temp_dir.path().join("test.txt");
    std::fs::write(&test_file, "test content")?;

    let result = ResourceFetchingService::canonicalize_with_context(
        &test_file,
        "test operation".to_string(),
        "test_function",
    );

    assert!(result.is_ok());
    let canonical_path = result?;
    assert!(canonical_path.is_absolute());
    assert!(canonical_path.exists());
    Ok(())
}

#[test]
fn canonicalize_with_context_error() -> Result<(), Box<dyn std::error::Error>> {
    let non_existent_path = Path::new("/definitely/does/not/exist/file.txt");

    let result = ResourceFetchingService::canonicalize_with_context(
        non_existent_path,
        "test operation".to_string(),
        "test_function",
    );

    assert!(result.is_err());
    let error = result.unwrap_err();

    // Verify structured file error context is present
    let file_error = error
        .downcast_ref::<agpm_cli::core::file_error::FileOperationError>()
        .expect("Expected FileOperationError");
    assert_eq!(file_error.operation, agpm_cli::core::file_error::FileOperation::Canonicalize);
    assert_eq!(file_error.caller, "test_function");
    assert_eq!(file_error.purpose, "test operation");
    assert_eq!(file_error.file_path, non_existent_path);
    Ok(())
}

#[test]
fn canonicalize_with_context_relative_path() -> Result<(), Box<dyn std::error::Error>> {
    let temp_dir = TempDir::new()?;
    std::fs::create_dir_all(temp_dir.path().join("subdir"))?;
    let test_file = temp_dir.path().join("subdir/test.txt");
    std::fs::write(&test_file, "test content")?;

    // Test with absolute path (no need to change global working directory)
    let absolute_path = temp_dir.path().join("subdir/test.txt");

    let result = ResourceFetchingService::canonicalize_with_context(
        &absolute_path,
        "resolving relative path".to_string(),
        "test_relative",
    );

    assert!(result.is_ok());
    let canonical_path = result?;
    assert!(canonical_path.is_absolute());
    assert!(canonical_path.exists());
    Ok(())
}
