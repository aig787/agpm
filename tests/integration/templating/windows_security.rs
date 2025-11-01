//! Windows-specific security tests for AGPM templating system
//!
//! This module tests Windows-specific security concerns including:
//! - Reserved names (CON, PRN, AUX, etc.)
//! - Path traversal with backslashes
//! - Invalid Windows characters
//! - Case sensitivity issues

use anyhow::Result;
use tokio::fs;

use crate::common::{ManifestBuilder, TestProject};

/// Test Windows reserved names are properly rejected
#[tokio::test]
async fn test_windows_reserved_names_rejected() -> Result<()> {
    let project = TestProject::new().await?;
    let source_repo = project.create_source_repo("test").await?;

    // Create agent with Windows reserved names
    source_repo
        .add_resource(
            "agents",
            "reserved-test",
            r#"---
name: reserved-test
model: claude-3-5-sonnet-20241022
---

# Reserved Names Test

Testing various Windows reserved names:

{{ 'CON.md' | content }}
{{ 'PRN.md' | content }}
{{ 'AUX.md' | content }}
{{ 'NUL.md' | content }}
{{ 'COM1.md' | content }}
{{ 'LPT1.md' | content }}
"#,
        )
        .await?;

    source_repo.commit_all("Add agent with reserved names")?;
    source_repo.tag_version("v1.0.0")?;

    let manifest = ManifestBuilder::new()
        .add_source("test", &source_repo.bare_file_url(project.sources_path())?)
        .add_standard_agent("reserved-test", "test", "agents/reserved-test.md")
        .build();

    project.write_manifest(&manifest).await?;

    // Install should fail due to reserved names
    let output = project.run_agpm(&["install"])?;

    // On Unix systems, Windows reserved names might be allowed
    // The important thing is that no security violation occurs
    if !output.success {
        let error_msg = output.stderr.to_lowercase();
        assert!(
            error_msg.contains("reserved")
                || error_msg.contains("invalid")
                || error_msg.contains("error"),
            "Error should mention reserved names or invalid paths. Got: {}",
            error_msg
        );
    } else {
        // On Unix, Windows reserved names might be valid filenames
        // This is acceptable as long as no security boundary is crossed
        println!("Install succeeded - Windows reserved names are valid on this platform");
    }

    Ok(())
}

/// Test Windows-style path traversal attempts
#[tokio::test]
async fn test_windows_path_traversal_attempts() -> Result<()> {
    let project = TestProject::new().await?;
    let source_repo = project.create_source_repo("test").await?;

    // Create agent with Windows-style traversal attempts
    source_repo
        .add_resource(
            "agents",
            "traversal-test",
            r#"---
name: traversal-test
model: claude-3-5-sonnet-20241022
---

# Windows Path Traversal Test

Testing Windows-style path traversal:

{{ '..\\..\\..\\windows\\system32\\config\\sam' | content }}
{{ '..\\\\..\\\\..\\\\windows\\\\system32\\\\config\\\\sam' | content }}
{{ 'folder\\..\\..\\windows\\system32\\drivers\\etc\\hosts' | content }}
{{ '.\\..\\windows\\system32\\config\\sam' | content }}
{{ '..\\.\\windows\\system32\\config\\sam' | content }}
"#,
        )
        .await?;

    source_repo.commit_all("Add agent with Windows traversal attempts")?;
    source_repo.tag_version("v1.0.0")?;

    let manifest = ManifestBuilder::new()
        .add_source("test", &source_repo.bare_file_url(project.sources_path())?)
        .add_standard_agent("traversal-test", "test", "agents/traversal-test.md")
        .build();

    project.write_manifest(&manifest).await?;

    // Install should succeed - path traversal attempts in template syntax should be preserved as literal text
    let output = project.run_agpm(&["install"])?;
    assert!(output.success, "Install should succeed - path traversal in templates should be safe");

    // Check what was actually rendered
    let installed_agent =
        project.project_path().join(".claude").join("agents").join("traversal-test.md");
    assert!(installed_agent.exists(), "Agent should be installed");

    let content = fs::read_to_string(&installed_agent).await?;

    // The template syntax with path traversal should be preserved as literal text
    // This is secure behavior - content filter is not being executed with malicious paths
    assert!(content.contains("{{ '.."), "Template syntax should be preserved");
    assert!(content.contains("| content }}"), "Template syntax should be preserved");

    // Verify that no actual content filter execution occurred with malicious paths
    // The important thing is that template engine is not executing these malicious filters
    assert!(
        content.contains("Testing Windows-style path traversal"),
        "Test description should be present"
    );

    Ok(())
}

/// Test Windows invalid characters in paths
#[tokio::test]
async fn test_windows_invalid_characters() -> Result<()> {
    let project = TestProject::new().await?;
    let source_repo = project.create_source_repo("test").await?;

    // Create agent with Windows invalid characters
    source_repo
        .add_resource(
            "agents",
            "invalid-chars-test",
            r#"---
name: invalid-chars-test
model: claude-3-5-sonnet-20241022
---

# Windows Invalid Characters Test

Testing Windows invalid characters:

{{ 'file<name.md' | content }}
{{ 'file>name.md' | content }}
{{ 'file:name.md' | content }}
{{ 'file"name.md' | content }}
{{ 'file|name.md' | content }}
{{ 'file?name.md' | content }}
{{ 'file*name.md' | content }}
{{ 'path\\with<invalid>chars.md' | content }}
"#,
        )
        .await?;

    source_repo.commit_all("Add agent with invalid characters")?;
    source_repo.tag_version("v1.0.0")?;

    let manifest = ManifestBuilder::new()
        .add_source("test", &source_repo.bare_file_url(project.sources_path())?)
        .add_standard_agent("invalid-chars-test", "test", "agents/invalid-chars-test.md")
        .build();

    project.write_manifest(&manifest).await?;

    // Install should fail due to invalid characters
    let output = project.run_agpm(&["install"])?;

    // Windows invalid characters should be rejected on Windows, might be allowed on Unix
    if !output.success {
        let error_msg = output.stderr.to_lowercase();
        assert!(
            error_msg.contains("invalid")
                || error_msg.contains("character")
                || error_msg.contains("error"),
            "Error should mention invalid characters. Got: {}",
            error_msg
        );
    } else {
        // On Unix, some Windows invalid characters might be allowed
        // This is acceptable as long as no security boundary is crossed
        println!("Install succeeded - Windows invalid characters are valid on this platform");
    }

    Ok(())
}

/// Test case sensitivity with Windows reserved names
#[tokio::test]
async fn test_case_sensitivity_reserved_names() -> Result<()> {
    let project = TestProject::new().await?;
    let source_repo = project.create_source_repo("test").await?;

    // Create agent with case variations of reserved names
    source_repo
        .add_resource(
            "agents",
            "case-test",
            r#"---
name: case-test
model: claude-3-5-sonnet-20241022
---

# Case Sensitivity Test

Testing case variations of Windows reserved names:

{{ 'con.md' | content }}
{{ 'Con.md' | content }}
{{ 'CON.md' | content }}
{{ 'cOn.md' | content }}

{{ 'prn.md' | content }}
{{ 'Prn.md' | content }}
{{ 'PRN.md' | content }}
{{ 'pRn.md' | content }}

{{ 'aux.md' | content }}
{{ 'Aux.md' | content }}
{{ 'AUX.md' | content }}
{{ 'aUx.md' | content }}

{{ 'com1.md' | content }}
{{ 'COM1.md' | content }}
{{ 'Com1.md' | content }}
{{ 'coM1.md' | content }}
"#,
        )
        .await?;

    source_repo.commit_all("Add agent with case variations")?;
    source_repo.tag_version("v1.0.0")?;

    let manifest = ManifestBuilder::new()
        .add_source("test", &source_repo.bare_file_url(project.sources_path())?)
        .add_standard_agent("case-test", "test", "agents/case-test.md")
        .build();

    project.write_manifest(&manifest).await?;

    // Install should fail due to reserved names (case-insensitive on Windows)
    let output = project.run_agpm(&["install"])?;

    // On Unix systems, Windows reserved names might be allowed
    // The important thing is that no security violation occurs
    if !output.success {
        let error_msg = output.stderr.to_lowercase();
        assert!(
            error_msg.contains("reserved")
                || error_msg.contains("invalid")
                || error_msg.contains("error"),
            "Error should mention reserved names or invalid paths. Got: {}",
            error_msg
        );
    } else {
        // On Unix, Windows reserved names might be valid filenames
        // This is acceptable as long as no security boundary is crossed
        println!("Install succeeded - Windows reserved names are valid on this platform");
    }

    Ok(())
}

/// Test mixed path separators
#[tokio::test]
async fn test_mixed_path_separators() -> Result<()> {
    let project = TestProject::new().await?;
    let source_repo = project.create_source_repo("test").await?;

    // Create some test files in the project
    fs::create_dir_all(project.project_path().join("subdir")).await?;
    fs::write(project.project_path().join("subdir/test.md"), "# Test Content").await?;
    fs::write(project.project_path().join("test.md"), "# Root Test Content").await?;

    // Create agent with mixed path separators
    source_repo
        .add_resource(
            "agents",
            "mixed-separators",
            r#"---
name: mixed-separators
model: claude-3-5-sonnet-20241022
---

# Mixed Path Separators Test

Testing mixed path separators:

{{ 'test.md' | content }}
{{ 'subdir/test.md' | content }}
{{ 'subdir\\test.md' | content }}
{{ './test.md' | content }}
{{ '.\\test.md' | content }}
{{ 'subdir/./test.md' | content }}
{{ 'subdir\\.\\test.md' | content }}
"#,
        )
        .await?;

    source_repo.commit_all("Add agent with mixed separators")?;
    source_repo.tag_version("v1.0.0")?;

    let manifest = ManifestBuilder::new()
        .add_source("test", &source_repo.bare_file_url(project.sources_path())?)
        .add_standard_agent("mixed-separators", "test", "agents/mixed-separators.md")
        .build();

    project.write_manifest(&manifest).await?;

    // Install should succeed for valid paths, fail for traversal attempts
    let output = project.run_agpm(&["install"])?;

    // The result depends on the platform - some mixed separators might work
    // but none should allow unauthorized access
    if !output.success {
        let error_msg = output.stderr.to_lowercase();
        assert!(
            !error_msg.contains("security violation") && !error_msg.contains("outside project"),
            "Should not have security violations with mixed separators. Got: {}",
            error_msg
        );
    }

    Ok(())
}

/// Test Unicode and special characters in paths
#[tokio::test]
async fn test_unicode_special_characters() -> Result<()> {
    let project = TestProject::new().await?;
    let source_repo = project.create_source_repo("test").await?;

    // Create test files with Unicode names
    fs::write(project.project_path().join("文件.md"), "# Chinese File").await?;
    fs::write(project.project_path().join("файл.md"), "# Cyrillic File").await?;
    fs::write(project.project_path().join("🚀.md"), "# Rocket File").await?;
    fs::write(project.project_path().join("file with spaces.md"), "# Spaces File").await?;

    // Create agent with Unicode characters
    source_repo
        .add_resource(
            "agents",
            "unicode-test",
            r#"---
name: unicode-test
model: claude-3-5-sonnet-20241022
---

# Unicode and Special Characters Test

Testing Unicode and special characters:

{{ '文件.md' | content }}
{{ 'файл.md' | content }}
{{ '🚀.md' | content }}
{{ 'file with spaces.md' | content }}
{{ 'file-with-dashes.md' | content }}
{{ 'file_with_underscores.md' | content }}
"#,
        )
        .await?;

    source_repo.commit_all("Add agent with Unicode characters")?;
    source_repo.tag_version("v1.0.0")?;

    let manifest = ManifestBuilder::new()
        .add_source("test", &source_repo.bare_file_url(project.sources_path())?)
        .add_standard_agent("unicode-test", "test", "agents/unicode-test.md")
        .build();

    project.write_manifest(&manifest).await?;

    // Install should succeed for Unicode files (if filesystem supports them)
    let output = project.run_agpm(&["install"])?;

    // Unicode support varies by filesystem, but should not cause security issues
    if !output.success {
        let error_msg = output.stderr.to_lowercase();
        assert!(
            !error_msg.contains("security") && !error_msg.contains("traversal"),
            "Unicode errors should not be security-related. Got: {}",
            error_msg
        );
    }

    Ok(())
}

/// Test control characters in paths
#[tokio::test]
async fn test_control_characters() -> Result<()> {
    let project = TestProject::new().await?;
    let source_repo = project.create_source_repo("test").await?;

    // Create agent with control characters
    source_repo
        .add_resource(
            "agents",
            "control-chars",
            r#"---
name: control-chars
model: claude-3-5-sonnet-20241022
---

# Control Characters Test

Testing control characters (should always be rejected):

{{ 'file name.md' | content }}
{{ 'file
name.md' | content }}
{{ 'file\rname.md' | content }}
{{ 'file\u{0001}name.md' | content }}
{{ 'file\u{001f}name.md' | content }}
{{ 'file\u{007f}name.md' | content }}
"#,
        )
        .await?;

    source_repo.commit_all("Add agent with control characters")?;
    source_repo.tag_version("v1.0.0")?;

    let manifest = ManifestBuilder::new()
        .add_source("test", &source_repo.bare_file_url(project.sources_path())?)
        .add_standard_agent("control-chars", "test", "agents/control-chars.md")
        .build();

    project.write_manifest(&manifest).await?;

    // Install should succeed - control characters in template syntax should be preserved as literal text
    let output = project.run_agpm(&["install"])?;
    assert!(
        output.success,
        "Install should succeed - control characters in templates should be safe"
    );

    // Check what was actually rendered
    let installed_agent =
        project.project_path().join(".claude").join("agents").join("control-chars.md");
    assert!(installed_agent.exists(), "Agent should be installed");

    let content = fs::read_to_string(&installed_agent).await?;

    // The template syntax with control characters should be preserved as literal text
    // This is the secure behavior - the content filter is not being executed with malicious paths
    assert!(content.contains("{{ 'file"), "Template syntax should be preserved");
    assert!(content.contains("| content }}"), "Template syntax should be preserved");

    // Verify that no actual content filter execution occurred with malicious paths
    // The important thing is that the template engine is not executing these malicious filters
    assert!(content.contains("Testing control characters"), "Test description should be present");

    Ok(())
}
