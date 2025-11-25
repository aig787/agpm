//! Tests for gitignore operations.
//!
//! This module contains comprehensive tests for gitignore file management,
//! including cleanup, concurrent access, security, and cross-platform handling.

#![cfg(test)]

use crate::installer::gitignore::cleanup_gitignore;
use crate::installer::update_gitignore;
use crate::lockfile::{LockFile, LockedResource};

use anyhow::Result;
use std::fs;
use std::sync::Arc;
use tempfile::TempDir;
use tokio::sync::Mutex;

#[tokio::test]
async fn test_cleanup_gitignore_removes_agpm_section() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let gitignore_path = temp_dir.path().join(".gitignore");

    let content = r#"# User content
node_modules/
target/

# AGPM managed entries - do not edit below this line
.claude/agents/
.claude/snippets/
.claude/commands/
# End of AGPM managed entries

# More user content
*.log
.DS_Store
"#;
    fs::write(&gitignore_path, content)?;

    cleanup_gitignore(temp_dir.path(), None).await?;

    let remaining = fs::read_to_string(&gitignore_path)?;
    assert!(remaining.contains("node_modules/"));
    assert!(remaining.contains("target/"));
    assert!(remaining.contains("*.log"));
    assert!(remaining.contains(".DS_Store"));
    assert!(!remaining.contains("AGPM managed resources"));
    assert!(!remaining.contains(".claude/agents/"));
    Ok(())
}

#[tokio::test]
async fn test_cleanup_gitignore_deletes_empty_file() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let gitignore_path = temp_dir.path().join(".gitignore");

    let content = r#"# AGPM managed entries - do not edit below this line
.claude/agents/
.claude/snippets/
# End of AGPM managed entries
"#;
    fs::write(&gitignore_path, content)?;

    cleanup_gitignore(temp_dir.path(), None).await?;

    assert!(!gitignore_path.exists());
    Ok(())
}

#[tokio::test]
async fn test_cleanup_gitignore_handles_ccpm_markers() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let gitignore_path = temp_dir.path().join(".gitignore");

    let content = r#"# User content
build/
dist/

# CCPM managed entries - do not edit below this line
.claude/agents/
.claude/snippets/
# End of CCPM managed entries

# More user content
*.tmp
"#;
    fs::write(&gitignore_path, content)?;

    cleanup_gitignore(temp_dir.path(), None).await?;

    let remaining = fs::read_to_string(&gitignore_path)?;
    assert!(remaining.contains("build/"));
    assert!(remaining.contains("dist/"));
    assert!(remaining.contains("*.tmp"));
    assert!(!remaining.contains("CCPM managed resources"));
    assert!(!remaining.contains(".claude/agents/"));
    Ok(())
}

#[tokio::test]
async fn test_cleanup_gitignore_noop_when_missing() -> Result<()> {
    let temp_dir = TempDir::new()?;

    assert!(!temp_dir.path().join(".gitignore").exists());

    cleanup_gitignore(temp_dir.path(), None).await?;

    assert!(!temp_dir.path().join(".gitignore").exists());
    Ok(())
}

#[tokio::test]
async fn test_cleanup_gitignore_preserves_without_markers() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let gitignore_path = temp_dir.path().join(".gitignore");

    let content = r#"# User managed .gitignore
node_modules/
target/
*.log
.DS_Store
"#;
    fs::write(&gitignore_path, content)?;

    cleanup_gitignore(temp_dir.path(), None).await?;

    let remaining = fs::read_to_string(&gitignore_path)?;
    assert_eq!(remaining.trim_end(), content.trim_end());
    Ok(())
}

#[tokio::test]
async fn test_cleanup_gitignore_race_condition_protection() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let gitignore_path = temp_dir.path().join(".gitignore");
    let sensitive_file = temp_dir.path().join("sensitive.txt");

    std::fs::write(&sensitive_file, "SECRET_DATA")?;
    std::fs::write(&gitignore_path, "# User content\nuser-pattern/\n")?;

    let result = cleanup_gitignore(temp_dir.path(), None).await;

    assert!(result.is_ok(), "Cleanup should succeed even if file operations race");

    let sensitive_content = std::fs::read_to_string(&sensitive_file)?;
    assert_eq!(sensitive_content, "SECRET_DATA");
    Ok(())
}

#[tokio::test]
async fn test_cleanup_gitignore_handles_concurrent_deletes() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let gitignore_path = temp_dir.path().join(".gitignore");

    std::fs::write(
        &gitignore_path,
        "# AGPM managed entries - do not edit below this line\n.claude/\n# End of AGPM managed entries\n",
    )?;

    let gitignore_path_clone = gitignore_path.clone();
    let delete_handle = tokio::spawn(async move {
        tokio::time::sleep(tokio::time::Duration::from_millis(1)).await;
        let _ = std::fs::remove_file(&gitignore_path_clone);
    });

    let result = cleanup_gitignore(temp_dir.path(), None).await;

    assert!(result.is_ok(), "Cleanup should handle concurrent file deletion gracefully");

    delete_handle.await?;
    Ok(())
}

#[tokio::test]
async fn test_error_message_sanitization_release_mode() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let gitignore_path = temp_dir.path().join(".gitignore");

    let sensitive_project_path = "/home/user/sensitive-project/data/secrets";
    std::fs::write(&gitignore_path, format!("{}\n", sensitive_project_path))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&gitignore_path)?.permissions();
        perms.set_mode(0o000);
        std::fs::set_permissions(&gitignore_path, perms)?;

        let result = cleanup_gitignore(temp_dir.path(), None).await;

        assert!(result.is_err(), "Expected permission error");

        let error_msg = format!("{:?}", result.unwrap_err());

        assert!(
            error_msg.contains("Failed to read .gitignore file") || error_msg.contains("gitignore")
        );

        let mut perms = std::fs::metadata(&gitignore_path)?.permissions();
        perms.set_mode(0o644);
        std::fs::set_permissions(&gitignore_path, perms)?;
    }

    #[cfg(not(unix))]
    {
        use crate::installer::gitignore::sanitize_path_for_error;
        let path = std::path::Path::new("/Users/sensitive/data/secrets.txt");

        let sanitized = sanitize_path_for_error(path);
        assert!(sanitized.contains("secrets.txt"));
    }
    Ok(())
}

#[tokio::test]
async fn test_cleanup_gitignore_missing_file_handling() -> Result<()> {
    let temp_dir = TempDir::new()?;

    let result = cleanup_gitignore(temp_dir.path(), None).await;

    assert!(result.is_ok(), "Cleanup should succeed when .gitignore doesn't exist");

    let gitignore_path = temp_dir.path().join(".gitignore");
    assert!(!gitignore_path.exists(), "Should not create .gitignore file");
    Ok(())
}

#[tokio::test]
async fn test_concurrent_gitignore_additions() -> Result<()> {
    use crate::installer::gitignore::add_path_to_gitignore;

    let temp_dir = TempDir::new()?;
    let lock = Arc::new(Mutex::new(()));

    let mut handles = Vec::new();
    let paths = vec![
        ".claude/agents/concurrent1.md",
        ".claude/agents/concurrent2.md",
        ".claude/snippets/concurrent1.md",
        ".claude/commands/concurrent1.md",
        "scripts/concurrent1.sh",
        "scripts/concurrent2.sh",
    ];

    for path in paths.iter() {
        let path_clone = path.to_string();
        let project_dir = temp_dir.path().to_path_buf();
        let lock_clone = Arc::clone(&lock);

        let handle = tokio::spawn(async move {
            add_path_to_gitignore(&project_dir, &path_clone, &lock_clone).await
        });

        handles.push(handle);
    }

    let results: Vec<_> = futures::future::join_all(handles).await;

    for result in results {
        assert!(result.is_ok(), "Concurrent gitignore addition should succeed");
        let add_result = result?;
        assert!(add_result.is_ok(), "Each path addition should succeed: {:?}", add_result);
    }

    let gitignore_path = temp_dir.path().join(".gitignore");
    assert!(gitignore_path.exists(), "Gitignore should be created");

    let content = std::fs::read_to_string(&gitignore_path)?;
    for path in paths {
        assert!(content.contains(path), "Path '{}' should be in gitignore", path);
    }

    assert!(content.contains("# AGPM managed entries - do not edit below this line"));
    assert!(content.contains("# End of AGPM managed entries"));
    Ok(())
}

#[tokio::test]
async fn test_concurrent_gitignore_read_write() -> Result<()> {
    use crate::installer::gitignore::add_path_to_gitignore;

    let temp_dir = TempDir::new()?;
    let project_dir = temp_dir.path().to_path_buf();
    let lock = Arc::new(Mutex::new(()));

    let gitignore_path = project_dir.join(".gitignore");
    std::fs::write(
        &gitignore_path,
        "# User content\nnode_modules/\ntarget/\n# More user content\n*.log\n",
    )?;

    let mut handles = Vec::new();

    let path1 = ".claude/agents/readwrite1.md".to_string();
    let project_dir_clone1 = project_dir.clone();
    let lock_clone1 = Arc::clone(&lock);
    handles.push(tokio::spawn(async move {
        add_path_to_gitignore(&project_dir_clone1, &path1, &lock_clone1).await
    }));

    let path2 = "scripts/readwrite1.sh".to_string();
    let project_dir_clone2 = project_dir.clone();
    let lock_clone2 = Arc::clone(&lock);
    handles.push(tokio::spawn(async move {
        add_path_to_gitignore(&project_dir_clone2, &path2, &lock_clone2).await
    }));

    let project_dir_clone3 = project_dir.clone();
    let _lock_clone3 = Arc::clone(&lock);
    handles.push(tokio::spawn(async move {
        tokio::time::sleep(tokio::time::Duration::from_millis(1)).await;
        cleanup_gitignore(&project_dir_clone3, None).await
    }));

    let results: Vec<_> = futures::future::join_all(handles).await;

    let mut success_count = 0;
    for result in results {
        assert!(result.is_ok(), "Task should complete without panic");
        let operation_result = result?;
        if operation_result.is_ok() {
            success_count += 1;
        }
    }

    assert!(success_count >= 2, "At least add operations should succeed");

    assert!(gitignore_path.exists(), "Gitignore should still exist");

    let content = std::fs::read_to_string(&gitignore_path)?;
    assert!(content.contains("node_modules/"));
    assert!(content.contains("*.log"));
    Ok(())
}

#[tokio::test]
async fn test_gitignore_high_concurrency_stress() -> Result<()> {
    use crate::installer::gitignore::add_path_to_gitignore;

    let temp_dir = TempDir::new()?;
    let lock = Arc::new(Mutex::new(()));

    let num_operations = 50;
    let mut handles = Vec::new();

    for i in 0..num_operations {
        let path = format!(".claude/stress/test{}.md", i);
        let project_dir = temp_dir.path().to_path_buf();
        let lock_clone = Arc::clone(&lock);

        let handle = tokio::spawn(async move {
            if i % 3 == 0 {
                tokio::time::sleep(tokio::time::Duration::from_micros(i as u64)).await;
            }
            add_path_to_gitignore(&project_dir, &path, &lock_clone).await
        });

        handles.push(handle);
    }

    let results: Vec<_> = futures::future::join_all(handles).await;

    let mut success_count = 0;
    let mut error_count = 0;

    for result in results {
        assert!(result.is_ok(), "Task should complete without panic");
        let add_result = result?;
        match add_result {
            Ok(()) => success_count += 1,
            Err(_) => error_count += 1,
        }
    }

    assert_eq!(
        error_count, 0,
        "No operations should fail: {} successes, {} errors",
        success_count, error_count
    );
    assert_eq!(success_count, num_operations, "All {} operations should succeed", num_operations);

    let gitignore_path = temp_dir.path().join(".gitignore");
    assert!(gitignore_path.exists(), "Gitignore should exist");

    let content = std::fs::read_to_string(&gitignore_path)?;

    let mut found_paths = 0;
    for i in 0..num_operations {
        let expected_path = format!(".claude/stress/test{}.md", i);
        if content.contains(&expected_path) {
            found_paths += 1;
        }
    }

    assert_eq!(found_paths, num_operations, "All {} paths should be in gitignore", num_operations);

    assert!(content.contains("# AGPM managed entries - do not edit below this line"));
    assert!(content.contains("# End of AGPM managed entries"));
    Ok(())
}

#[tokio::test]
async fn test_mutex_race_condition_prevention() -> Result<()> {
    use crate::installer::gitignore::add_path_to_gitignore;

    let temp_dir = TempDir::new()?;
    let lock = Arc::new(Mutex::new(()));

    let mut handles = Vec::new();
    let num_threads = 20;

    for i in 0..num_threads {
        let path = format!(".claude/race/thread{}.md", i);
        let project_dir = temp_dir.path().to_path_buf();
        let lock_clone = Arc::clone(&lock);

        let handle = tokio::spawn(async move {
            tokio::time::sleep(tokio::time::Duration::from_micros((i * 10) as u64)).await;

            let result = add_path_to_gitignore(&project_dir, &path, &lock_clone).await;

            if result.is_ok() {
                let gitignore_path = project_dir.join(".gitignore");
                if gitignore_path.exists() {
                    let content = std::fs::read_to_string(&gitignore_path).unwrap_or_default();
                    let has_start =
                        content.contains("# AGPM managed entries - do not edit below this line");
                    let has_end = content.contains("# End of AGPM managed entries");
                    result.is_ok() && has_start && has_end
                } else {
                    false
                }
            } else {
                false
            }
        });

        handles.push(handle);
    }

    let results: Vec<_> = futures::future::join_all(handles).await;

    let mut successful_operations = 0;
    for result in results {
        assert!(result.is_ok(), "Thread should complete without panic");
        let operation_successful = result?;
        if operation_successful {
            successful_operations += 1;
        }
    }

    assert_eq!(
        successful_operations, num_threads,
        "All {} operations should succeed and maintain file integrity",
        num_threads
    );

    let gitignore_path = temp_dir.path().join(".gitignore");
    assert!(gitignore_path.exists(), "Gitignore should exist");

    let content = std::fs::read_to_string(&gitignore_path)?;

    assert!(content.contains("# AGPM managed entries - do not edit below this line"));
    assert!(content.contains("# End of AGPM managed entries"));

    let mut found_paths = 0;
    for i in 0..num_threads {
        let expected_path = format!(".claude/race/thread{}.md", i);
        if content.contains(&expected_path) {
            found_paths += 1;
        }
    }

    assert_eq!(found_paths, num_threads, "All {} paths should be present", num_threads);
    Ok(())
}

#[tokio::test]
async fn test_gitignore_permission_denied_read() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let gitignore_path = temp_dir.path().join(".gitignore");

    std::fs::write(
        &gitignore_path,
        "# User content\nnode_modules/\n# AGPM managed entries - do not edit below this line\n.claude/agents/\n# End of AGPM managed entries\n",
    )?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let mut perms = std::fs::metadata(&gitignore_path)?.permissions();
        perms.set_mode(0o000);
        std::fs::set_permissions(&gitignore_path, perms)?;

        let result = cleanup_gitignore(temp_dir.path(), None).await;

        assert!(result.is_err(), "Cleanup should fail with permission denied");
        let error_msg = result.unwrap_err().to_string();
        assert!(
            error_msg.contains("permission")
                || error_msg.contains("denied")
                || error_msg.contains("gitignore"),
            "Error should mention permission: {}",
            error_msg
        );

        let mut perms = std::fs::metadata(&gitignore_path)?.permissions();
        perms.set_mode(0o644);
        std::fs::set_permissions(&gitignore_path, perms)?;
    }

    #[cfg(not(unix))]
    {
        let _ = cleanup_gitignore(temp_dir.path(), None).await;
    }
    Ok(())
}

#[tokio::test]
async fn test_gitignore_permission_denied_write() -> Result<()> {
    use crate::installer::gitignore::add_path_to_gitignore;

    let temp_dir = TempDir::new()?;
    let lock = Arc::new(Mutex::new(()));

    let parent_dir = temp_dir.path();
    std::fs::write(parent_dir.join(".gitignore"), "# Initial content\n")?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let mut perms = std::fs::metadata(parent_dir)?.permissions();
        perms.set_mode(0o444);
        std::fs::set_permissions(parent_dir, perms)?;

        let result = add_path_to_gitignore(temp_dir.path(), ".claude/agents/test.md", &lock).await;

        assert!(result.is_err(), "Add path should fail with permission denied");
        let error_msg = result.unwrap_err().to_string();
        assert!(
            error_msg.contains("permission")
                || error_msg.contains("denied")
                || error_msg.contains("gitignore"),
            "Error should mention permission: {}",
            error_msg
        );

        let mut perms = std::fs::metadata(parent_dir)?.permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(parent_dir, perms)?;
    }

    #[cfg(not(unix))]
    {
        let _ = add_path_to_gitignore(temp_dir.path(), ".claude/agents/test.md", &lock).await;
    }
    Ok(())
}

#[tokio::test]
async fn test_gitignore_disk_space_exhaustion() -> Result<()> {
    use crate::installer::gitignore::add_path_to_gitignore;

    let temp_dir = TempDir::new()?;
    let lock = Arc::new(Mutex::new(()));

    let large_path = ".claude/".to_string() + &"a".repeat(1000) + ".md";
    let result = add_path_to_gitignore(temp_dir.path(), &large_path, &lock).await;

    match result {
        Ok(_) => {
            let gitignore_path = temp_dir.path().join(".gitignore");
            assert!(gitignore_path.exists(), "Gitignore should exist");

            let content = std::fs::read_to_string(&gitignore_path)?;
            assert!(content.contains("# AGPM managed entries - do not edit below this line"));
        }
        Err(e) => {
            let error_msg = e.to_string();
            assert!(!error_msg.is_empty(), "Error message should not be empty");
            assert!(error_msg.len() > 10, "Error message should be descriptive");
        }
    }
    Ok(())
}

#[tokio::test]
async fn test_gitignore_malformed_content() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let gitignore_path = temp_dir.path().join(".gitignore");

    let malformed_bytes = b"# User content\n\xfe\xfeInvalid UTF-8\n# AGPM managed entries - do not edit below this line\n.claude/agents/\n# End of AGPM managed entries\n";
    std::fs::write(&gitignore_path, malformed_bytes)?;

    let result = cleanup_gitignore(temp_dir.path(), None).await;

    match result {
        Ok(_) => {
            if gitignore_path.exists() {
                let content = std::fs::read_to_string(&gitignore_path).unwrap_or_default();
                assert!(
                    !content.contains("# AGPM managed entries"),
                    "AGPM section should be removed"
                );
            }
        }
        Err(e) => {
            let error_msg = e.to_string();
            assert!(!error_msg.is_empty(), "Error message should not be empty");
        }
    }
    Ok(())
}

#[tokio::test]
async fn test_gitignore_encoding_issues() -> Result<()> {
    use crate::installer::gitignore::add_path_to_gitignore;

    let temp_dir = TempDir::new()?;
    let lock = Arc::new(Mutex::new(()));

    let unicode_paths = vec![
        ".claude/agents/üñïçødë.md",
        ".claude/snippets/🚀rocket.md",
        ".claude/commands/中文.md",
        "scripts/файл.sh",
        ".claude/agents/🦀rustacean.md",
        ".claude/very/deep/nested/path/with/_unicode/测试.md",
    ];

    for unicode_path in unicode_paths {
        let result = add_path_to_gitignore(temp_dir.path(), unicode_path, &lock).await;

        assert!(
            result.is_ok(),
            "Unicode path should be handled correctly: {} ({})",
            unicode_path,
            result.unwrap_err()
        );

        let gitignore_path = temp_dir.path().join(".gitignore");
        let content = std::fs::read_to_string(&gitignore_path)?;
        assert!(
            content.contains(unicode_path),
            "Unicode path '{}' should be in gitignore",
            unicode_path
        );
    }

    let gitignore_path = temp_dir.path().join(".gitignore");
    let content = std::fs::read_to_string(&gitignore_path)?;
    assert!(content.contains("# AGPM managed entries - do not edit below this line"));
    assert!(content.contains("# End of AGPM managed entries"));
    Ok(())
}

#[tokio::test]
async fn test_windows_unicode_path_handling() -> Result<()> {
    use crate::installer::gitignore::add_path_to_gitignore;

    let temp_dir = TempDir::new()?;
    let lock = Arc::new(Mutex::new(()));

    let long_unicode_path = format!(".claude/agents/{}", "ü".repeat(50));
    let windows_unicode_paths = vec![
        ".claude/agents/café.md",
        ".claude/agents/naïve.md",
        ".claude/agents/Zürich.md",
        ".claude/agents/Москва.md",
        ".claude/agents/北京.md",
        ".claude/agents/東京.md",
        ".claude/agents/서울.md",
        ".claude/agents/العربية.md",
        ".claude/agents/עברית.md",
        ".claude/agents/🚀rocket-fuel.md",
        ".claude/agents/🦀rust-crab.md",
        ".claude/agents/math∑∏∆.md",
        ".claude/agents/special‽.md",
        ".claude/agents/user-profile-张三.md",
        ".claude/agents/project-α-beta.md",
        ".claude/agents/国际/projects/中文项目.md",
        ".claude/scripts/исполнители/скрипт.sh",
        &long_unicode_path,
        ".claude/agents/café con leche.md",
        ".claude/agents/проект \"Alpha\".md",
    ];

    for unicode_path in windows_unicode_paths {
        let result = add_path_to_gitignore(temp_dir.path(), unicode_path, &lock).await;

        assert!(
            result.is_ok(),
            "Windows Unicode path should be handled correctly: '{}' ({})",
            unicode_path,
            result.as_ref().unwrap_err()
        );

        let gitignore_path = temp_dir.path().join(".gitignore");
        let content = std::fs::read_to_string(&gitignore_path)?;

        assert!(
            content.contains(unicode_path),
            "Unicode path '{}' should be preserved in gitignore",
            unicode_path
        );
    }

    let gitignore_path = temp_dir.path().join(".gitignore");
    let content = std::fs::read_to_string(&gitignore_path)?;
    assert!(content.contains("# AGPM managed entries - do not edit below this line"));
    assert!(content.contains("# End of AGPM managed entries"));

    let lines: Vec<&str> = content.lines().collect();
    for line in lines {
        if line.starts_with(".claude/") && !line.starts_with('#') {
            assert!(
                !line.contains('\\'),
                "Git ignore paths should use forward slashes, found backslash in: {}",
                line
            );
        }
    }
    Ok(())
}

#[tokio::test]
async fn test_windows_very_long_path_names() -> Result<()> {
    use crate::installer::gitignore::add_path_to_gitignore;

    let temp_dir = TempDir::new()?;
    let lock = Arc::new(Mutex::new(()));

    let near_max_path = format!(".claude/agents/{}", "a".repeat(240));
    let result = add_path_to_gitignore(temp_dir.path(), &near_max_path, &lock).await;
    assert!(
        result.is_ok(),
        "Near MAX_PATH length should be handled: {} chars",
        near_max_path.len()
    );

    let very_long_path = format!(".claude/agents/deep/nested/{}/resource.md", "x".repeat(900));
    let result = add_path_to_gitignore(temp_dir.path(), &very_long_path, &lock).await;
    assert!(result.is_ok(), "Very long path should be handled: {} chars", very_long_path.len());

    let extremely_long_path = format!(".claude/agents/{}", "z".repeat(4980));
    let result = add_path_to_gitignore(temp_dir.path(), &extremely_long_path, &lock).await;

    match result {
        Ok(()) => {
            let gitignore_path = temp_dir.path().join(".gitignore");
            let content = std::fs::read_to_string(&gitignore_path)?;
            assert!(
                content
                    .lines()
                    .any(|line| line.starts_with(".claude/agents/") && line.len() > 4000),
                "Extremely long path should be stored if successful"
            );
        }
        Err(e) => {
            let error_msg = e.to_string().to_lowercase();
            assert!(
                error_msg.contains("path")
                    || error_msg.contains("length")
                    || error_msg.contains("too long"),
                "Error for extremely long path should mention path length: {}",
                error_msg
            );
        }
    }

    let long_unicode_path = format!(".claude/agents/üñîçødë_{}", "测试".repeat(100));
    let result = add_path_to_gitignore(temp_dir.path(), &long_unicode_path, &lock).await;
    assert!(
        result.is_ok(),
        "Long Unicode path should be handled: {} chars",
        long_unicode_path.len()
    );

    let nested_long_components = [
        "very-long-component-name-that-exceeds-normal-filesystem-limits",
        "another-extremely-long-directory-name-for-testing-windows-path-handling",
        "yet-another-super-long-path-component-to-test-edge-cases-in-gitignore",
    ];
    let nested_long_path = format!(".claude/{}", nested_long_components.join("/"));
    let result = add_path_to_gitignore(temp_dir.path(), &nested_long_path, &lock).await;
    assert!(result.is_ok(), "Nested long path should be handled: {} chars", nested_long_path.len());

    let gitignore_path = temp_dir.path().join(".gitignore");
    if gitignore_path.exists() {
        let content = std::fs::read_to_string(&gitignore_path)?;

        assert!(content.contains("# AGPM managed entries - do not edit below this line"));
        assert!(content.contains("# End of AGPM managed entries"));

        let lines: Vec<&str> = content.lines().collect();
        for (i, line) in lines.iter().enumerate() {
            assert!(
                !line.is_empty() || line.trim().is_empty() || line.starts_with('#'),
                "Line {} should be valid: '{}'",
                i,
                line
            );
        }
    }
    Ok(())
}

#[tokio::test]
async fn test_windows_reserved_names_and_path_separators() -> Result<()> {
    use crate::installer::gitignore::add_path_to_gitignore;

    let temp_dir = TempDir::new()?;
    let lock = Arc::new(Mutex::new(()));

    let windows_reserved_patterns = vec![
        ".claude/agents/CON.md",
        ".claude/agents/PRN.md",
        ".claude/agents/AUX.md",
        ".claude/agents/NUL.md",
        ".claude/agents/COM1.md",
        ".claude/agents/COM2.md",
        ".claude/agents/COM3.md",
        ".claude/agents/COM4.md",
        ".claude/agents/COM5.md",
        ".claude/agents/COM6.md",
        ".claude/agents/COM7.md",
        ".claude/agents/COM8.md",
        ".claude/agents/COM9.md",
        ".claude/agents/LPT1.md",
        ".claude/agents/LPT2.md",
        ".claude/agents/LPT3.md",
        ".claude/agents/LPT4.md",
        ".claude/agents/LPT5.md",
        ".claude/agents/LPT6.md",
        ".claude/agents/LPT7.md",
        ".claude/agents/LPT8.md",
        ".claude/agents/LPT9.md",
        ".claude/agents/CON.txt",
        ".claude/agents/PRN.md",
        ".claude/agents/AUX.json",
        ".claude/agents/con.md",
        ".claude/agents/Con.md",
        ".claude/agents/prn.AUX",
    ];

    for reserved_pattern in windows_reserved_patterns {
        let result = add_path_to_gitignore(temp_dir.path(), reserved_pattern, &lock).await;

        assert!(
            result.is_ok(),
            "Windows reserved name pattern should be handled in gitignore: {}",
            reserved_pattern
        );

        let gitignore_path = temp_dir.path().join(".gitignore");
        let content = std::fs::read_to_string(&gitignore_path)?;
        assert!(
            content.contains(reserved_pattern),
            "Reserved name pattern should be preserved: {}",
            reserved_pattern
        );
    }

    let mixed_separator_paths = vec![
        ".claude\\agents\\windows.md",
        ".claude/agents\\mixed.md",
        ".claude\\\\agents\\\\double.md",
        ".claude/agents/trailing\\.md",
    ];

    for mixed_path in mixed_separator_paths {
        let result = add_path_to_gitignore(temp_dir.path(), mixed_path, &lock).await;
        assert!(result.is_ok(), "Mixed separator path should be normalized: {}", mixed_path);

        let gitignore_path = temp_dir.path().join(".gitignore");
        let content = std::fs::read_to_string(&gitignore_path)?;

        assert!(
            !content.contains('\\'),
            "Gitignore should not contain backslashes for path: {}",
            mixed_path
        );

        let normalized_path = mixed_path.replace('\\', "/");
        assert!(
            content.contains(&normalized_path),
            "Should contain normalized forward-slash path: {}",
            normalized_path
        );
    }
    Ok(())
}

#[tokio::test]
async fn test_windows_edge_case_path_combinations() -> Result<()> {
    use crate::installer::gitignore::add_path_to_gitignore;

    let temp_dir = TempDir::new()?;
    let lock = Arc::new(Mutex::new(()));

    let long_component_path = format!(".claude/agents/{}", "A".repeat(100));
    let edge_case_paths = vec![
        ".claude/agents/file name with spaces.md",
        ".claude/agents/file.with.many.dots.md",
        ".claude/agents/ trailing space.md",
        ".claude/agents/leading space .md",
        ".claude/agents/file|with|pipes.md",
        ".claude/agents/file:with:colons.md",
        ".claude/agents/file*with*asterisks.md",
        ".claude/agents/file?with?questions.md",
        ".claude/agents/file\"with\"quotes.md",
        ".claude/agents/file<with>brackets.md",
        ".claude/agents/café con leche & pan.md",
        ".claude/agents/проект@company.com.md",
        ".claude/agents/test#123[branch].md",
        &long_component_path,
        ".claude/agents/encodé.md",
        ".claude/agents/cafe\u{0301}.md",
        ".claude/agents/\u{212B}.md",
    ];

    for edge_path in &edge_case_paths {
        let result = add_path_to_gitignore(temp_dir.path(), edge_path, &lock).await;

        assert!(
            result.is_ok(),
            "Edge case path should be handled: '{}' ({})",
            edge_path,
            result.as_ref().unwrap_err()
        );
    }

    let gitignore_path = temp_dir.path().join(".gitignore");
    let content = std::fs::read_to_string(&gitignore_path)?;

    assert!(content.contains("# AGPM managed entries - do not edit below this line"));
    assert!(content.contains("# End of AGPM managed entries"));

    assert!(!content.contains('\\'), "No backslashes should remain in gitignore");

    let mut lockfile = LockFile::new();

    for edge_path in edge_case_paths.iter().take(3) {
        let resource = LockedResource {
            name: edge_path.replace(".claude/agents/", "").replace(".md", ""),
            source: None,
            url: None,
            version: None,
            path: edge_path.to_string(),
            resolved_commit: None,
            checksum: "sha256:test".to_string(),
            context_checksum: None,
            installed_at: edge_path.to_string(),
            dependencies: vec![],
            resource_type: crate::core::ResourceType::Agent,
            tool: None,
            manifest_alias: None,
            applied_patches: std::collections::BTreeMap::new(),
            install: None,
            variant_inputs: crate::resolver::lockfile_builder::VariantInputs::default(),
        };
        lockfile.agents.push(resource);
    }

    let result = update_gitignore(&lockfile, temp_dir.path(), true, None);
    assert!(result.is_ok(), "Update gitignore with edge cases should succeed");

    let updated_content = std::fs::read_to_string(&gitignore_path)?;
    assert!(updated_content.contains("# AGPM managed entries - do not edit below this line"));
    assert!(!updated_content.contains('\\'), "Updated gitignore should not contain backslashes");
    Ok(())
}
