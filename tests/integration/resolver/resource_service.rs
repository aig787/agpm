//! Integration-style tests for resource service helpers (previously under tests/unit).

use anyhow::Result;

use agpm_cli::resolver::resource_service::ResourceFetchingService;
use crate::common::TestProject;

#[tokio::test]
async fn canonicalize_with_context_success() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;

    // Create test file
    let test_content = "test content";
    let test_file = project.project_path().join("test.txt");
    tokio::fs::write(&test_file, test_content).await?;

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

#[tokio::test]
async fn canonicalize_with_context_error() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;
    let non_existent_path = project.project_path().join("definitely").join("does").join("not").join("exist").join("file.txt");

    let result = ResourceFetchingService::canonicalize_with_context(
        &non_existent_path,
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

#[tokio::test]
async fn canonicalize_with_context_relative_path() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;

    // Create subdirectory and test file
    let subdir = project.project_path().join("subdir");
    tokio::fs::create_dir_all(&subdir).await?;
    let test_file = subdir.join("test.txt");
    tokio::fs::write(&test_file, "test content").await?;

    // Test with absolute path (no need to change global working directory)
    let absolute_path = &test_file;

    let result = ResourceFetchingService::canonicalize_with_context(
        absolute_path,
        "resolving relative path".to_string(),
        "test_relative",
    );

    assert!(result.is_ok());
    let canonical_path = result?;
    assert!(canonical_path.is_absolute());
    assert!(canonical_path.exists());
    Ok(())
}
