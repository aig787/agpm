//! Tests for the resolver module.

use super::*;
use crate::manifest::DetailedDependency;
use crate::resolver::lockfile_builder::VariantInputs;
use tempfile::TempDir;

#[tokio::test]
async fn resolver_new() -> Result<(), Box<dyn std::error::Error>> {
    let manifest = Manifest::new();
    let temp_dir = TempDir::new()?;
    let cache = Cache::with_dir(temp_dir.path().to_path_buf())?;
    let resolver = DependencyResolver::with_cache(manifest, cache).await?;

    // Check if we can create a resolver successfully
    assert!(resolver.core.manifest.manifest_dir.is_some());
    Ok(())
}

#[tokio::test]
async fn resolve_local_dependency() -> Result<(), Box<dyn std::error::Error>> {
    let temp_dir = TempDir::new()?;
    let mut manifest = Manifest::new();
    manifest.manifest_dir = Some(temp_dir.path().to_path_buf());
    manifest.add_dependency(
        "local-agent".to_string(),
        ResourceDependency::Simple("../agents/local.md".to_string()),
        true,
    );

    // Create dummy file to allow transitive dependency extraction
    let agents_dir = temp_dir.path().parent().ok_or("No parent directory")?.join("agents");
    std::fs::create_dir_all(&agents_dir)?;
    std::fs::write(agents_dir.join("local.md"), "# Local Agent")?;

    let cache = Cache::with_dir(temp_dir.path().to_path_buf())?;
    let mut resolver = DependencyResolver::with_cache(manifest, cache);

    let lockfile = resolver.resolve().await?;
    assert_eq!(lockfile.agents.len(), 1);

    let entry = &lockfile.agents[0];
    assert_eq!(entry.name, "local-agent");
    assert_eq!(entry.path, "../agents/local.md");
    assert!(entry.source.is_none());
    assert!(entry.url.is_none());
    Ok(())
}

#[tokio::test]
async fn pre_sync_sources() -> Result<(), Box<dyn std::error::Error>> {
    // Skip test if git is not available
    if std::process::Command::new("git").arg("--version").output().is_err() {
        eprintln!("Skipping test: git not available");
        return Ok(());
    }

    // Create a test Git repository with resources
    let temp_dir = TempDir::new()?;
    let repo_dir = temp_dir.path().join("test-repo");
    std::fs::create_dir(&repo_dir)?;

    // Initialize git repo
    let output = std::process::Command::new("git").args(["init"]).current_dir(&repo_dir).output()?;
    if !output.status.success() {
        return Err(format!("git init failed: {}", String::from_utf8_lossy(&output.stderr)).into());
    }

    let output = std::process::Command::new("git")
        .args(["config", "user.email", "test@example.com"])
        .current_dir(&repo_dir)
        .output()?;
    if !output.status.success() {
        return Err(format!("git config email failed: {}", String::from_utf8_lossy(&output.stderr)).into());
    }

    let output = std::process::Command::new("git")
        .args(["config", "user.name", "Test User"])
        .current_dir(&repo_dir)
        .output()?;
    if !output.status.success() {
        return Err(format!("git config name failed: {}", String::from_utf8_lossy(&output.stderr)).into());
    }

    // Create test files
    std::fs::create_dir_all(repo_dir.join("agents"))?;
    std::fs::write(repo_dir.join("agents/test.md"), "# Test Agent\n\nTest content")?;

    // Commit files
    let output = std::process::Command::new("git").args(["add", "."]).current_dir(&repo_dir).output()?;
    if !output.status.success() {
        return Err(format!("git add failed: {}", String::from_utf8_lossy(&output.stderr)).into());
    }

    let output = std::process::Command::new("git")
        .args(["commit", "-m", "Initial commit"])
        .current_dir(&repo_dir)
        .output()?;
    if !output.status.success() {
        return Err(format!("git commit failed: {}", String::from_utf8_lossy(&output.stderr)).into());
    }

    let output = std::process::Command::new("git")
        .args(["tag", "v1.0.0"])
        .current_dir(&repo_dir)
        .output()?;
    if !output.status.success() {
        return Err(format!("git tag failed: {}", String::from_utf8_lossy(&output.stderr)).into());
    }

    // Create a manifest with a dependency from this source
    let mut manifest = Manifest::new();
    let source_url = format!("file://{}", repo_dir.display());
    manifest.add_source("test-source".to_string(), source_url.clone());

    manifest.add_dependency(
        "test-agent".to_string(),
        ResourceDependency::Detailed(Box::new(DetailedDependency {
            source: Some("test-source".to_string()),
            path: "agents/test.md".to_string(),
            version: Some("v1.0.0".to_string()),
            branch: None,
            rev: None,
            command: None,
            args: None,
            target: None,
            filename: None,
            dependencies: None,
            tool: None,
            flatten: None,
            install: None,
            template_vars: Some(serde_json::Value::Object(serde_json::Map::new())),
        })),
        true,
    );

    let cache = Cache::with_dir(temp_dir.path().to_path_buf())?;
    let mut resolver = DependencyResolver::with_cache(manifest, cache);

    // Get dependencies for pre-sync
    let deps: Vec<(String, ResourceDependency)> = resolver
        .manifest
        .all_dependencies()
        .into_iter()
        .map(|(name, dep)| (name.to_string(), dep.clone()))
        .collect();

    // Pre-sync sources
    resolver.pre_sync_sources(&deps, None).await?;

    // Verify that the source was synced
    assert!(resolver.version_service.has_entries());
    Ok(())
}

#[tokio::test]
async fn resolve_with_transitive_dependencies() -> Result<(), Box<dyn std::error::Error>> {
    let temp_dir = TempDir::new()?;
    let mut manifest = Manifest::new();
    manifest.manifest_dir = Some(temp_dir.path().to_path_buf());

    // Create a local dependency with transitive dependencies
    let agents_dir = temp_dir.path().join("agents");
    std::fs::create_dir_all(&agents_dir)?;

    // Create a parent agent that depends on a helper
    let parent_content = r#"---
dependencies:
  agents:
    - path: "helper.md"
---
# Parent Agent

This agent depends on helper.md.
"#;
    std::fs::write(agents_dir.join("parent.md"), parent_content)?;

    // Create the helper agent
    std::fs::write(agents_dir.join("helper.md"), "# Helper Agent\n\nHelper content")?;

    manifest.add_dependency(
        "parent".to_string(),
        ResourceDependency::Simple("agents/parent.md".to_string()),
        true,
    );

    let cache = Cache::with_dir(temp_dir.path().to_path_buf())?;
    let mut resolver = DependencyResolver::with_cache(manifest, cache);

    let lockfile = resolver.resolve().await?;

    // Should have both parent and helper agents
    assert_eq!(lockfile.agents.len(), 2);

    let parent_entry = lockfile.agents.iter().find(|e| e.name == "parent").ok_or("Parent entry not found")?;
    let helper_entry = lockfile.agents.iter().find(|e| e.name == "helper").ok_or("Helper entry not found")?;

    assert_eq!(parent_entry.name, "parent");
    assert_eq!(helper_entry.name, "helper");

    // Parent should depend on helper
    assert!(parent_entry.dependencies.contains(&"agent/helper".to_string()));
    Ok(())
}

#[tokio::test]
async fn pattern_expansion() -> Result<(), Box<dyn std::error::Error>> {
    let temp_dir = TempDir::new()?;
    let mut manifest = Manifest::new();
    manifest.manifest_dir = Some(temp_dir.path().to_path_buf());

    // Create multiple agents in a directory
    let agents_dir = temp_dir.path().join("agents");
    std::fs::create_dir_all(&agents_dir)?;

    std::fs::write(agents_dir.join("agent1.md"), "# Agent 1")?;
    std::fs::write(agents_dir.join("agent2.md"), "# Agent 2")?;
    std::fs::write(agents_dir.join("agent3.md"), "# Agent 3")?;

    // Add a pattern dependency
    manifest.add_dependency(
        "all-agents".to_string(),
        ResourceDependency::Simple("agents/*.md".to_string()),
        true,
    );

    let cache = Cache::with_dir(temp_dir.path().to_path_buf())?;
    let mut resolver = DependencyResolver::with_cache(manifest, cache);

    let lockfile = resolver.resolve().await?;

    // Should have all three agents
    assert_eq!(lockfile.agents.len(), 3);

    let agent_names: std::collections::HashSet<_> =
        lockfile.agents.iter().map(|e| e.name.as_str()).collect();

    assert!(agent_names.contains("agent1"));
    assert!(agent_names.contains("agent2"));
    assert!(agent_names.contains("agent3"));

    // All should have the same pattern alias
    for agent in &lockfile.agents {
        assert_eq!(agent.manifest_alias.as_deref(), Some("all-agents"));
    }
    Ok(())
}

#[tokio::test]
async fn conflict_detection() -> Result<(), Box<dyn std::error::Error>> {
    let temp_dir = TempDir::new()?;
    let mut manifest = Manifest::new();
    manifest.manifest_dir = Some(temp_dir.path().to_path_buf());

    // Create two dependencies that would conflict (same path)
    let agents_dir = temp_dir.path().join("agents");
    std::fs::create_dir_all(&agents_dir)?;

    std::fs::write(agents_dir.join("conflict.md"), "# Conflict Agent")?;

    // Add two dependencies with different names but same path
    manifest.add_dependency(
        "agent-a".to_string(),
        ResourceDependency::Simple("agents/conflict.md".to_string()),
        true,
    );

    manifest.add_dependency(
        "agent-b".to_string(),
        ResourceDependency::Simple("agents/conflict.md".to_string()),
        true,
    );

    let cache = Cache::with_dir(temp_dir.path().to_path_buf())?;
    let mut resolver = DependencyResolver::with_cache(manifest, cache);

    let result = resolver.resolve().await;

    // Should detect the conflict
    assert!(result.is_err());
    let error_msg = result.unwrap_err().to_string();
    assert!(error_msg.contains("conflict") || error_msg.contains("same path"));
    Ok(())
}

#[test]
fn extract_meaningful_path() -> Result<(), Box<dyn std::error::Error>> {
    use std::path::Path;

    // Test relative paths with parent navigation
    assert_eq!(
        extract_meaningful_path(Path::new("../../snippets/dir/file.md")),
        "snippets/dir/file.md"
    );

    // Test clean relative paths
    assert_eq!(extract_meaningful_path(Path::new("agents/test.md")), "agents/test.md");

    // Test absolute paths - Unix
    #[cfg(unix)]
    assert_eq!(extract_meaningful_path(Path::new("/tmp/foo/../bar/agent.md")), "tmp/bar/agent.md");

    // Test absolute paths - Windows
    #[cfg(windows)]
    assert_eq!(
        extract_meaningful_path(Path::new("C:\\tmp\\foo\\..\\bar\\agent.md")),
        "tmp/bar/agent.md"
    );
    Ok(())
}

#[tokio::test]
async fn update_specific_dependency() -> Result<(), Box<dyn std::error::Error>> {
    let temp_dir = TempDir::new()?;
    let mut manifest = Manifest::new();
    manifest.manifest_dir = Some(temp_dir.path().to_path_buf());

    // Create initial lockfile
    let mut lockfile = LockFile::default();

    // Add an existing entry
    lockfile.agents.push(LockedResource {
        name: "test-agent".to_string(),
        source: None,
        url: None,
        path: "agents/test-agent.md".to_string(),
        version: None,
        resolved_commit: Some("old-commit".to_string()),
        checksum: "old-checksum".to_string(),
        installed_at: ".claude/agents/test-agent.md".to_string(),
        dependencies: vec![],
        resource_type: ResourceType::Agent,
        tool: Some("claude-code".to_string()),
        manifest_alias: None,
        context_checksum: None,
        applied_patches: std::collections::BTreeMap::new(),
        install: None,
        variant_inputs: VariantInputs::default(),
    });

    // Create the agent file
    let agents_dir = temp_dir.path().join("agents");
    std::fs::create_dir_all(&agents_dir)?;
    std::fs::write(agents_dir.join("test-agent.md"), "# Test Agent")?;

    manifest.add_dependency(
        "test-agent".to_string(),
        ResourceDependency::Simple("agents/test-agent.md".to_string()),
        true,
    );

    let cache = Cache::with_dir(temp_dir.path().to_path_buf())?;
    let mut resolver = DependencyResolver::with_cache(manifest, cache);

    // Update the specific dependency
    let lockfile = resolver.update(&lockfile, Some(vec!["test-agent".to_string()]), None).await?;

    // Verify the entry was updated
    assert_eq!(lockfile.agents.len(), 1);
    let entry = &lockfile.agents[0];
    assert_eq!(entry.name, "test-agent");
    assert_ne!(entry.resolved_commit, Some("old-commit".to_string()));
    assert_ne!(entry.checksum, "old-checksum");
    Ok(())
}

#[tokio::test]
async fn version_constraint_resolution() -> Result<(), Box<dyn std::error::Error>> {
    // Skip test if git is not available
    if std::process::Command::new("git").arg("--version").output().is_err() {
        eprintln!("Skipping test: git not available");
        return Ok(());
    }

    let temp_dir = TempDir::new()?;
    let repo_dir = temp_dir.path().join("test-repo");
    std::fs::create_dir(&repo_dir)?;

    // Initialize git repo
    let output = std::process::Command::new("git").args(["init"]).current_dir(&repo_dir).output()?;
    if !output.status.success() {
        return Err(format!("git init failed: {}", String::from_utf8_lossy(&output.stderr)).into());
    }

    let output = std::process::Command::new("git")
        .args(["config", "user.email", "test@example.com"])
        .current_dir(&repo_dir)
        .output()?;
    if !output.status.success() {
        return Err(format!("git config email failed: {}", String::from_utf8_lossy(&output.stderr)).into());
    }

    let output = std::process::Command::new("git")
        .args(["config", "user.name", "Test User"])
        .current_dir(&repo_dir)
        .output()?;
    if !output.status.success() {
        return Err(format!("git config name failed: {}", String::from_utf8_lossy(&output.stderr)).into());
    }

    // Create test files
    std::fs::create_dir_all(repo_dir.join("agents"))?;
    std::fs::write(repo_dir.join("agents/test.md"), "# Test Agent\n\nTest content")?;

    // Create multiple versions
    let output = std::process::Command::new("git").args(["add", "."]).current_dir(&repo_dir).output()?;
    if !output.status.success() {
        return Err(format!("git add failed: {}", String::from_utf8_lossy(&output.stderr)).into());
    }

    let output = std::process::Command::new("git")
        .args(["commit", "-m", "Initial commit"])
        .current_dir(&repo_dir)
        .output()?;
    if !output.status.success() {
        return Err(format!("git commit failed: {}", String::from_utf8_lossy(&output.stderr)).into());
    }

    let output = std::process::Command::new("git")
        .args(["tag", "v1.0.0"])
        .current_dir(&repo_dir)
        .output()?;
    if !output.status.success() {
        return Err(format!("git tag v1.0.0 failed: {}", String::from_utf8_lossy(&output.stderr)).into());
    }

    // Update file and create v2.0.0
    std::fs::write(repo_dir.join("agents/test.md"), "# Test Agent v2\n\nUpdated content")?;

    let output = std::process::Command::new("git").args(["add", "."]).current_dir(&repo_dir).output()?;
    if !output.status.success() {
        return Err(format!("git add failed: {}", String::from_utf8_lossy(&output.stderr)).into());
    }

    let output = std::process::Command::new("git")
        .args(["commit", "-m", "Update to v2"])
        .current_dir(&repo_dir)
        .output()?;
    if !output.status.success() {
        return Err(format!("git commit failed: {}", String::from_utf8_lossy(&output.stderr)).into());
    }

    let output = std::process::Command::new("git")
        .args(["tag", "v2.0.0"])
        .current_dir(&repo_dir)
        .output()?;
    if !output.status.success() {
        return Err(format!("git tag v2.0.0 failed: {}", String::from_utf8_lossy(&output.stderr)).into());
    }

    // Create manifest with version constraint
    let mut manifest = Manifest::new();
    let source_url = format!("file://{}", repo_dir.display());
    manifest.add_source("test-source".to_string(), source_url.clone());

    manifest.add_dependency(
        "test-agent".to_string(),
        ResourceDependency::Detailed(Box::new(DetailedDependency {
            source: Some("test-source".to_string()),
            path: "agents/test.md".to_string(),
            version: Some("^1.0.0".to_string()), // Should match v1.0.0 but not v2.0.0
            branch: None,
            rev: None,
            command: None,
            args: None,
            target: None,
            filename: None,
            dependencies: None,
            tool: None,
            flatten: None,
            install: None,
            template_vars: Some(serde_json::Value::Object(serde_json::Map::new())),
        })),
        true,
    );

    let cache = Cache::with_dir(temp_dir.path().to_path_buf())?;
    let mut resolver = DependencyResolver::with_cache(manifest, cache);

    let lockfile = resolver.resolve().await?;

    assert_eq!(lockfile.agents.len(), 1);
    let entry = &lockfile.agents[0];
    assert_eq!(entry.name, "test-agent");
    // Should resolve to v1.0.0 due to ^1.0.0 constraint
    // Lockfile stores RESOLVED version, not constraint (like Cargo.lock)
    assert!(entry.resolved_commit.is_some());
    assert_eq!(entry.version.as_ref().ok_or("No version found")?, "v1.0.0");
    Ok(())
}

#[tokio::test]
async fn multi_tool_support() -> Result<(), Box<dyn std::error::Error>> {
    let temp_dir = TempDir::new()?;
    let mut manifest = Manifest::new();
    manifest.manifest_dir = Some(temp_dir.path().to_path_buf());

    // Create agents directory
    let agents_dir = temp_dir.path().join("agents");
    std::fs::create_dir_all(&agents_dir)?;

    std::fs::write(agents_dir.join("claude-agent.md"), "# Claude Agent")?;
    std::fs::write(agents_dir.join("opencode-agent.md"), "# OpenCode Agent")?;

    // Add dependencies for different tools
    manifest.add_dependency(
        "claude-agent".to_string(),
        ResourceDependency::Detailed(Box::new(DetailedDependency {
            source: None,
            path: "agents/claude-agent.md".to_string(),
            version: None,
            branch: None,
            rev: None,
            command: None,
            args: None,
            target: None,
            filename: None,
            dependencies: None,
            tool: Some("claude-code".to_string()),
            flatten: None,
            install: None,
            template_vars: Some(serde_json::Value::Object(serde_json::Map::new())),
        })),
        true,
    );

    manifest.add_dependency(
        "opencode-agent".to_string(),
        ResourceDependency::Detailed(Box::new(DetailedDependency {
            source: None,
            path: "agents/opencode-agent.md".to_string(),
            version: None,
            branch: None,
            rev: None,
            command: None,
            args: None,
            target: None,
            filename: None,
            dependencies: None,
            tool: Some("opencode".to_string()),
            flatten: None,
            install: None,
            template_vars: Some(serde_json::Value::Object(serde_json::Map::new())),
        })),
        true,
    );

    let cache = Cache::with_dir(temp_dir.path().to_path_buf())?;
    let mut resolver = DependencyResolver::with_cache(manifest, cache);

    let lockfile = resolver.resolve().await?;

    assert_eq!(lockfile.agents.len(), 2);

    let claude_entry = lockfile.agents.iter().find(|e| e.name == "claude-agent").ok_or("Claude entry not found")?;
    let opencode_entry = lockfile.agents.iter().find(|e| e.name == "opencode-agent").ok_or("OpenCode entry not found")?;

    assert_eq!(claude_entry.tool, Some("claude-code".to_string()));
    assert_eq!(opencode_entry.tool, Some("opencode".to_string()));

    // Should install to different tool-specific directories
    assert!(claude_entry.installed_at.contains(".claude/"));
    assert!(opencode_entry.installed_at.contains(".opencode/"));
    Ok(())
}

#[tokio::test]
async fn dependency_cycle_detection() -> Result<(), Box<dyn std::error::Error>> {
    let temp_dir = TempDir::new()?;
    let mut manifest = Manifest::new();
    manifest.manifest_dir = Some(temp_dir.path().to_path_buf());

    // Create agents directory
    let agents_dir = temp_dir.path().join("agents");
    std::fs::create_dir_all(&agents_dir)?;

    // Create agent A that depends on B
    let agent_a_content = r#"---
dependencies:
  agents:
    - path: "./agent-b.md"
---
# Agent A

Depends on Agent B.
"#;
    std::fs::write(agents_dir.join("agent-a.md"), agent_a_content)?;

    // Create agent B that depends on A (creating a cycle)
    let agent_b_content = r#"---
dependencies:
  agents:
    - path: "./agent-a.md"
---
# Agent B

Depends on Agent A (cycle!).
"#;
    std::fs::write(agents_dir.join("agent-b.md"), agent_b_content)?;

    manifest.add_dependency(
        "agent-a".to_string(),
        ResourceDependency::Simple("agents/agent-a.md".to_string()),
        true,
    );

    let cache = Cache::with_dir(temp_dir.path().to_path_buf())?;
    let mut resolver = DependencyResolver::with_cache(manifest, cache);

    let result = resolver.resolve().await;

    // Should detect the cycle
    assert!(result.is_err());
    let error_msg = result.unwrap_err().to_string();
    assert!(error_msg.contains("Circular dependency"));
    Ok(())
}

#[tokio::test]
async fn template_variable_inheritance() -> Result<(), Box<dyn std::error::Error>> {
    let temp_dir = TempDir::new()?;
    let mut manifest = Manifest::new();
    manifest.manifest_dir = Some(temp_dir.path().to_path_buf());

    // Create agents directory
    let agents_dir = temp_dir.path().join("agents");
    std::fs::create_dir_all(&agents_dir)?;

    std::fs::write(agents_dir.join("templated.md"), "# Templated Agent")?;

    // Add dependency with template variables
    use serde_json::json;
    manifest.add_dependency(
        "templated".to_string(),
        ResourceDependency::Detailed(Box::new(DetailedDependency {
            source: None,
            path: "agents/templated.md".to_string(),
            version: None,
            branch: None,
            rev: None,
            command: None,
            args: None,
            target: None,
            filename: None,
            dependencies: None,
            tool: None,
            flatten: None,
            install: None,
            template_vars: Some(json!({"local_var": "local_value"})),
        })),
        true,
    );

    let cache = Cache::with_dir(temp_dir.path().to_path_buf())?;
    let mut resolver = DependencyResolver::with_cache(manifest, cache);

    let lockfile = resolver.resolve().await?;

    assert_eq!(lockfile.agents.len(), 1);
    let entry = &lockfile.agents[0];

    // Should have template variables from dependency
    assert!(!entry.template_vars.is_empty());
    Ok(())
}

#[tokio::test]
async fn patch_application() -> Result<(), Box<dyn std::error::Error>> {
    let temp_dir = TempDir::new()?;
    let mut manifest = Manifest::new();
    manifest.manifest_dir = Some(temp_dir.path().to_path_buf());

    // Create agents directory
    let agents_dir = temp_dir.path().join("agents");
    std::fs::create_dir_all(&agents_dir)?;

    std::fs::write(agents_dir.join("patched.md"), "# Patched Agent")?;

    // Add dependency
    manifest.add_dependency(
        "patched".to_string(),
        ResourceDependency::Simple("agents/patched.md".to_string()),
        true,
    );

    // Add patches
    use crate::manifest::PatchData;
    let mut patch_data = PatchData::new();
    patch_data.insert("model".to_string(), toml::Value::String("claude-3-haiku".to_string()));
    patch_data.insert("temperature".to_string(), toml::Value::Float(0.8));
    manifest.patches.agents.insert("patched".to_string(), patch_data);

    let cache = Cache::with_dir(temp_dir.path().to_path_buf())?;
    let mut resolver = DependencyResolver::with_cache(manifest, cache);

    let lockfile = resolver.resolve().await?;

    assert_eq!(lockfile.agents.len(), 1);
    let entry = &lockfile.agents[0];

    // Should have the patches applied
    assert_eq!(entry.applied_patches.len(), 2);
    assert!(entry.applied_patches.contains_key("model"));
    assert!(entry.applied_patches.contains_key("temperature"));
    Ok(())
}
