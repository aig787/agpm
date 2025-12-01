//! Private lockfile management for user-level dependencies and patches.
//!
//! The private lockfile (`agpm.private.lock`) tracks:
//! 1. **Private dependencies**: Full `LockedResource` entries from `agpm.private.toml`
//! 2. **Private patches**: Patches from `agpm.private.toml` applied to project dependencies
//!
//! This separation allows team members to have different private configurations without
//! causing lockfile conflicts in the shared `agpm.lock`.
//!
//! # Structure
//!
//! The private lockfile uses the same array-based format as `agpm.lock`:
//!
//! ```toml
//! version = 1
//!
//! # Full private dependency entries (is_private = true)
//! [[agents]]
//! name = "my-private-agent"
//! source = "private-repo"
//! path = "agents/private.md"
//! checksum = "sha256:..."
//! installed_at = ".claude/agents/private/my-private-agent.md"
//! is_private = true
//! ```
//!
//! # Usage
//!
//! ## Splitting a lockfile
//!
//! After dependency resolution, split the combined lockfile into public and private parts:
//!
//! ```rust,no_run
//! use agpm_cli::lockfile::{LockFile, PrivateLockFile};
//! use std::path::Path;
//!
//! let combined_lockfile = LockFile::new();
//! // ... resolve dependencies ...
//!
//! // Split into public and private parts
//! let (public_lock, private_lock) = combined_lockfile.split_by_privacy();
//!
//! // Save each to appropriate file
//! public_lock.save(Path::new("agpm.lock"))?;
//! private_lock.save(Path::new("."))?;
//! # Ok::<(), anyhow::Error>(())
//! ```
//!
//! ## Loading and merging
//!
//! When loading, merge the private lockfile back into the main lockfile:
//!
//! ```rust,no_run
//! use agpm_cli::lockfile::{LockFile, PrivateLockFile};
//! use std::path::Path;
//!
//! let mut lockfile = LockFile::load(Path::new("agpm.lock"))?;
//! if let Some(private_lock) = PrivateLockFile::load(Path::new("."))? {
//!     lockfile.merge_private(&private_lock);
//! }
//! # Ok::<(), anyhow::Error>(())
//! ```

use super::LockedResource;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

const PRIVATE_LOCK_FILENAME: &str = "agpm.private.lock";
const PRIVATE_LOCK_VERSION: u32 = 1;

/// Private lockfile tracking user-level dependencies.
///
/// This file is gitignored and contains full `LockedResource` entries for
/// dependencies that came from `agpm.private.toml`. It works alongside
/// `agpm.lock` to provide full reproducibility while keeping team lockfiles
/// deterministic.
///
/// Uses the same array-based format as `agpm.lock` for consistency.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PrivateLockFile {
    /// Lockfile format version
    pub version: u32,

    /// Private agents
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub agents: Vec<LockedResource>,

    /// Private snippets
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub snippets: Vec<LockedResource>,

    /// Private commands
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub commands: Vec<LockedResource>,

    /// Private scripts
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scripts: Vec<LockedResource>,

    /// Private MCP servers
    #[serde(default, skip_serializing_if = "Vec::is_empty", rename = "mcp-servers")]
    pub mcp_servers: Vec<LockedResource>,

    /// Private hooks
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hooks: Vec<LockedResource>,

    /// Private skills
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub skills: Vec<LockedResource>,
}

impl Default for PrivateLockFile {
    fn default() -> Self {
        Self::new()
    }
}

impl PrivateLockFile {
    /// Create a new empty private lockfile.
    pub fn new() -> Self {
        Self {
            version: PRIVATE_LOCK_VERSION,
            agents: Vec::new(),
            snippets: Vec::new(),
            commands: Vec::new(),
            scripts: Vec::new(),
            mcp_servers: Vec::new(),
            hooks: Vec::new(),
            skills: Vec::new(),
        }
    }

    /// Load private lockfile from disk.
    ///
    /// Returns `Ok(None)` if the file doesn't exist (no private resources).
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use agpm_cli::lockfile::private_lock::PrivateLockFile;
    /// use std::path::Path;
    ///
    /// let project_dir = Path::new(".");
    /// match PrivateLockFile::load(project_dir)? {
    ///     Some(lock) => println!("Loaded {} private resources", lock.total_resources()),
    ///     None => println!("No private lockfile found"),
    /// }
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    pub fn load(project_dir: &Path) -> Result<Option<Self>> {
        let path = project_dir.join(PRIVATE_LOCK_FILENAME);
        if !path.exists() {
            return Ok(None);
        }

        let content = std::fs::read_to_string(&path)
            .with_context(|| format!("Failed to read {}", path.display()))?;

        let mut lock: Self = toml::from_str(&content)
            .with_context(|| format!("Failed to parse {}", path.display()))?;

        // Validate version
        if lock.version > PRIVATE_LOCK_VERSION {
            anyhow::bail!(
                "Private lockfile version {} is newer than supported version {}. \
                 Please upgrade AGPM.",
                lock.version,
                PRIVATE_LOCK_VERSION
            );
        }

        // Set resource types after deserialization (not stored in TOML)
        Self::set_resource_types(&mut lock.agents, crate::core::ResourceType::Agent);
        Self::set_resource_types(&mut lock.snippets, crate::core::ResourceType::Snippet);
        Self::set_resource_types(&mut lock.commands, crate::core::ResourceType::Command);
        Self::set_resource_types(&mut lock.scripts, crate::core::ResourceType::Script);
        Self::set_resource_types(&mut lock.mcp_servers, crate::core::ResourceType::McpServer);
        Self::set_resource_types(&mut lock.hooks, crate::core::ResourceType::Hook);
        Self::set_resource_types(&mut lock.skills, crate::core::ResourceType::Skill);

        Ok(Some(lock))
    }

    /// Set resource_type for all resources in a vector.
    fn set_resource_types(
        resources: &mut [LockedResource],
        resource_type: crate::core::ResourceType,
    ) {
        for resource in resources {
            resource.resource_type = resource_type;
        }
    }

    /// Save private lockfile to disk.
    ///
    /// Deletes the file if the lockfile is empty (no private resources).
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use agpm_cli::lockfile::private_lock::PrivateLockFile;
    /// use std::path::Path;
    ///
    /// let lock = PrivateLockFile::new();
    /// lock.save(Path::new("."))?;
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    pub fn save(&self, project_dir: &Path) -> Result<()> {
        let path = project_dir.join(PRIVATE_LOCK_FILENAME);

        // Don't create empty lockfiles; delete if exists
        if self.is_empty() {
            if path.exists() {
                std::fs::remove_file(&path)
                    .with_context(|| format!("Failed to remove {}", path.display()))?;
            }
            return Ok(());
        }

        let content = serialize_private_lockfile(self)?;

        std::fs::write(&path, content)
            .with_context(|| format!("Failed to write {}", path.display()))?;

        Ok(())
    }

    /// Check if the lockfile has any private resources.
    pub fn is_empty(&self) -> bool {
        self.agents.is_empty()
            && self.snippets.is_empty()
            && self.commands.is_empty()
            && self.scripts.is_empty()
            && self.mcp_servers.is_empty()
            && self.hooks.is_empty()
            && self.skills.is_empty()
    }

    /// Count total number of private resources.
    pub fn total_resources(&self) -> usize {
        self.agents.len()
            + self.snippets.len()
            + self.commands.len()
            + self.scripts.len()
            + self.mcp_servers.len()
            + self.hooks.len()
            + self.skills.len()
    }

    /// Get all resources from the private lockfile.
    pub fn all_resources(&self) -> Vec<&LockedResource> {
        let mut resources: Vec<&LockedResource> = Vec::new();
        resources.extend(self.agents.iter());
        resources.extend(self.snippets.iter());
        resources.extend(self.commands.iter());
        resources.extend(self.scripts.iter());
        resources.extend(self.mcp_servers.iter());
        resources.extend(self.hooks.iter());
        resources.extend(self.skills.iter());
        resources
    }

    /// Create a private lockfile from a vector of private resources.
    ///
    /// Filters and distributes resources into appropriate type vectors.
    pub fn from_resources(resources: Vec<LockedResource>) -> Self {
        let mut private_lock = Self::new();

        for resource in resources {
            match resource.resource_type {
                crate::core::ResourceType::Agent => private_lock.agents.push(resource),
                crate::core::ResourceType::Snippet => private_lock.snippets.push(resource),
                crate::core::ResourceType::Command => private_lock.commands.push(resource),
                crate::core::ResourceType::Script => private_lock.scripts.push(resource),
                crate::core::ResourceType::McpServer => private_lock.mcp_servers.push(resource),
                crate::core::ResourceType::Hook => private_lock.hooks.push(resource),
                crate::core::ResourceType::Skill => private_lock.skills.push(resource),
            }
        }

        private_lock
    }
}

/// Serialize private lockfile to TOML string.
///
/// Uses the same serialization format as the main lockfile.
fn serialize_private_lockfile(lockfile: &PrivateLockFile) -> Result<String> {
    use toml_edit::{DocumentMut, Item};

    // First serialize to a toml_edit document
    let toml_str =
        toml::to_string_pretty(lockfile).context("Failed to serialize private lockfile to TOML")?;
    let mut doc: DocumentMut = toml_str.parse().context("Failed to parse TOML document")?;

    // Convert all `applied_patches` and `variant_inputs` tables to inline tables
    let resource_types = ["agents", "snippets", "commands", "scripts", "hooks", "mcp-servers", "skills"];

    for resource_type in &resource_types {
        if let Some(Item::ArrayOfTables(array)) = doc.get_mut(resource_type) {
            for table in array.iter_mut() {
                // Convert applied_patches to inline table
                if let Some(Item::Table(patches_table)) = table.get_mut("applied_patches") {
                    let mut inline = toml_edit::InlineTable::new();
                    for (key, val) in patches_table.iter() {
                        if let Some(v) = val.as_value() {
                            inline.insert(key, v.clone());
                        }
                    }
                    table.insert("applied_patches", toml_edit::value(inline));
                }

                // Convert variant_inputs to inline table
                if let Some(Item::Table(variant_table)) = table.get_mut("variant_inputs") {
                    let mut inline = toml_edit::InlineTable::new();
                    for (key, val) in variant_table.iter() {
                        if let Some(v) = val.as_value() {
                            inline.insert(key, v.clone());
                        }
                    }
                    table.insert("variant_inputs", toml_edit::value(inline));
                }
            }
        }
    }

    Ok(doc.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::ResourceType;
    use crate::resolver::lockfile_builder::VariantInputs;
    use std::collections::BTreeMap;
    use tempfile::TempDir;

    fn create_test_resource(name: &str, resource_type: ResourceType) -> LockedResource {
        LockedResource {
            name: name.to_string(),
            source: Some("test-source".to_string()),
            url: Some("https://github.com/test/repo.git".to_string()),
            path: format!("{}/{}.md", resource_type, name),
            version: Some("v1.0.0".to_string()),
            resolved_commit: Some("abc123def456".to_string()),
            checksum: "sha256:test123".to_string(),
            context_checksum: None,
            installed_at: format!(".claude/{}/private/{}.md", resource_type, name),
            dependencies: Vec::new(),
            resource_type,
            tool: Some("claude-code".to_string()),
            manifest_alias: Some(name.to_string()),
            applied_patches: BTreeMap::new(),
            install: None,
            variant_inputs: VariantInputs::default(),
            is_private: true,
        }
    }

    #[test]
    fn test_new_lockfile_is_empty() {
        let lock = PrivateLockFile::new();
        assert!(lock.is_empty());
        assert_eq!(lock.total_resources(), 0);
    }

    #[test]
    fn test_from_resources() {
        let resources = vec![
            create_test_resource("agent1", ResourceType::Agent),
            create_test_resource("snippet1", ResourceType::Snippet),
            create_test_resource("command1", ResourceType::Command),
        ];

        let lock = PrivateLockFile::from_resources(resources);

        assert!(!lock.is_empty());
        assert_eq!(lock.total_resources(), 3);
        assert_eq!(lock.agents.len(), 1);
        assert_eq!(lock.snippets.len(), 1);
        assert_eq!(lock.commands.len(), 1);
    }

    #[test]
    fn test_save_and_load() {
        let temp_dir = TempDir::new().unwrap();
        let resources = vec![create_test_resource("test-agent", ResourceType::Agent)];
        let lock = PrivateLockFile::from_resources(resources);

        // Save
        lock.save(temp_dir.path()).unwrap();

        // Load
        let loaded = PrivateLockFile::load(temp_dir.path()).unwrap();
        assert!(loaded.is_some());
        let loaded_lock = loaded.unwrap();
        assert_eq!(loaded_lock.agents.len(), 1);
        assert_eq!(loaded_lock.agents[0].name, "test-agent");
        assert_eq!(loaded_lock.agents[0].resource_type, ResourceType::Agent);
    }

    #[test]
    fn test_empty_lockfile_deletes_file() {
        let temp_dir = TempDir::new().unwrap();
        let lock_path = temp_dir.path().join(PRIVATE_LOCK_FILENAME);

        // Create file
        std::fs::write(&lock_path, "test").unwrap();
        assert!(lock_path.exists());

        // Save empty lockfile should delete
        let lock = PrivateLockFile::new();
        lock.save(temp_dir.path()).unwrap();
        assert!(!lock_path.exists());
    }

    #[test]
    fn test_load_nonexistent_returns_none() {
        let temp_dir = TempDir::new().unwrap();
        let result = PrivateLockFile::load(temp_dir.path()).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_all_resources() {
        let resources = vec![
            create_test_resource("agent1", ResourceType::Agent),
            create_test_resource("agent2", ResourceType::Agent),
            create_test_resource("snippet1", ResourceType::Snippet),
        ];

        let lock = PrivateLockFile::from_resources(resources);
        let all = lock.all_resources();

        assert_eq!(all.len(), 3);
    }

    #[test]
    fn test_lockfile_split_by_privacy() {
        use crate::lockfile::LockFile;

        // Create a lockfile with both public and private resources
        let mut lockfile = LockFile::new();

        // Add public agents
        let public_agent = LockedResource {
            name: "public-agent".to_string(),
            source: Some("test".to_string()),
            url: Some("https://github.com/test/repo.git".to_string()),
            path: "agents/public.md".to_string(),
            version: Some("v1.0.0".to_string()),
            resolved_commit: Some("abc123".to_string()),
            checksum: "sha256:test".to_string(),
            context_checksum: None,
            installed_at: ".claude/agents/agpm/public.md".to_string(),
            dependencies: Vec::new(),
            resource_type: ResourceType::Agent,
            tool: Some("claude-code".to_string()),
            manifest_alias: Some("public-agent".to_string()),
            applied_patches: BTreeMap::new(),
            install: None,
            variant_inputs: VariantInputs::default(),
            is_private: false,
        };

        // Add private agent
        let private_agent = LockedResource {
            name: "private-agent".to_string(),
            source: Some("private".to_string()),
            url: Some("git@github.com:me/private.git".to_string()),
            path: "agents/private.md".to_string(),
            version: Some("v1.0.0".to_string()),
            resolved_commit: Some("def456".to_string()),
            checksum: "sha256:private".to_string(),
            context_checksum: None,
            installed_at: ".claude/agents/agpm/private/private.md".to_string(),
            dependencies: Vec::new(),
            resource_type: ResourceType::Agent,
            tool: Some("claude-code".to_string()),
            manifest_alias: Some("private-agent".to_string()),
            applied_patches: BTreeMap::new(),
            install: None,
            variant_inputs: VariantInputs::default(),
            is_private: true,
        };

        lockfile.agents.push(public_agent);
        lockfile.agents.push(private_agent);

        // Split by privacy
        let (public_lock, private_lock) = lockfile.split_by_privacy();

        // Public lockfile should have only the public agent
        assert_eq!(public_lock.agents.len(), 1);
        assert_eq!(public_lock.agents[0].name, "public-agent");
        assert!(!public_lock.agents[0].is_private);

        // Private lockfile should have only the private agent
        assert_eq!(private_lock.agents.len(), 1);
        assert_eq!(private_lock.agents[0].name, "private-agent");
        assert!(private_lock.agents[0].is_private);
    }

    #[test]
    fn test_lockfile_merge_private() {
        use crate::lockfile::LockFile;

        // Create a public lockfile
        let mut public_lock = LockFile::new();
        public_lock.agents.push(LockedResource {
            name: "public-agent".to_string(),
            source: Some("test".to_string()),
            url: Some("https://github.com/test/repo.git".to_string()),
            path: "agents/public.md".to_string(),
            version: Some("v1.0.0".to_string()),
            resolved_commit: Some("abc123".to_string()),
            checksum: "sha256:test".to_string(),
            context_checksum: None,
            installed_at: ".claude/agents/agpm/public.md".to_string(),
            dependencies: Vec::new(),
            resource_type: ResourceType::Agent,
            tool: Some("claude-code".to_string()),
            manifest_alias: Some("public-agent".to_string()),
            applied_patches: BTreeMap::new(),
            install: None,
            variant_inputs: VariantInputs::default(),
            is_private: false,
        });
        public_lock.resource_count = Some(1);

        // Create private lockfile
        let private_lock = PrivateLockFile::from_resources(vec![create_test_resource(
            "private-agent",
            ResourceType::Agent,
        )]);

        // Merge private into public
        public_lock.merge_private(&private_lock);

        // Should now have both agents
        assert_eq!(public_lock.agents.len(), 2);
        assert!(public_lock.agents.iter().any(|a| a.name == "public-agent"));
        assert!(public_lock.agents.iter().any(|a| a.name == "private-agent"));

        // Resource count should be updated
        assert_eq!(public_lock.resource_count, Some(2));
    }

    #[test]
    fn test_split_and_merge_roundtrip() {
        use crate::lockfile::LockFile;

        // Create original lockfile with mixed resources
        let mut original = LockFile::new();
        original.agents.push(LockedResource {
            name: "public".to_string(),
            source: Some("test".to_string()),
            url: Some("https://github.com/test/repo.git".to_string()),
            path: "agents/public.md".to_string(),
            version: Some("v1.0.0".to_string()),
            resolved_commit: Some("abc123".to_string()),
            checksum: "sha256:test".to_string(),
            context_checksum: None,
            installed_at: ".claude/agents/agpm/public.md".to_string(),
            dependencies: Vec::new(),
            resource_type: ResourceType::Agent,
            tool: Some("claude-code".to_string()),
            manifest_alias: Some("public".to_string()),
            applied_patches: BTreeMap::new(),
            install: None,
            variant_inputs: VariantInputs::default(),
            is_private: false,
        });
        original.agents.push(LockedResource {
            name: "private".to_string(),
            source: Some("private".to_string()),
            url: Some("git@github.com:me/private.git".to_string()),
            path: "agents/private.md".to_string(),
            version: Some("v1.0.0".to_string()),
            resolved_commit: Some("def456".to_string()),
            checksum: "sha256:private".to_string(),
            context_checksum: None,
            installed_at: ".claude/agents/agpm/private/private.md".to_string(),
            dependencies: Vec::new(),
            resource_type: ResourceType::Agent,
            tool: Some("claude-code".to_string()),
            manifest_alias: Some("private".to_string()),
            applied_patches: BTreeMap::new(),
            install: None,
            variant_inputs: VariantInputs::default(),
            is_private: true,
        });

        // Split
        let (mut public_lock, private_lock) = original.split_by_privacy();

        // After split, public should have 1, private should have 1
        assert_eq!(public_lock.agents.len(), 1);
        assert_eq!(private_lock.agents.len(), 1);

        // Merge back
        public_lock.merge_private(&private_lock);

        // Should be back to 2
        assert_eq!(public_lock.agents.len(), 2);
    }
}
