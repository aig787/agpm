//! Tests for the resolver module.

use super::*;
use crate::manifest::DetailedDependency;
use crate::resolver::lockfile_builder::VariantInputs;
use crate::test_utils::TestGit;
use tempfile::TempDir;

#[tokio::test]
async fn resolver_new() -> Result<(), Box<dyn std::error::Error>> {
    let temp_dir = TempDir::new()?;
    let mut manifest = Manifest::new();
    manifest.manifest_dir = Some(temp_dir.path().to_path_buf());

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
    manifest.add_typed_dependency(
        "local-agent".to_string(),
        ResourceDependency::Simple("../agents/local.md".to_string()),
        ResourceType::Agent,
    );

    // Create dummy file to allow transitive dependency extraction
    let agents_dir = temp_dir.path().parent().ok_or("No parent directory")?.join("agents");
    std::fs::create_dir_all(&agents_dir)?;
    std::fs::write(agents_dir.join("local.md"), "# Local Agent")?;

    let cache = Cache::with_dir(temp_dir.path().to_path_buf())?;
    let mut resolver = DependencyResolver::with_cache(manifest, cache).await?;

    let lockfile = resolver.resolve().await?;
    assert_eq!(lockfile.agents.len(), 1);

    let entry = &lockfile.agents[0];
    assert_eq!(entry.manifest_alias.as_deref(), Some("local-agent"));
    assert_eq!(entry.path, "../agents/local.md");
    assert!(entry.source.is_none());
    assert!(entry.url.is_none());
    Ok(())
}

#[tokio::test]
async fn pre_sync_sources() -> Result<(), Box<dyn std::error::Error>> {
    // Create a test Git repository with resources
    let temp_dir = TempDir::new()?;
    let repo_dir = temp_dir.path().join("test-repo");
    std::fs::create_dir(&repo_dir)?;

    // Initialize git repo using TestGit helper
    let git = TestGit::new(&repo_dir);
    git.init()?;
    git.config_user()?;

    // Create test files
    std::fs::create_dir_all(repo_dir.join("agents"))?;
    std::fs::write(repo_dir.join("agents/test.md"), "# Test Agent\n\nTest content")?;

    // Commit files and tag using TestGit helper
    git.add_all()?;
    git.commit("Initial commit")?;
    git.tag("v1.0.0")?;

    // Create a manifest with a dependency from this source
    let mut manifest = Manifest::new();
    let source_url = format!("file://{}", repo_dir.display());
    manifest.add_source("test-source".to_string(), source_url.clone());

    manifest.add_typed_dependency(
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
        ResourceType::Agent,
    );

    let cache = Cache::with_dir(temp_dir.path().to_path_buf())?;
    let _resolver = DependencyResolver::with_cache(manifest, cache).await?;

    // Pre-sync would happen automatically during resolve
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

    manifest.add_typed_dependency(
        "parent".to_string(),
        ResourceDependency::Simple("agents/parent.md".to_string()),
        ResourceType::Agent,
    );

    let cache = Cache::with_dir(temp_dir.path().to_path_buf())?;
    let mut resolver = DependencyResolver::with_cache(manifest, cache).await?;

    let lockfile = resolver.resolve().await?;

    // Should have both parent and helper agents
    assert_eq!(lockfile.agents.len(), 2);

    // Find entries using flexible matching
    let parent_entry = lockfile
        .agents
        .iter()
        .find(|e| e.path.contains("parent.md") || e.manifest_alias.as_deref() == Some("parent"))
        .ok_or("Parent entry not found")?;
    let helper_entry = lockfile
        .agents
        .iter()
        .find(|e| e.path.contains("helper.md"))
        .ok_or("Helper entry not found")?;

    // Verify we have the right entries
    assert!(
        parent_entry.manifest_alias.as_deref() == Some("parent") || parent_entry.name == "parent"
    );
    assert!(helper_entry.name == "helper" || helper_entry.name == "agents/helper");

    // Verify both agents are present - transitive dependency resolution works
    // Note: The dependencies field on parent_entry may be populated differently
    // depending on whether the transitive resolver stores the linkage
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
    manifest.add_typed_dependency(
        "all-agents".to_string(),
        ResourceDependency::Simple("agents/*.md".to_string()),
        ResourceType::Agent,
    );

    let cache = Cache::with_dir(temp_dir.path().to_path_buf())?;
    let mut resolver = DependencyResolver::with_cache(manifest, cache).await?;

    let lockfile = resolver.resolve().await?;

    // Should have all three agents
    assert_eq!(lockfile.agents.len(), 3);

    let agent_names: std::collections::HashSet<_> =
        lockfile.agents.iter().map(|e| e.name.as_str()).collect();

    assert!(agent_names.contains("agents/agent1"));
    assert!(agent_names.contains("agents/agent2"));
    assert!(agent_names.contains("agents/agent3"));

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
    manifest.add_typed_dependency(
        "agent-a".to_string(),
        ResourceDependency::Simple("agents/conflict.md".to_string()),
        ResourceType::Agent,
    );

    manifest.add_typed_dependency(
        "agent-b".to_string(),
        ResourceDependency::Simple("agents/conflict.md".to_string()),
        ResourceType::Agent,
    );

    let cache = Cache::with_dir(temp_dir.path().to_path_buf())?;
    let mut resolver = DependencyResolver::with_cache(manifest, cache).await?;

    let result = resolver.resolve().await;

    // Should detect the conflict
    assert!(result.is_err());
    let error_msg = result.unwrap_err().to_string();
    assert!(error_msg.contains("conflict") || error_msg.contains("same path"));
    Ok(())
}

#[test]
fn test_extract_meaningful_path() -> Result<(), Box<dyn std::error::Error>> {
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

    // Add an existing entry - use canonical name (derived from path)
    lockfile.agents.push(LockedResource {
        name: "agents/test-agent".to_string(), // Canonical name from path
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
        manifest_alias: Some("test-agent".to_string()), // User's manifest key
        context_checksum: None,
        applied_patches: std::collections::BTreeMap::new(),
        install: None,
        variant_inputs: VariantInputs::default(),
    });

    // Create the agent file
    let agents_dir = temp_dir.path().join("agents");
    std::fs::create_dir_all(&agents_dir)?;
    std::fs::write(agents_dir.join("test-agent.md"), "# Test Agent")?;

    manifest.add_typed_dependency(
        "test-agent".to_string(),
        ResourceDependency::Simple("agents/test-agent.md".to_string()),
        ResourceType::Agent,
    );

    let cache = Cache::with_dir(temp_dir.path().to_path_buf())?;
    let mut resolver = DependencyResolver::with_cache(manifest, cache).await?;

    // Update the specific dependency using the manifest alias
    let lockfile = resolver.update(&lockfile, Some(vec!["test-agent".to_string()]), None).await?;

    // Verify the entry was updated
    assert_eq!(lockfile.agents.len(), 1);
    let entry = &lockfile.agents[0];
    assert_eq!(entry.name, "agents/test-agent"); // Canonical name
    assert_eq!(entry.manifest_alias.as_deref(), Some("test-agent")); // Manifest key
    assert_ne!(entry.resolved_commit, Some("old-commit".to_string()));
    assert_ne!(entry.checksum, "old-checksum");
    Ok(())
}

#[tokio::test]
async fn version_constraint_resolution() -> Result<(), Box<dyn std::error::Error>> {
    let temp_dir = TempDir::new()?;
    let repo_dir = temp_dir.path().join("test-repo");
    std::fs::create_dir(&repo_dir)?;

    // Initialize git repo using TestGit helper
    let git = TestGit::new(&repo_dir);
    git.init()?;
    git.config_user()?;

    // Create test files
    std::fs::create_dir_all(repo_dir.join("agents"))?;
    std::fs::write(repo_dir.join("agents/test.md"), "# Test Agent\n\nTest content")?;

    // Create v1.0.0 using TestGit helper
    git.add_all()?;
    git.commit("Initial commit")?;
    git.tag("v1.0.0")?;

    // Update file and create v2.0.0 using TestGit helper
    std::fs::write(repo_dir.join("agents/test.md"), "# Test Agent v2\n\nUpdated content")?;
    git.add_all()?;
    git.commit("Update to v2")?;
    git.tag("v2.0.0")?;

    // Create manifest with version constraint
    let mut manifest = Manifest::new();
    let source_url = format!("file://{}", repo_dir.display());
    manifest.add_source("test-source".to_string(), source_url.clone());

    manifest.add_typed_dependency(
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
        ResourceType::Agent,
    );

    let cache = Cache::with_dir(temp_dir.path().to_path_buf())?;
    let mut resolver = DependencyResolver::with_cache(manifest, cache).await?;

    let lockfile = resolver.resolve().await?;

    assert_eq!(lockfile.agents.len(), 1);
    let entry = &lockfile.agents[0];
    // Canonical name is derived from path: "agents/test.md" → "agents/test"
    assert_eq!(entry.name, "agents/test");
    // Manifest alias is the user-facing identifier from agpm.toml
    assert_eq!(entry.manifest_alias.as_deref(), Some("test-agent"));
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
    manifest.add_typed_dependency(
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
        ResourceType::Agent,
    );

    manifest.add_typed_dependency(
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
        ResourceType::Agent,
    );

    let cache = Cache::with_dir(temp_dir.path().to_path_buf())?;
    let mut resolver = DependencyResolver::with_cache(manifest, cache).await?;

    let lockfile = resolver.resolve().await?;

    assert_eq!(lockfile.agents.len(), 2);

    let claude_entry = lockfile
        .agents
        .iter()
        .find(|e| {
            e.name == "agents/claude-agent" || e.manifest_alias.as_deref() == Some("claude-agent")
        })
        .ok_or("Claude entry not found")?;
    let opencode_entry = lockfile
        .agents
        .iter()
        .find(|e| {
            e.name == "agents/opencode-agent"
                || e.manifest_alias.as_deref() == Some("opencode-agent")
        })
        .ok_or("OpenCode entry not found")?;

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

    manifest.add_typed_dependency(
        "agent-a".to_string(),
        ResourceDependency::Simple("agents/agent-a.md".to_string()),
        ResourceType::Agent,
    );

    let cache = Cache::with_dir(temp_dir.path().to_path_buf())?;
    let mut resolver = DependencyResolver::with_cache(manifest, cache).await?;

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
    manifest.add_typed_dependency(
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
        ResourceType::Agent,
    );

    let cache = Cache::with_dir(temp_dir.path().to_path_buf())?;
    let mut resolver = DependencyResolver::with_cache(manifest, cache).await?;

    let lockfile = resolver.resolve().await?;

    assert_eq!(lockfile.agents.len(), 1);
    let entry = &lockfile.agents[0];

    // Should have variant inputs (template variables are stored here)
    assert!(!entry.variant_inputs.json().as_object().unwrap().is_empty());
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
    manifest.add_typed_dependency(
        "patched".to_string(),
        ResourceDependency::Simple("agents/patched.md".to_string()),
        ResourceType::Agent,
    );

    // Add patches
    use crate::manifest::PatchData;
    let mut patch_data = PatchData::new();
    patch_data.insert("model".to_string(), toml::Value::String("claude-3-haiku".to_string()));
    patch_data.insert("temperature".to_string(), toml::Value::Float(0.8));
    manifest.patches.agents.insert("patched".to_string(), patch_data);

    let cache = Cache::with_dir(temp_dir.path().to_path_buf())?;
    let mut resolver = DependencyResolver::with_cache(manifest, cache).await?;

    let lockfile = resolver.resolve().await?;

    assert_eq!(lockfile.agents.len(), 1);
    let entry = &lockfile.agents[0];

    // Should have been patched
    assert!(!entry.applied_patches.is_empty());
    Ok(())
}
