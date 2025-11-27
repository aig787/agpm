#[cfg(test)]
mod installer_tests {
    use crate::cache::Cache;
    use crate::installer::{
        InstallContext, ResourceFilter, install_resource, install_resource_with_progress,
        install_resources, install_updated_resources, update_gitignore,
    };
    use crate::lockfile::{LockFile, LockedResource};
    use crate::manifest::Manifest;

    use crate::utils::ensure_dir;
    use anyhow::Result;
    use indicatif::ProgressBar;
    use std::sync::Arc;
    use tempfile::TempDir;

    fn create_test_locked_resource(name: &str, is_local: bool) -> LockedResource {
        if is_local {
            LockedResource {
                name: name.to_string(),
                source: None,
                url: None,
                version: None,
                path: format!("{}.md", name),
                resolved_commit: None,
                checksum: "sha256:test".to_string(),
                context_checksum: None,
                installed_at: String::new(),
                dependencies: vec![],
                resource_type: crate::core::ResourceType::Agent,
                tool: None,
                manifest_alias: None,
                applied_patches: std::collections::BTreeMap::new(),
                install: None,
                variant_inputs: crate::resolver::lockfile_builder::VariantInputs::default(),
            }
        } else {
            LockedResource {
                name: name.to_string(),
                source: Some("test_source".to_string()),
                url: Some("https://github.com/test/repo.git".to_string()),
                version: Some("v1.0.0".to_string()),
                path: format!("{}.md", name),
                resolved_commit: None,
                checksum: "sha256:test".to_string(),
                context_checksum: None,
                installed_at: format!("{}.md", name),
                dependencies: vec![],
                resource_type: crate::core::ResourceType::Agent,
                tool: None,
                manifest_alias: None,
                applied_patches: std::collections::BTreeMap::new(),
                install: None,
                variant_inputs: crate::resolver::lockfile_builder::VariantInputs::default(),
            }
        }
    }

    #[tokio::test]
    async fn test_install_resource_local() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let project_dir = temp_dir.path();
        let cache = Cache::with_dir(temp_dir.path().join("cache"))?;

        let local_file = temp_dir.path().join("test.md");
        std::fs::write(&local_file, "# Test Resource\nThis is a test")?;

        let mut entry = create_test_locked_resource("local-test", true);
        entry.path = local_file.to_string_lossy().to_string();

        let context = InstallContext::builder(project_dir, &cache).build();

        let result = install_resource(&entry, "agents", &context).await;
        assert!(result.is_ok(), "Failed to install local resource: {:?}", result);

        let (installed, _checksum, _context_checksum, _applied_patches) = result?;
        assert!(installed, "Should have installed new resource");

        let expected_path = project_dir.join("agents").join("local-test.md");
        assert!(expected_path.exists(), "Installed file not found");

        let content = std::fs::read_to_string(expected_path)?;
        assert_eq!(content, "# Test Resource\nThis is a test");
        Ok(())
    }

    #[tokio::test]
    async fn test_install_resource_with_custom_path() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let project_dir = temp_dir.path();
        let cache = Cache::with_dir(temp_dir.path().join("cache"))?;

        let local_file = temp_dir.path().join("test.md");
        std::fs::write(&local_file, "# Custom Path Test")?;

        let mut entry = create_test_locked_resource("custom-test", true);
        entry.path = local_file.to_string_lossy().to_string();
        entry.installed_at = "custom/location/resource.md".to_string();

        let context = InstallContext::builder(project_dir, &cache).build();

        let result = install_resource(&entry, "agents", &context).await;
        let (installed, _checksum, _context_checksum, _applied_patches) = result?;
        assert!(installed, "Should have installed new resource");

        let expected_path = project_dir.join("custom/location/resource.md");
        assert!(expected_path.exists(), "File not installed at custom path");
        Ok(())
    }

    #[tokio::test]
    async fn test_install_resource_local_missing_file() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let project_dir = temp_dir.path();
        let cache = Cache::with_dir(temp_dir.path().join("cache"))?;

        let mut entry = create_test_locked_resource("missing-test", true);
        entry.path = "/non/existent/file.md".to_string();

        let context = InstallContext::builder(project_dir, &cache).build();

        let result = install_resource(&entry, "agents", &context).await;
        assert!(result.is_err());
        let error_msg = result.unwrap_err().to_string();
        assert!(error_msg.contains("Local file") && error_msg.contains("not found"));
        Ok(())
    }

    #[tokio::test]
    async fn test_install_resource_invalid_markdown_frontmatter() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let project_dir = temp_dir.path();
        let cache = Cache::with_dir(temp_dir.path().join("cache"))?;

        let local_file = temp_dir.path().join("invalid.md");
        std::fs::write(&local_file, "---\ninvalid: yaml: [\n---\nContent")?;

        let mut entry = create_test_locked_resource("invalid-test", true);
        entry.path = local_file.to_string_lossy().to_string();

        let context = InstallContext::builder(project_dir, &cache).build();

        let result = install_resource(&entry, "agents", &context).await;
        let (installed, _checksum, _context_checksum, _applied_patches) = result?;
        assert!(installed);

        let dest_path = project_dir.join("agents/invalid-test.md");
        assert!(dest_path.exists());

        let installed_content = std::fs::read_to_string(&dest_path)?;
        assert!(installed_content.contains("---"));
        assert!(installed_content.contains("invalid: yaml:"));
        assert!(installed_content.contains("Content"));
        Ok(())
    }

    #[tokio::test]
    async fn test_install_resource_with_progress() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let project_dir = temp_dir.path();
        let cache = Cache::with_dir(temp_dir.path().join("cache"))?;
        let pb = ProgressBar::new(1);

        let local_file = temp_dir.path().join("test.md");
        std::fs::write(&local_file, "# Progress Test")?;

        let mut entry = create_test_locked_resource("progress-test", true);
        entry.path = local_file.to_string_lossy().to_string();

        let context = InstallContext::builder(project_dir, &cache).build();

        let result = install_resource_with_progress(&entry, "agents", &context, &pb).await;
        let _ = result?;

        let expected_path = project_dir.join("agents").join("progress-test.md");
        assert!(expected_path.exists());
        Ok(())
    }

    #[tokio::test]
    async fn test_install_resources_empty() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let project_dir = temp_dir.path();
        let cache = Cache::with_dir(temp_dir.path().join("cache"))?;

        let lockfile = LockFile::new();
        let manifest = Manifest::new();

        let results = install_resources(
            ResourceFilter::All,
            &Arc::new(lockfile),
            &manifest,
            project_dir,
            cache,
            false,
            None,
            None,
            false,
            None,
            false, // don't trust checksums in tests
        )
        .await?;

        assert_eq!(results.installed_count, 0, "Should install 0 resources from empty lockfile");
        Ok(())
    }

    #[tokio::test]
    async fn test_install_resources_multiple() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let project_dir = temp_dir.path();
        let cache = Cache::with_dir(temp_dir.path().join("cache"))?;

        let file1 = temp_dir.path().join("agent.md");
        let file2 = temp_dir.path().join("snippet.md");
        let file3 = temp_dir.path().join("command.md");
        std::fs::write(&file1, "# Agent")?;
        std::fs::write(&file2, "# Snippet")?;
        std::fs::write(&file3, "# Command")?;

        let mut lockfile = LockFile::new();
        let mut agent = create_test_locked_resource("test-agent", true);
        agent.path = file1.to_string_lossy().to_string();
        agent.installed_at = ".claude/agents/test-agent.md".to_string();
        lockfile.agents.push(agent);

        let mut snippet = create_test_locked_resource("test-snippet", true);
        snippet.path = file2.to_string_lossy().to_string();
        snippet.resource_type = crate::core::ResourceType::Snippet;
        snippet.tool = Some("agpm".to_string());
        snippet.installed_at = ".agpm/snippets/test-snippet.md".to_string();
        lockfile.snippets.push(snippet);

        let mut command = create_test_locked_resource("test-command", true);
        command.path = file3.to_string_lossy().to_string();
        command.resource_type = crate::core::ResourceType::Command;
        command.installed_at = ".claude/commands/test-command.md".to_string();
        lockfile.commands.push(command);

        let manifest = Manifest::new();

        let results = install_resources(
            ResourceFilter::All,
            &Arc::new(lockfile),
            &manifest,
            project_dir,
            cache,
            false,
            None,
            None,
            false,
            None,
            false, // don't trust checksums in tests
        )
        .await?;

        assert_eq!(results.installed_count, 3, "Should install 3 resources");

        assert!(project_dir.join(".claude/agents/test-agent.md").exists());
        assert!(project_dir.join(".agpm/snippets/test-snippet.md").exists());
        assert!(project_dir.join(".claude/commands/test-command.md").exists());
        Ok(())
    }

    #[tokio::test]
    async fn test_install_updated_resources() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let project_dir = temp_dir.path();
        let cache = Cache::with_dir(temp_dir.path().join("cache"))?;

        let file1 = temp_dir.path().join("agent.md");
        let file2 = temp_dir.path().join("snippet.md");
        std::fs::write(&file1, "# Updated Agent")?;
        std::fs::write(&file2, "# Updated Snippet")?;

        let mut lockfile = LockFile::new();
        let mut agent = create_test_locked_resource("test-agent", true);
        agent.path = file1.to_string_lossy().to_string();
        lockfile.agents.push(agent);

        let mut snippet = create_test_locked_resource("test-snippet", true);
        snippet.path = file2.to_string_lossy().to_string();
        lockfile.snippets.push(snippet);

        let manifest = Manifest::new();
        let lockfile = Arc::new(lockfile);

        let updates =
            vec![("test-agent".to_string(), None, "v1.0.0".to_string(), "v1.1.0".to_string())];

        let context = InstallContext::builder(project_dir, &cache)
            .manifest(&manifest)
            .lockfile(&lockfile)
            .build();

        let count =
            install_updated_resources(&updates, &lockfile, &manifest, &context, None, false)
                .await?;

        assert_eq!(count, 1, "Should install 1 updated resource");
        assert!(project_dir.join(".claude/agents/test-agent.md").exists());
        assert!(!project_dir.join(".claude/snippets/test-snippet.md").exists());
        Ok(())
    }

    #[tokio::test]
    async fn test_install_updated_resources_quiet_mode() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let project_dir = temp_dir.path();
        let cache = Cache::with_dir(temp_dir.path().join("cache"))?;

        let file = temp_dir.path().join("command.md");
        std::fs::write(&file, "# Command")?;

        let mut lockfile = LockFile::new();
        let mut command = create_test_locked_resource("test-command", true);
        command.path = file.to_string_lossy().to_string();
        command.resource_type = crate::core::ResourceType::Command;
        lockfile.commands.push(command);

        let manifest = Manifest::new();
        let lockfile = Arc::new(lockfile);

        let updates =
            vec![("test-command".to_string(), None, "v1.0.0".to_string(), "v2.0.0".to_string())];

        let context = InstallContext::builder(project_dir, &cache)
            .manifest(&manifest)
            .lockfile(&lockfile)
            .build();

        let count =
            install_updated_resources(&updates, &lockfile, &manifest, &context, None, true).await?;

        assert_eq!(count, 1);
        assert!(project_dir.join(".claude/commands/test-command.md").exists());
        Ok(())
    }

    #[tokio::test]
    async fn test_install_resource_for_parallel() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let project_dir = temp_dir.path();
        let cache = Cache::with_dir(temp_dir.path().join("cache"))?;

        let local_file = temp_dir.path().join("parallel.md");
        std::fs::write(&local_file, "# Parallel Test")?;

        let mut entry = create_test_locked_resource("parallel-test", true);
        entry.path = local_file.to_string_lossy().to_string();

        let context = InstallContext::builder(project_dir, &cache).build();

        let result = install_resource(&entry, ".claude", &context).await;
        let _ = result?;

        let expected_path = project_dir.join(&entry.installed_at);
        assert!(expected_path.exists());
        Ok(())
    }

    #[tokio::test]
    async fn test_install_resource_creates_nested_directories() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let project_dir = temp_dir.path();
        let cache = Cache::with_dir(temp_dir.path().join("cache"))?;

        let local_file = temp_dir.path().join("nested.md");
        std::fs::write(&local_file, "# Nested Test")?;

        let mut entry = create_test_locked_resource("nested-test", true);
        entry.path = local_file.to_string_lossy().to_string();
        entry.installed_at = "very/deeply/nested/path/resource.md".to_string();

        let context = InstallContext::builder(project_dir, &cache).build();

        let result = install_resource(&entry, "agents", &context).await;
        let (installed, _checksum, _context_checksum, _applied_patches) = result?;
        assert!(installed, "Should have installed new resource");

        let expected_path = project_dir.join("very/deeply/nested/path/resource.md");
        assert!(expected_path.exists());
        Ok(())
    }

    #[tokio::test]
    async fn test_update_gitignore_creates_new_file() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let project_dir = temp_dir.path();

        let mut lockfile = LockFile::new();

        let mut agent = create_test_locked_resource("test-agent", true);
        agent.installed_at = ".claude/agents/test-agent.md".to_string();
        lockfile.agents.push(agent);

        let mut snippet = create_test_locked_resource("test-snippet", true);
        snippet.installed_at = ".agpm/snippets/test-snippet.md".to_string();
        lockfile.snippets.push(snippet);

        let result = update_gitignore(&lockfile, project_dir, true);
        result?;

        let gitignore_path = project_dir.join(".gitignore");
        assert!(gitignore_path.exists(), "Gitignore file should be created");

        let content = std::fs::read_to_string(&gitignore_path)?;
        assert!(content.contains("AGPM managed entries"));
        assert!(content.contains(".claude/agents/test-agent.md"));
        assert!(content.contains(".agpm/snippets/test-snippet.md"));
        Ok(())
    }

    #[tokio::test]
    async fn test_update_gitignore_disabled() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let project_dir = temp_dir.path();

        let lockfile = LockFile::new();

        let result = update_gitignore(&lockfile, project_dir, false);
        result?;

        let gitignore_path = project_dir.join(".gitignore");
        assert!(!gitignore_path.exists(), "Gitignore should not be created when disabled");
        Ok(())
    }

    #[tokio::test]
    async fn test_update_gitignore_preserves_user_entries() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let project_dir = temp_dir.path();

        let claude_dir = project_dir.join(".claude");
        ensure_dir(&claude_dir)?;

        let gitignore_path = project_dir.join(".gitignore");
        let existing_content = "# User comment\n\
                               user-file.txt\n\
                               # AGPM managed entries - do not edit below this line\n\
                               .claude/agents/old-entry.md\n\
                               # End of AGPM managed entries\n";
        std::fs::write(&gitignore_path, existing_content)?;

        let mut lockfile = LockFile::new();
        let mut agent = create_test_locked_resource("new-agent", true);
        agent.installed_at = ".claude/agents/new-agent.md".to_string();
        lockfile.agents.push(agent);

        let result = update_gitignore(&lockfile, project_dir, true);
        result?;

        let updated_content = std::fs::read_to_string(&gitignore_path)?;
        assert!(updated_content.contains("user-file.txt"));
        assert!(updated_content.contains("# User comment"));

        assert!(updated_content.contains(".claude/agents/new-agent.md"));

        assert!(!updated_content.contains(".claude/agents/old-entry.md"));
        Ok(())
    }

    #[tokio::test]
    async fn test_update_gitignore_handles_external_paths() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let project_dir = temp_dir.path();

        let mut lockfile = LockFile::new();

        let mut script = create_test_locked_resource("test-script", true);
        script.installed_at = "scripts/test.sh".to_string();
        lockfile.scripts.push(script);

        let mut agent = create_test_locked_resource("test-agent", true);
        agent.installed_at = ".claude/agents/test.md".to_string();
        lockfile.agents.push(agent);

        let result = update_gitignore(&lockfile, project_dir, true);
        result?;

        let gitignore_path = project_dir.join(".gitignore");
        let content = std::fs::read_to_string(&gitignore_path)?;

        assert!(content.contains("scripts/test.sh"));
        assert!(content.contains(".claude/agents/test.md"));
        Ok(())
    }

    #[tokio::test]
    async fn test_update_gitignore_migrates_ccpm_entries() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let project_dir = temp_dir.path();

        tokio::fs::create_dir_all(project_dir.join(".claude/agents")).await?;

        let gitignore_path = project_dir.join(".gitignore");
        let legacy_content = r#"# User's custom entries
temp/

# CCPM managed entries - do not edit below this line
.claude/agents/old-ccpm-agent.md
.claude/commands/old-ccpm-command.md
# End of CCPM managed entries

# More user entries
local-config.json
"#;
        tokio::fs::write(&gitignore_path, legacy_content).await?;

        let mut lockfile = LockFile::new();
        let mut agent = create_test_locked_resource("new-agent", true);
        agent.installed_at = ".claude/agents/new-agent.md".to_string();
        lockfile.agents.push(agent);

        let result = update_gitignore(&lockfile, project_dir, true);
        result?;

        let updated_content = tokio::fs::read_to_string(&gitignore_path).await?;

        assert!(updated_content.contains("temp/"));
        assert!(updated_content.contains("local-config.json"));

        assert!(updated_content.contains("# AGPM managed entries - do not edit below this line"));
        assert!(updated_content.contains("# End of AGPM managed entries"));

        assert!(!updated_content.contains("# CCPM managed entries"));
        assert!(!updated_content.contains("# End of CCPM managed entries"));

        assert!(!updated_content.contains("old-ccpm-agent.md"));
        assert!(!updated_content.contains("old-ccpm-command.md"));

        assert!(updated_content.contains(".claude/agents/new-agent.md"));
        Ok(())
    }

    #[tokio::test]
    async fn test_install_updated_resources_not_found() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let project_dir = temp_dir.path();
        let cache = Cache::with_dir(temp_dir.path().join("cache"))?;

        let lockfile = Arc::new(LockFile::new());
        let manifest = Manifest::new();

        let updates =
            vec![("non-existent".to_string(), None, "v1.0.0".to_string(), "v2.0.0".to_string())];

        let context = InstallContext::builder(project_dir, &cache)
            .manifest(&manifest)
            .lockfile(&lockfile)
            .build();

        let count =
            install_updated_resources(&updates, &lockfile, &manifest, &context, None, false)
                .await?;

        assert_eq!(count, 0, "Should install 0 resources when not found");
        Ok(())
    }

    #[tokio::test]
    async fn test_local_dependency_change_detection() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let project_dir = temp_dir.path();
        let cache = Cache::with_dir(temp_dir.path().join("cache"))?;

        let local_file = temp_dir.path().join("test.md");
        std::fs::write(&local_file, "# Test Resource\nOriginal content")?;

        let mut entry = create_test_locked_resource("local-change-test", true);
        entry.path = local_file.to_string_lossy().to_string();
        entry.installed_at = "agents/local-change-test.md".to_string();

        let context = InstallContext::builder(project_dir, &cache).build();

        let result = install_resource(&entry, "agents", &context).await;
        assert!(result.is_ok(), "Failed initial install: {:?}", result);
        let (installed, checksum1, _, _) = result?;
        assert!(installed, "Should have installed new resource");

        let installed_path = project_dir.join("agents/local-change-test.md");
        assert!(installed_path.exists(), "Installed file not found");
        let content1 = std::fs::read_to_string(&installed_path)?;
        assert_eq!(content1, "# Test Resource\nOriginal content");

        std::fs::write(&local_file, "# Test Resource\nModified content")?;

        let mut old_entry = entry.clone();
        old_entry.checksum = checksum1.clone();

        let mut old_lockfile = LockFile::default();
        old_lockfile.agents.push(old_entry);

        let context_with_old =
            InstallContext::builder(project_dir, &cache).old_lockfile(&old_lockfile).build();

        let result = install_resource(&entry, "agents", &context_with_old).await;
        assert!(result.is_ok(), "Failed second install: {:?}", result);
        let (reinstalled, checksum2, _, _) = result?;

        assert!(reinstalled, "Should have detected local file change and reinstalled");

        assert_ne!(checksum1, checksum2, "Checksum should change when content changes");

        let content2 = std::fs::read_to_string(&installed_path)?;
        assert_eq!(content2, "# Test Resource\nModified content");
        Ok(())
    }

    #[tokio::test]
    async fn test_git_dependency_early_exit_still_works() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let project_dir = temp_dir.path();
        let cache = Cache::with_dir(temp_dir.path().join("cache"))?;

        let mut entry = create_test_locked_resource("git-test", false);
        entry.resolved_commit = Some("a".repeat(40));
        entry.checksum = "sha256:test123".to_string();
        entry.installed_at = "agents/git-test.md".to_string();

        let installed_path = project_dir.join("agents/git-test.md");
        let parent_dir = installed_path
            .parent()
            .ok_or_else(|| anyhow::anyhow!("File has no parent directory"))?;
        ensure_dir(parent_dir)?;
        std::fs::write(&installed_path, "# Git Resource\nContent")?;

        let mut old_lockfile = LockFile::default();
        old_lockfile.agents.push(entry.clone());

        let _context =
            InstallContext::builder(project_dir, &cache).old_lockfile(&old_lockfile).build();

        Ok(())
    }

    #[tokio::test]
    async fn test_ensure_gitignore_state() -> Result<()> {
        use crate::installer::gitignore::ensure_gitignore_state;

        let temp_dir = TempDir::new()?;
        let project_dir = temp_dir.path();

        let manifest = Manifest::default();
        let lockfile = LockFile::default();

        let result = ensure_gitignore_state(&manifest, &lockfile, project_dir).await;
        result?;
        Ok(())
    }

    #[tokio::test]
    async fn test_update_gitignore() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let project_dir = temp_dir.path();
        let lockfile = LockFile::default();

        let result = update_gitignore(&lockfile, project_dir, true);
        result?;

        assert!(project_dir.join(".gitignore").exists());
        Ok(())
    }

    #[tokio::test]
    async fn test_cleanup_gitignore() -> Result<()> {
        use crate::installer::gitignore::cleanup_gitignore;

        let temp_dir = TempDir::new()?;
        let project_dir = temp_dir.path();

        let gitignore_content = r#"
# User content
*.log

# AGPM managed entries - do not edit below this line
.claude/agents/test.md
# End of AGPM managed entries
"#;
        std::fs::write(project_dir.join(".gitignore"), gitignore_content)?;

        let result = cleanup_gitignore(project_dir).await;
        result?;

        let content = std::fs::read_to_string(project_dir.join(".gitignore"))?;
        assert!(content.contains("*.log"));
        assert!(!content.contains("AGPM managed entries"));
        Ok(())
    }

    #[tokio::test]
    async fn test_install_context_builder_common_options() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let cache = Cache::with_dir(temp_dir.path().join("cache"))?;
        let manifest = Manifest::default();
        let lockfile = Arc::new(LockFile::default());

        let context = InstallContext::with_common_options(
            temp_dir.path(),
            &cache,
            Some(&manifest),
            Some(&lockfile),
            false,
            false,
            None,
        );

        assert!(context.manifest.is_some());
        assert!(context.lockfile.is_some());
        Ok(())
    }
}

#[cfg(test)]
mod should_skip_trusted_tests {
    use crate::cache::Cache;
    use crate::installer::{InstallContext, resource::should_skip_trusted};
    use crate::lockfile::{LockFile, LockedResource};
    use crate::resolver::lockfile_builder::VariantInputs;

    use anyhow::Result;
    use std::collections::BTreeMap;
    use tempfile::TempDir;

    fn create_git_locked_resource(name: &str) -> LockedResource {
        LockedResource {
            name: name.to_string(),
            source: Some("test_source".to_string()),
            url: Some("https://github.com/test/repo.git".to_string()),
            version: Some("v1.0.0".to_string()),
            path: format!("{}.md", name),
            resolved_commit: Some("a".repeat(40)),
            checksum: "sha256:abc123".to_string(),
            context_checksum: Some("sha256:ctx456".to_string()),
            installed_at: format!(".claude/agents/{}.md", name),
            dependencies: vec![],
            resource_type: crate::core::ResourceType::Agent,
            tool: None,
            manifest_alias: None,
            applied_patches: BTreeMap::new(),
            install: None,
            variant_inputs: VariantInputs::default(),
        }
    }

    fn create_local_locked_resource(name: &str) -> LockedResource {
        LockedResource {
            name: name.to_string(),
            source: None,
            url: None,
            version: None,
            path: format!("{}.md", name),
            resolved_commit: None, // Local deps have no resolved_commit
            checksum: "sha256:abc123".to_string(),
            context_checksum: None,
            installed_at: format!(".claude/agents/{}.md", name),
            dependencies: vec![],
            resource_type: crate::core::ResourceType::Agent,
            tool: None,
            manifest_alias: None,
            applied_patches: BTreeMap::new(),
            install: None,
            variant_inputs: VariantInputs::default(),
        }
    }

    #[test]
    fn test_skip_trusted_disabled_mode() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let cache = Cache::with_dir(temp_dir.path().join("cache"))?;

        let entry = create_git_locked_resource("test-agent");
        let dest_path = temp_dir.path().join(".claude/agents/test-agent.md");

        // trust_lockfile_checksums is false by default
        let context = InstallContext::builder(temp_dir.path(), &cache).build();

        let result = should_skip_trusted(&entry, &dest_path, &context);
        assert!(result.is_none(), "Should return None when trust mode is disabled");
        Ok(())
    }

    #[test]
    fn test_skip_trusted_local_dependency() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let cache = Cache::with_dir(temp_dir.path().join("cache"))?;

        let entry = create_local_locked_resource("local-test");
        let dest_path = temp_dir.path().join(".claude/agents/local-test.md");

        let mut old_lockfile = LockFile::default();
        old_lockfile.agents.push(entry.clone());

        let context = InstallContext::builder(temp_dir.path(), &cache)
            .trust_lockfile_checksums(true)
            .old_lockfile(&old_lockfile)
            .build();

        let result = should_skip_trusted(&entry, &dest_path, &context);
        assert!(
            result.is_none(),
            "Should return None for local dependencies (they can change anytime)"
        );
        Ok(())
    }

    #[test]
    fn test_skip_trusted_missing_old_lockfile() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let cache = Cache::with_dir(temp_dir.path().join("cache"))?;

        let entry = create_git_locked_resource("test-agent");
        let dest_path = temp_dir.path().join(".claude/agents/test-agent.md");

        // No old lockfile provided
        let context =
            InstallContext::builder(temp_dir.path(), &cache).trust_lockfile_checksums(true).build();

        let result = should_skip_trusted(&entry, &dest_path, &context);
        assert!(result.is_none(), "Should return None when no old lockfile is available");
        Ok(())
    }

    #[test]
    fn test_skip_trusted_missing_file() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let cache = Cache::with_dir(temp_dir.path().join("cache"))?;

        let entry = create_git_locked_resource("test-agent");
        let dest_path = temp_dir.path().join(".claude/agents/test-agent.md");
        // Note: dest_path does NOT exist

        let mut old_lockfile = LockFile::default();
        old_lockfile.agents.push(entry.clone());

        let context = InstallContext::builder(temp_dir.path(), &cache)
            .trust_lockfile_checksums(true)
            .old_lockfile(&old_lockfile)
            .build();

        let result = should_skip_trusted(&entry, &dest_path, &context);
        assert!(result.is_none(), "Should return None when destination file is missing");
        Ok(())
    }

    #[test]
    fn test_skip_trusted_changed_commit() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let cache = Cache::with_dir(temp_dir.path().join("cache"))?;

        let mut entry = create_git_locked_resource("test-agent");
        entry.resolved_commit = Some("b".repeat(40)); // Different commit

        let mut old_entry = create_git_locked_resource("test-agent");
        old_entry.resolved_commit = Some("a".repeat(40)); // Original commit

        let dest_path = temp_dir.path().join(".claude/agents/test-agent.md");
        std::fs::create_dir_all(dest_path.parent().unwrap())?;
        std::fs::write(&dest_path, "# Test")?;

        let mut old_lockfile = LockFile::default();
        old_lockfile.agents.push(old_entry);

        let context = InstallContext::builder(temp_dir.path(), &cache)
            .trust_lockfile_checksums(true)
            .old_lockfile(&old_lockfile)
            .build();

        let result = should_skip_trusted(&entry, &dest_path, &context);
        assert!(result.is_none(), "Should return None when resolved_commit changed");
        Ok(())
    }

    #[test]
    fn test_skip_trusted_install_false_requires_verification() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let cache = Cache::with_dir(temp_dir.path().join("cache"))?;

        let mut entry = create_git_locked_resource("content-only");
        entry.install = Some(false); // Content-only, not installed to disk

        let mut old_lockfile = LockFile::default();
        old_lockfile.agents.push(entry.clone());

        let dest_path = temp_dir.path().join(".claude/agents/content-only.md");
        // Note: file doesn't exist (and shouldn't need to for install=false)

        let context = InstallContext::builder(temp_dir.path(), &cache)
            .trust_lockfile_checksums(true)
            .old_lockfile(&old_lockfile)
            .build();

        let result = should_skip_trusted(&entry, &dest_path, &context);
        // install=false resources must still go through checksum verification
        // to catch force-pushed tags. They should NOT use the trusted path.
        assert!(
            result.is_none(),
            "install=false resources should NOT use trusted path - must verify checksums"
        );
        Ok(())
    }

    #[test]
    fn test_skip_trusted_all_conditions_met() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let cache = Cache::with_dir(temp_dir.path().join("cache"))?;

        let entry = create_git_locked_resource("test-agent");
        let dest_path = temp_dir.path().join(".claude/agents/test-agent.md");

        // Create the file at expected location
        std::fs::create_dir_all(dest_path.parent().unwrap())?;
        std::fs::write(&dest_path, "# Test Agent\nContent")?;

        let mut old_lockfile = LockFile::default();
        old_lockfile.agents.push(entry.clone());

        let context = InstallContext::builder(temp_dir.path(), &cache)
            .trust_lockfile_checksums(true)
            .old_lockfile(&old_lockfile)
            .build();

        let result = should_skip_trusted(&entry, &dest_path, &context);
        assert!(result.is_some(), "Should return Some when all conditions are met");

        let (actually_installed, checksum, context_checksum, _patches) = result.unwrap();
        assert!(!actually_installed, "Should report as not installed (skipped)");
        assert_eq!(checksum, "sha256:abc123");
        assert_eq!(context_checksum, Some("sha256:ctx456".to_string()));
        Ok(())
    }

    #[test]
    fn test_skip_trusted_force_refresh() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let cache = Cache::with_dir(temp_dir.path().join("cache"))?;

        let entry = create_git_locked_resource("test-agent");
        let dest_path = temp_dir.path().join(".claude/agents/test-agent.md");

        std::fs::create_dir_all(dest_path.parent().unwrap())?;
        std::fs::write(&dest_path, "# Test Agent")?;

        let mut old_lockfile = LockFile::default();
        old_lockfile.agents.push(entry.clone());

        let context = InstallContext::builder(temp_dir.path(), &cache)
            .trust_lockfile_checksums(true)
            .force_refresh(true) // Force refresh enabled
            .old_lockfile(&old_lockfile)
            .build();

        let result = should_skip_trusted(&entry, &dest_path, &context);
        assert!(result.is_none(), "Should return None when force_refresh is enabled");
        Ok(())
    }
}
