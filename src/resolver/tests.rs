//! Tests for the resolver module.

use super::*;
use crate::manifest::DetailedDependency;
use crate::test_utils::compute_variant_inputs_hash;
use tempfile::TempDir;

#[test]
fn test_resolver_new() {
    let manifest = Manifest::new();
    let temp_dir = TempDir::new().unwrap();
    let cache = Cache::with_dir(temp_dir.path().to_path_buf()).unwrap();
    let resolver = DependencyResolver::with_cache(manifest, cache);

    assert_eq!(resolver.cache.get_cache_location(), temp_dir.path());
}

#[tokio::test]
async fn test_resolve_local_dependency() {
    let temp_dir = TempDir::new().unwrap();
    let mut manifest = Manifest::new();
    manifest.manifest_dir = Some(temp_dir.path().to_path_buf());
    manifest.add_dependency(
        "local-agent".to_string(),
        ResourceDependency::Simple("../agents/local.md".to_string()),
        true,
    );

    // Create dummy file to allow transitive dependency extraction
    let agents_dir = temp_dir.path().parent().unwrap().join("agents");
    std::fs::create_dir_all(&agents_dir).unwrap();
    std::fs::write(agents_dir.join("local.md"), "# Local Agent").unwrap();

    let cache = Cache::with_dir(temp_dir.path().to_path_buf()).unwrap();
    let mut resolver = DependencyResolver::with_cache(manifest, cache);

    let lockfile = resolver.resolve().await.unwrap();
    assert_eq!(lockfile.agents.len(), 1);

    let entry = &lockfile.agents[0];
    assert_eq!(entry.name, "local-agent");
    assert_eq!(entry.path, "../agents/local.md");
    assert!(entry.source.is_none());
    assert!(entry.url.is_none());
}

#[tokio::test]
async fn test_pre_sync_sources() {
    // Skip test if git is not available
    if std::process::Command::new("git").arg("--version").output().is_err() {
        eprintln!("Skipping test: git not available");
        return;
    }

    // Create a test Git repository with resources
    let temp_dir = TempDir::new().unwrap();
    let repo_dir = temp_dir.path().join("test-repo");
    std::fs::create_dir(&repo_dir).unwrap();

    // Initialize git repo
    std::process::Command::new("git").args(["init"]).current_dir(&repo_dir).output().unwrap();

    std::process::Command::new("git")
        .args(["config", "user.email", "test@example.com"])
        .current_dir(&repo_dir)
        .output()
        .unwrap();

    std::process::Command::new("git")
        .args(["config", "user.name", "Test User"])
        .current_dir(&repo_dir)
        .output()
        .unwrap();

    // Create test files
    std::fs::create_dir_all(repo_dir.join("agents")).unwrap();
    std::fs::write(repo_dir.join("agents/test.md"), "# Test Agent\n\nTest content").unwrap();

    // Commit files
    std::process::Command::new("git").args(["add", "."]).current_dir(&repo_dir).output().unwrap();

    std::process::Command::new("git")
        .args(["commit", "-m", "Initial commit"])
        .current_dir(&repo_dir)
        .output()
        .unwrap();

    std::process::Command::new("git")
        .args(["tag", "v1.0.0"])
        .current_dir(&repo_dir)
        .output()
        .unwrap();

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

    let cache = Cache::with_dir(temp_dir.path().to_path_buf()).unwrap();
    let mut resolver = DependencyResolver::with_cache(manifest, cache);

    // Get dependencies for pre-sync
    let deps: Vec<(String, ResourceDependency)> = resolver
        .manifest
        .all_dependencies()
        .into_iter()
        .map(|(name, dep)| (name.to_string(), dep.clone()))
        .collect();

    // Pre-sync sources
    resolver.pre_sync_sources(&deps).await.unwrap();

    // Verify that the source was synced
    assert!(resolver.version_resolver.has_entries());
}

#[tokio::test]
async fn test_resolve_with_transitive_dependencies() {
    let temp_dir = TempDir::new().unwrap();
    let mut manifest = Manifest::new();
    manifest.manifest_dir = Some(temp_dir.path().to_path_buf());

    // Create a local dependency with transitive dependencies
    let agents_dir = temp_dir.path().join("agents");
    std::fs::create_dir_all(&agents_dir).unwrap();

    // Create a parent agent that depends on a helper
    let parent_content = r#"---
dependencies:
  agents:
    - path: "helper.md"
---
# Parent Agent

This agent depends on helper.md.
"#;
    std::fs::write(agents_dir.join("parent.md"), parent_content).unwrap();

    // Create the helper agent
    std::fs::write(agents_dir.join("helper.md"), "# Helper Agent\n\nHelper content").unwrap();

    manifest.add_dependency(
        "parent".to_string(),
        ResourceDependency::Simple("agents/parent.md".to_string()),
        true,
    );

    let cache = Cache::with_dir(temp_dir.path().to_path_buf()).unwrap();
    let mut resolver = DependencyResolver::with_cache(manifest, cache);

    let lockfile = resolver.resolve().await.unwrap();

    // Should have both parent and helper agents
    assert_eq!(lockfile.agents.len(), 2);

    let parent_entry = lockfile.agents.iter().find(|e| e.name == "parent").unwrap();
    let helper_entry = lockfile.agents.iter().find(|e| e.name == "helper").unwrap();

    assert_eq!(parent_entry.name, "parent");
    assert_eq!(helper_entry.name, "helper");

    // Parent should depend on helper
    assert!(parent_entry.dependencies.contains(&"agent/helper".to_string()));
}

#[tokio::test]
async fn test_pattern_expansion() {
    let temp_dir = TempDir::new().unwrap();
    let mut manifest = Manifest::new();
    manifest.manifest_dir = Some(temp_dir.path().to_path_buf());

    // Create multiple agents in a directory
    let agents_dir = temp_dir.path().join("agents");
    std::fs::create_dir_all(&agents_dir).unwrap();

    std::fs::write(agents_dir.join("agent1.md"), "# Agent 1").unwrap();
    std::fs::write(agents_dir.join("agent2.md"), "# Agent 2").unwrap();
    std::fs::write(agents_dir.join("agent3.md"), "# Agent 3").unwrap();

    // Add a pattern dependency
    manifest.add_dependency(
        "all-agents".to_string(),
        ResourceDependency::Simple("agents/*.md".to_string()),
        true,
    );

    let cache = Cache::with_dir(temp_dir.path().to_path_buf()).unwrap();
    let mut resolver = DependencyResolver::with_cache(manifest, cache);

    let lockfile = resolver.resolve().await.unwrap();

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
}

#[tokio::test]
async fn test_conflict_detection() {
    let temp_dir = TempDir::new().unwrap();
    let mut manifest = Manifest::new();
    manifest.manifest_dir = Some(temp_dir.path().to_path_buf());

    // Create two dependencies that would conflict (same path)
    let agents_dir = temp_dir.path().join("agents");
    std::fs::create_dir_all(&agents_dir).unwrap();

    std::fs::write(agents_dir.join("conflict.md"), "# Conflict Agent").unwrap();

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

    let cache = Cache::with_dir(temp_dir.path().to_path_buf()).unwrap();
    let mut resolver = DependencyResolver::with_cache(manifest, cache);

    let result = resolver.resolve().await;

    // Should detect the conflict
    assert!(result.is_err());
    let error_msg = result.unwrap_err().to_string();
    assert!(error_msg.contains("conflict") || error_msg.contains("same path"));
}

#[test]
fn test_extract_meaningful_path() {
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
}

#[tokio::test]
async fn test_update_specific_dependency() {
    let temp_dir = TempDir::new().unwrap();
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
        variant_inputs: serde_json::json!({}),
    });

    // Create the agent file
    let agents_dir = temp_dir.path().join("agents");
    std::fs::create_dir_all(&agents_dir).unwrap();
    std::fs::write(agents_dir.join("test-agent.md"), "# Test Agent").unwrap();

    manifest.add_dependency(
        "test-agent".to_string(),
        ResourceDependency::Simple("agents/test-agent.md".to_string()),
        true,
    );

    let cache = Cache::with_dir(temp_dir.path().to_path_buf()).unwrap();
    let mut resolver = DependencyResolver::with_cache(manifest, cache);

    // Update the specific dependency
    let lockfile = resolver.update(&lockfile, Some(vec!["test-agent".to_string()])).await.unwrap();

    // Verify the entry was updated
    assert_eq!(lockfile.agents.len(), 1);
    let entry = &lockfile.agents[0];
    assert_eq!(entry.name, "test-agent");
    assert_ne!(entry.resolved_commit, Some("old-commit".to_string()));
    assert_ne!(entry.checksum, "old-checksum");
}

#[tokio::test]
async fn test_version_constraint_resolution() {
    // Skip test if git is not available
    if std::process::Command::new("git").arg("--version").output().is_err() {
        eprintln!("Skipping test: git not available");
        return;
    }

    let temp_dir = TempDir::new().unwrap();
    let repo_dir = temp_dir.path().join("test-repo");
    std::fs::create_dir(&repo_dir).unwrap();

    // Initialize git repo
    std::process::Command::new("git").args(["init"]).current_dir(&repo_dir).output().unwrap();

    std::process::Command::new("git")
        .args(["config", "user.email", "test@example.com"])
        .current_dir(&repo_dir)
        .output()
        .unwrap();

    std::process::Command::new("git")
        .args(["config", "user.name", "Test User"])
        .current_dir(&repo_dir)
        .output()
        .unwrap();

    // Create test files
    std::fs::create_dir_all(repo_dir.join("agents")).unwrap();
    std::fs::write(repo_dir.join("agents/test.md"), "# Test Agent\n\nTest content").unwrap();

    // Create multiple versions
    std::process::Command::new("git").args(["add", "."]).current_dir(&repo_dir).output().unwrap();

    std::process::Command::new("git")
        .args(["commit", "-m", "Initial commit"])
        .current_dir(&repo_dir)
        .output()
        .unwrap();

    std::process::Command::new("git")
        .args(["tag", "v1.0.0"])
        .current_dir(&repo_dir)
        .output()
        .unwrap();

    // Update file and create v2.0.0
    std::fs::write(repo_dir.join("agents/test.md"), "# Test Agent v2\n\nUpdated content").unwrap();

    std::process::Command::new("git").args(["add", "."]).current_dir(&repo_dir).output().unwrap();

    std::process::Command::new("git")
        .args(["commit", "-m", "Update to v2"])
        .current_dir(&repo_dir)
        .output()
        .unwrap();

    std::process::Command::new("git")
        .args(["tag", "v2.0.0"])
        .current_dir(&repo_dir)
        .output()
        .unwrap();

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

    let cache = Cache::with_dir(temp_dir.path().to_path_buf()).unwrap();
    let mut resolver = DependencyResolver::with_cache(manifest, cache);

    let lockfile = resolver.resolve().await.unwrap();

    assert_eq!(lockfile.agents.len(), 1);
    let entry = &lockfile.agents[0];
    assert_eq!(entry.name, "test-agent");
    // Should resolve to v1.0.0 due to ^1.0.0 constraint
    // Lockfile stores RESOLVED version, not constraint (like Cargo.lock)
    assert!(entry.resolved_commit.is_some());
    assert_eq!(entry.version.as_ref().unwrap(), "v1.0.0");
}

#[tokio::test]
async fn test_multi_tool_support() {
    let temp_dir = TempDir::new().unwrap();
    let mut manifest = Manifest::new();
    manifest.manifest_dir = Some(temp_dir.path().to_path_buf());

    // Create agents directory
    let agents_dir = temp_dir.path().join("agents");
    std::fs::create_dir_all(&agents_dir).unwrap();

    std::fs::write(agents_dir.join("claude-agent.md"), "# Claude Agent").unwrap();
    std::fs::write(agents_dir.join("opencode-agent.md"), "# OpenCode Agent").unwrap();

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

    let cache = Cache::with_dir(temp_dir.path().to_path_buf()).unwrap();
    let mut resolver = DependencyResolver::with_cache(manifest, cache);

    let lockfile = resolver.resolve().await.unwrap();

    assert_eq!(lockfile.agents.len(), 2);

    let claude_entry = lockfile.agents.iter().find(|e| e.name == "claude-agent").unwrap();
    let opencode_entry = lockfile.agents.iter().find(|e| e.name == "opencode-agent").unwrap();

    assert_eq!(claude_entry.tool, Some("claude-code".to_string()));
    assert_eq!(opencode_entry.tool, Some("opencode".to_string()));

    // Should install to different tool-specific directories
    assert!(claude_entry.installed_at.contains(".claude/"));
    assert!(opencode_entry.installed_at.contains(".opencode/"));
}

#[tokio::test]
async fn test_dependency_cycle_detection() {
    let temp_dir = TempDir::new().unwrap();
    let mut manifest = Manifest::new();
    manifest.manifest_dir = Some(temp_dir.path().to_path_buf());

    // Create agents directory
    let agents_dir = temp_dir.path().join("agents");
    std::fs::create_dir_all(&agents_dir).unwrap();

    // Create agent A that depends on B
    let agent_a_content = r#"---
dependencies:
  agents:
    - path: "./agent-b.md"
---
# Agent A

Depends on Agent B.
"#;
    std::fs::write(agents_dir.join("agent-a.md"), agent_a_content).unwrap();

    // Create agent B that depends on A (creating a cycle)
    let agent_b_content = r#"---
dependencies:
  agents:
    - path: "./agent-a.md"
---
# Agent B

Depends on Agent A (cycle!).
"#;
    std::fs::write(agents_dir.join("agent-b.md"), agent_b_content).unwrap();

    manifest.add_dependency(
        "agent-a".to_string(),
        ResourceDependency::Simple("agents/agent-a.md".to_string()),
        true,
    );

    let cache = Cache::with_dir(temp_dir.path().to_path_buf()).unwrap();
    let mut resolver = DependencyResolver::with_cache(manifest, cache);

    let result = resolver.resolve().await;

    // Should detect the cycle
    assert!(result.is_err());
    let error_msg = result.unwrap_err().to_string();
    assert!(error_msg.contains("Circular dependency"));
}

#[tokio::test]
async fn test_template_variable_inheritance() {
    let temp_dir = TempDir::new().unwrap();
    let mut manifest = Manifest::new();
    manifest.manifest_dir = Some(temp_dir.path().to_path_buf());

    // Create agents directory
    let agents_dir = temp_dir.path().join("agents");
    std::fs::create_dir_all(&agents_dir).unwrap();

    std::fs::write(agents_dir.join("templated.md"), "# Templated Agent").unwrap();

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

    let cache = Cache::with_dir(temp_dir.path().to_path_buf()).unwrap();
    let mut resolver = DependencyResolver::with_cache(manifest, cache);

    let lockfile = resolver.resolve().await.unwrap();

    assert_eq!(lockfile.agents.len(), 1);
    let entry = &lockfile.agents[0];

    // Should have template variables from dependency
    assert!(!entry.template_vars.is_empty());
}

#[tokio::test]
async fn test_patch_application() {
    let temp_dir = TempDir::new().unwrap();
    let mut manifest = Manifest::new();
    manifest.manifest_dir = Some(temp_dir.path().to_path_buf());

    // Create agents directory
    let agents_dir = temp_dir.path().join("agents");
    std::fs::create_dir_all(&agents_dir).unwrap();

    std::fs::write(agents_dir.join("patched.md"), "# Patched Agent").unwrap();

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

    let cache = Cache::with_dir(temp_dir.path().to_path_buf()).unwrap();
    let mut resolver = DependencyResolver::with_cache(manifest, cache);

    let lockfile = resolver.resolve().await.unwrap();

    assert_eq!(lockfile.agents.len(), 1);
    let entry = &lockfile.agents[0];

    // Should have the patches applied
    assert_eq!(entry.applied_patches.len(), 2);
    assert!(entry.applied_patches.contains_key("model"));
    assert!(entry.applied_patches.contains_key("temperature"));
}
