//! Shared installation utilities for CCPM resources.
//!
//! This module provides common functionality for installing resources from
//! lockfile entries to the project directory. It's shared between the install
//! and update commands to avoid code duplication.

use anyhow::{Context, Result};
use futures::future::try_join_all;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::cache::Cache;
use crate::lockfile::{LockFile, LockedResource};
use crate::manifest::Manifest;
use crate::markdown::MarkdownFile;
use crate::utils::fs::{atomic_write, ensure_dir};
use crate::utils::progress::ProgressBar;

/// Install a single resource from a lock entry
pub async fn install_resource(
    entry: &LockedResource,
    project_dir: &Path,
    resource_dir: &str,
    cache: &Cache,
) -> Result<()> {
    // Determine destination path
    let dest_path = if entry.installed_at.is_empty() {
        // Default location based on resource type
        project_dir
            .join(resource_dir)
            .join(format!("{}.md", entry.name))
    } else {
        project_dir.join(&entry.installed_at)
    };

    // Install based on source type
    if let Some(source_name) = &entry.source {
        // Remote resource - use cache
        let url = entry
            .url
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Remote resource {} has no URL", entry.name))?;

        // Get or clone the source to cache
        let cache_dir = cache
            .get_or_clone_source(
                source_name,
                url,
                entry
                    .resolved_commit
                    .as_ref()
                    .or(entry.version.as_ref())
                    .map(std::string::String::as_str),
            )
            .await
            .with_context(|| {
                format!(
                    "Failed to sync source '{}' for resource '{}'",
                    source_name, entry.name
                )
            })?;

        // Source path in the cache
        let source_path = cache_dir.join(&entry.path);

        // Read and validate markdown file
        let content = std::fs::read_to_string(&source_path)
            .with_context(|| format!("Failed to read resource file: {}", source_path.display()))?;

        // Validate markdown format
        let _markdown = MarkdownFile::parse(&content)
            .with_context(|| format!("Invalid markdown file: {}", source_path.display()))?;

        // Ensure destination directory exists
        if let Some(parent) = dest_path.parent() {
            ensure_dir(parent)?;
        }

        // Write to destination with atomic operation
        atomic_write(&dest_path, content.as_bytes())
            .with_context(|| format!("Failed to install resource to {}", dest_path.display()))?;
    } else {
        // Local resource
        let source_path = Path::new(&entry.path);
        if !source_path.exists() {
            return Err(anyhow::anyhow!(
                "Local resource file does not exist: {}",
                source_path.display()
            ));
        }

        // Read and validate markdown file
        let content = std::fs::read_to_string(source_path)
            .with_context(|| format!("Failed to read resource file: {}", source_path.display()))?;

        // Validate markdown format
        let _markdown = MarkdownFile::parse(&content)
            .with_context(|| format!("Invalid markdown file: {}", source_path.display()))?;

        // Ensure destination directory exists
        if let Some(parent) = dest_path.parent() {
            ensure_dir(parent)?;
        }

        // Write to destination
        atomic_write(&dest_path, content.as_bytes())
            .with_context(|| format!("Failed to install resource to {}", dest_path.display()))?;
    }

    Ok(())
}

/// Install a single resource with progress tracking
pub async fn install_resource_with_progress(
    entry: &LockedResource,
    project_dir: &Path,
    resource_dir: &str,
    pb: &ProgressBar,
    cache: &Cache,
) -> Result<()> {
    pb.set_message(format!("Installing {}", entry.name));
    install_resource(entry, project_dir, resource_dir, cache).await
}

/// Install multiple resources in parallel
pub async fn install_resources_parallel(
    lockfile: &LockFile,
    manifest: &Manifest,
    project_dir: &Path,
    pb: &ProgressBar,
    cache: &Cache,
) -> Result<usize> {
    // Collect all entries to install
    let mut all_entries = Vec::new();

    // Add all entries
    for entry in &lockfile.agents {
        all_entries.push((entry, manifest.target.agents.as_str()));
    }
    for entry in &lockfile.snippets {
        all_entries.push((entry, manifest.target.snippets.as_str()));
    }
    for entry in &lockfile.commands {
        all_entries.push((entry, manifest.target.commands.as_str()));
    }

    if all_entries.is_empty() {
        return Ok(0);
    }

    // Create thread-safe progress tracking
    let installed_count = Arc::new(Mutex::new(0));
    let total = all_entries.len();
    let pb = Arc::new(pb.clone());

    // Wrap the cache in Arc so it can be shared across async tasks
    let cache = Arc::new(cache);

    // Set initial progress
    pb.set_message(format!("Installing 0/{total} resources"));

    // Create installation tasks
    let tasks = all_entries.into_iter().map(|(entry, resource_dir)| {
        let entry = entry.clone();
        let project_dir = project_dir.to_path_buf();
        let resource_dir = resource_dir.to_string();
        let installed_count = Arc::clone(&installed_count);
        let pb = Arc::clone(&pb);
        let cache = Arc::clone(&cache);

        async move {
            // Install the resource
            install_resource_for_parallel(&entry, &project_dir, &resource_dir, cache.as_ref())
                .await?;

            // Update progress
            let mut count = installed_count.lock().await;
            *count += 1;
            pb.set_message(format!("Installing {}/{} resources", *count, total));

            Ok::<(), anyhow::Error>(())
        }
    });

    // Execute all tasks in parallel
    try_join_all(tasks).await?;

    let final_count = *installed_count.lock().await;
    Ok(final_count)
}

/// Install a single resource in a thread-safe manner (for parallel execution)
async fn install_resource_for_parallel(
    entry: &LockedResource,
    project_dir: &Path,
    resource_dir: &str,
    cache: &Cache,
) -> Result<()> {
    install_resource(entry, project_dir, resource_dir, cache).await
}

/// Install only specific updated resources
pub async fn install_updated_resources(
    updates: &[(String, String, String)], // (name, old_version, new_version)
    lockfile: &LockFile,
    manifest: &Manifest,
    project_dir: &Path,
    cache: &Cache,
    quiet: bool,
) -> Result<usize> {
    let mut install_count = 0;

    for (name, _, _) in updates {
        // Find the resource in the lockfile
        if let Some(entry) = lockfile.agents.iter().find(|e| &e.name == name) {
            if !quiet {
                println!("  Installing {name} (agent)");
            }
            install_resource(entry, project_dir, &manifest.target.agents, cache).await?;
            install_count += 1;
        } else if let Some(entry) = lockfile.snippets.iter().find(|e| &e.name == name) {
            if !quiet {
                println!("  Installing {name} (snippet)");
            }
            install_resource(entry, project_dir, &manifest.target.snippets, cache).await?;
            install_count += 1;
        } else if let Some(entry) = lockfile.commands.iter().find(|e| &e.name == name) {
            if !quiet {
                println!("  Installing {name} (command)");
            }
            install_resource(entry, project_dir, &manifest.target.commands, cache).await?;
            install_count += 1;
        }
    }

    Ok(install_count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_locked_resource(name: &str, is_local: bool) -> LockedResource {
        if is_local {
            LockedResource {
                name: name.to_string(),
                source: None,
                url: None,
                path: "test.md".to_string(),
                version: None,
                resolved_commit: None,
                checksum: String::new(),
                installed_at: String::new(),
            }
        } else {
            LockedResource {
                name: name.to_string(),
                source: Some("test_source".to_string()),
                url: Some("https://github.com/test/repo.git".to_string()),
                path: "resources/test.md".to_string(),
                version: Some("v1.0.0".to_string()),
                resolved_commit: Some("abc123".to_string()),
                checksum: "sha256:test".to_string(),
                installed_at: String::new(),
            }
        }
    }

    #[tokio::test]
    async fn test_install_resource_local() {
        let temp_dir = TempDir::new().unwrap();
        let project_dir = temp_dir.path();
        let cache = Cache::with_dir(temp_dir.path().join("cache")).unwrap();

        // Create a local markdown file
        let local_file = temp_dir.path().join("test.md");
        std::fs::write(&local_file, "# Test Resource\nThis is a test").unwrap();

        // Create a locked resource pointing to the local file
        let mut entry = create_test_locked_resource("local-test", true);
        entry.path = local_file.to_string_lossy().to_string();

        // Install the resource
        let result = install_resource(&entry, project_dir, "agents", &cache).await;
        assert!(
            result.is_ok(),
            "Failed to install local resource: {:?}",
            result
        );

        // Verify the file was installed
        let expected_path = project_dir.join("agents").join("local-test.md");
        assert!(expected_path.exists(), "Installed file not found");

        // Verify content
        let content = std::fs::read_to_string(expected_path).unwrap();
        assert_eq!(content, "# Test Resource\nThis is a test");
    }

    #[tokio::test]
    async fn test_install_resource_with_custom_path() {
        let temp_dir = TempDir::new().unwrap();
        let project_dir = temp_dir.path();
        let cache = Cache::with_dir(temp_dir.path().join("cache")).unwrap();

        // Create a local markdown file
        let local_file = temp_dir.path().join("test.md");
        std::fs::write(&local_file, "# Custom Path Test").unwrap();

        // Create a locked resource with custom installation path
        let mut entry = create_test_locked_resource("custom-test", true);
        entry.path = local_file.to_string_lossy().to_string();
        entry.installed_at = "custom/location/resource.md".to_string();

        // Install the resource
        let result = install_resource(&entry, project_dir, "agents", &cache).await;
        assert!(result.is_ok());

        // Verify the file was installed at custom path
        let expected_path = project_dir.join("custom/location/resource.md");
        assert!(expected_path.exists(), "File not installed at custom path");
    }

    #[tokio::test]
    async fn test_install_resource_local_missing_file() {
        let temp_dir = TempDir::new().unwrap();
        let project_dir = temp_dir.path();
        let cache = Cache::with_dir(temp_dir.path().join("cache")).unwrap();

        // Create a locked resource pointing to non-existent file
        let mut entry = create_test_locked_resource("missing-test", true);
        entry.path = "/non/existent/file.md".to_string();

        // Try to install the resource
        let result = install_resource(&entry, project_dir, "agents", &cache).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("does not exist"));
    }

    #[tokio::test]
    async fn test_install_resource_invalid_markdown() {
        let temp_dir = TempDir::new().unwrap();
        let project_dir = temp_dir.path();
        let cache = Cache::with_dir(temp_dir.path().join("cache")).unwrap();

        // Create an invalid markdown file
        let local_file = temp_dir.path().join("invalid.md");
        std::fs::write(&local_file, "---\ninvalid: yaml: [\n---\nContent").unwrap();

        // Create a locked resource
        let mut entry = create_test_locked_resource("invalid-test", true);
        entry.path = local_file.to_string_lossy().to_string();

        // Try to install the resource
        let result = install_resource(&entry, project_dir, "agents", &cache).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Invalid markdown"));
    }

    #[tokio::test]
    async fn test_install_resource_with_progress() {
        let temp_dir = TempDir::new().unwrap();
        let project_dir = temp_dir.path();
        let cache = Cache::with_dir(temp_dir.path().join("cache")).unwrap();
        let pb = ProgressBar::new(1);

        // Create a local markdown file
        let local_file = temp_dir.path().join("test.md");
        std::fs::write(&local_file, "# Progress Test").unwrap();

        // Create a locked resource
        let mut entry = create_test_locked_resource("progress-test", true);
        entry.path = local_file.to_string_lossy().to_string();

        // Install with progress
        let result =
            install_resource_with_progress(&entry, project_dir, "agents", &pb, &cache).await;
        assert!(result.is_ok());

        // Verify installation
        let expected_path = project_dir.join("agents").join("progress-test.md");
        assert!(expected_path.exists());
    }

    #[tokio::test]
    async fn test_install_resources_parallel_empty() {
        let temp_dir = TempDir::new().unwrap();
        let project_dir = temp_dir.path();
        let cache = Cache::with_dir(temp_dir.path().join("cache")).unwrap();
        let pb = ProgressBar::new(1);

        // Create empty lockfile and manifest
        let lockfile = LockFile::new();
        let manifest = Manifest::new();

        let count = install_resources_parallel(&lockfile, &manifest, project_dir, &pb, &cache)
            .await
            .unwrap();

        assert_eq!(count, 0, "Should install 0 resources from empty lockfile");
    }

    #[tokio::test]
    async fn test_install_resources_parallel_multiple() {
        let temp_dir = TempDir::new().unwrap();
        let project_dir = temp_dir.path();
        let cache = Cache::with_dir(temp_dir.path().join("cache")).unwrap();
        let pb = ProgressBar::new(1);

        // Create test markdown files
        let file1 = temp_dir.path().join("agent.md");
        let file2 = temp_dir.path().join("snippet.md");
        let file3 = temp_dir.path().join("command.md");
        std::fs::write(&file1, "# Agent").unwrap();
        std::fs::write(&file2, "# Snippet").unwrap();
        std::fs::write(&file3, "# Command").unwrap();

        // Create lockfile with multiple resources
        let mut lockfile = LockFile::new();
        let mut agent = create_test_locked_resource("test-agent", true);
        agent.path = file1.to_string_lossy().to_string();
        lockfile.agents.push(agent);

        let mut snippet = create_test_locked_resource("test-snippet", true);
        snippet.path = file2.to_string_lossy().to_string();
        lockfile.snippets.push(snippet);

        let mut command = create_test_locked_resource("test-command", true);
        command.path = file3.to_string_lossy().to_string();
        lockfile.commands.push(command);

        let manifest = Manifest::new();

        let count = install_resources_parallel(&lockfile, &manifest, project_dir, &pb, &cache)
            .await
            .unwrap();

        assert_eq!(count, 3, "Should install 3 resources");

        // Verify all files were installed (using default directories)
        assert!(project_dir
            .join(".claude/agents/ccpm/test-agent.md")
            .exists());
        assert!(project_dir
            .join(".claude/ccpm/snippets/test-snippet.md")
            .exists());
        assert!(project_dir
            .join(".claude/commands/ccpm/test-command.md")
            .exists());
    }

    #[tokio::test]
    async fn test_install_updated_resources() {
        let temp_dir = TempDir::new().unwrap();
        let project_dir = temp_dir.path();
        let cache = Cache::with_dir(temp_dir.path().join("cache")).unwrap();

        // Create test markdown files
        let file1 = temp_dir.path().join("agent.md");
        let file2 = temp_dir.path().join("snippet.md");
        std::fs::write(&file1, "# Updated Agent").unwrap();
        std::fs::write(&file2, "# Updated Snippet").unwrap();

        // Create lockfile with resources
        let mut lockfile = LockFile::new();
        let mut agent = create_test_locked_resource("test-agent", true);
        agent.path = file1.to_string_lossy().to_string();
        lockfile.agents.push(agent);

        let mut snippet = create_test_locked_resource("test-snippet", true);
        snippet.path = file2.to_string_lossy().to_string();
        lockfile.snippets.push(snippet);

        let manifest = Manifest::new();

        // Define updates (only agent is updated)
        let updates = vec![(
            "test-agent".to_string(),
            "v1.0.0".to_string(),
            "v1.1.0".to_string(),
        )];

        let count = install_updated_resources(
            &updates,
            &lockfile,
            &manifest,
            project_dir,
            &cache,
            false, // quiet
        )
        .await
        .unwrap();

        assert_eq!(count, 1, "Should install 1 updated resource");
        assert!(project_dir
            .join(".claude/agents/ccpm/test-agent.md")
            .exists());
        assert!(!project_dir
            .join(".claude/snippets/test-snippet.md")
            .exists()); // Not updated
    }

    #[tokio::test]
    async fn test_install_updated_resources_quiet_mode() {
        let temp_dir = TempDir::new().unwrap();
        let project_dir = temp_dir.path();
        let cache = Cache::with_dir(temp_dir.path().join("cache")).unwrap();

        // Create test markdown file
        let file = temp_dir.path().join("command.md");
        std::fs::write(&file, "# Command").unwrap();

        // Create lockfile
        let mut lockfile = LockFile::new();
        let mut command = create_test_locked_resource("test-command", true);
        command.path = file.to_string_lossy().to_string();
        lockfile.commands.push(command);

        let manifest = Manifest::new();

        let updates = vec![(
            "test-command".to_string(),
            "v1.0.0".to_string(),
            "v2.0.0".to_string(),
        )];

        let count = install_updated_resources(
            &updates,
            &lockfile,
            &manifest,
            project_dir,
            &cache,
            true, // quiet mode
        )
        .await
        .unwrap();

        assert_eq!(count, 1);
        assert!(project_dir
            .join(".claude/commands/ccpm/test-command.md")
            .exists());
    }

    #[tokio::test]
    async fn test_install_resource_for_parallel() {
        let temp_dir = TempDir::new().unwrap();
        let project_dir = temp_dir.path();
        let cache = Cache::with_dir(temp_dir.path().join("cache")).unwrap();

        // Create a local markdown file
        let local_file = temp_dir.path().join("parallel.md");
        std::fs::write(&local_file, "# Parallel Test").unwrap();

        // Create a locked resource
        let mut entry = create_test_locked_resource("parallel-test", true);
        entry.path = local_file.to_string_lossy().to_string();

        // Install using the parallel function
        let result = install_resource_for_parallel(&entry, project_dir, "agents", &cache).await;
        assert!(result.is_ok());

        // Verify installation
        let expected_path = project_dir.join("agents").join("parallel-test.md");
        assert!(expected_path.exists());
    }

    #[tokio::test]
    async fn test_install_resource_creates_nested_directories() {
        let temp_dir = TempDir::new().unwrap();
        let project_dir = temp_dir.path();
        let cache = Cache::with_dir(temp_dir.path().join("cache")).unwrap();

        // Create a local markdown file
        let local_file = temp_dir.path().join("nested.md");
        std::fs::write(&local_file, "# Nested Test").unwrap();

        // Create a locked resource with deeply nested path
        let mut entry = create_test_locked_resource("nested-test", true);
        entry.path = local_file.to_string_lossy().to_string();
        entry.installed_at = "very/deeply/nested/path/resource.md".to_string();

        // Install the resource
        let result = install_resource(&entry, project_dir, "agents", &cache).await;
        assert!(result.is_ok());

        // Verify nested directories were created
        let expected_path = project_dir.join("very/deeply/nested/path/resource.md");
        assert!(expected_path.exists());
    }

    #[tokio::test]
    async fn test_install_updated_resources_not_found() {
        let temp_dir = TempDir::new().unwrap();
        let project_dir = temp_dir.path();
        let cache = Cache::with_dir(temp_dir.path().join("cache")).unwrap();

        let lockfile = LockFile::new();
        let manifest = Manifest::new();

        // Try to update a resource that doesn't exist
        let updates = vec![(
            "non-existent".to_string(),
            "v1.0.0".to_string(),
            "v2.0.0".to_string(),
        )];

        let count =
            install_updated_resources(&updates, &lockfile, &manifest, project_dir, &cache, false)
                .await
                .unwrap();

        assert_eq!(count, 0, "Should install 0 resources when not found");
    }
}
