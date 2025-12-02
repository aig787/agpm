//! Lockfile entry building and conflict tracking.
//!
//! This module handles the creation and management of lockfile entries,
//! including deduplication, conflict tracking, and backtracking updates.

use std::str::FromStr;

use anyhow::Result;

use crate::core::ResourceType;
use crate::lockfile::lockfile_dependency_ref::LockfileDependencyRef;
use crate::lockfile::{LockFile, LockedResource};
use crate::manifest::ResourceDependency;

use super::DependencyResolver;
use super::backtracking;
use super::lockfile_builder;
use super::types::ResolvedDependencyInfo;

impl DependencyResolver {
    /// Add or update a lockfile entry with deduplication.
    pub(super) fn add_or_update_lockfile_entry(
        &self,
        lockfile: &mut LockFile,
        entry: LockedResource,
    ) {
        let resources = lockfile.get_resources_mut(&entry.resource_type);

        if let Some(existing) =
            resources.iter_mut().find(|e| lockfile_builder::is_duplicate_entry(e, &entry))
        {
            // Use the lockfile_builder's deterministic merge strategy
            // This ensures consistent behavior regardless of processing order
            if lockfile_builder::should_replace_duplicate(existing, &entry) {
                tracing::debug!(
                    "Replacing {} (manifest_alias={:?}) with {} (manifest_alias={:?})",
                    existing.name,
                    existing.manifest_alias,
                    entry.name,
                    entry.manifest_alias
                );
                *existing = entry;
            } else {
                tracing::debug!(
                    "Keeping {} (manifest_alias={:?}) over {} (manifest_alias={:?})",
                    existing.name,
                    existing.manifest_alias,
                    entry.name,
                    entry.manifest_alias
                );
            }
        } else {
            resources.push(entry);
        }
    }

    /// Add version information to dependency references in lockfile.
    pub(super) fn add_version_to_dependencies(&self, lockfile: &mut LockFile) -> Result<()> {
        lockfile_builder::add_version_to_all_dependencies(lockfile);
        Ok(())
    }

    /// Detect target path conflicts between resources.
    pub(super) fn detect_target_conflicts(&self, lockfile: &LockFile) -> Result<()> {
        lockfile_builder::detect_target_conflicts(lockfile)
    }

    /// Track a resolved dependency for later conflict detection.
    ///
    /// CRITICAL: This function is carefully structured to avoid AB-BA deadlocks.
    /// When multiple threads process pattern-expanded dependencies in parallel,
    /// they may simultaneously:
    /// 1. Iterate `resolved_deps_for_conflict_check` (holding read locks on shards)
    /// 2. Insert into `resolved_deps_for_conflict_check` (needing write locks)
    ///
    /// If Thread A holds shard X and wants shard Y, while Thread B holds shard Y
    /// and wants shard X, both deadlock. To prevent this:
    /// - Collect all data needed (including iteration results) FIRST
    /// - Release all DashMap guards
    /// - THEN perform inserts
    pub(super) fn track_resolved_dependency_for_conflicts(
        &mut self,
        name: &str,
        dep: &ResourceDependency,
        locked_entry: &LockedResource,
        resource_type: ResourceType,
    ) {
        // Skip if install=false (content-only dependencies don't conflict)
        if dep.get_install() == Some(false) {
            tracing::debug!(
                "Skipping conflict tracking for content-only dependency '{}' (install=false)",
                name
            );
            return;
        }

        // Skip local dependencies (no version conflicts possible)
        if dep.is_local() {
            return;
        }

        // Build a unique resource identifier that includes variant/context information
        let resource_id = Self::build_resource_identity(dep, locked_entry, resource_type);

        // Get version constraint (None means HEAD/unspecified)
        let version_constraint = dep.get_version().unwrap_or("HEAD");

        // Get resolved SHA from locked entry
        let resolved_sha = locked_entry.resolved_commit.as_deref().unwrap_or("");

        // Skip if no resolved commit (shouldn't happen for Git deps, but be safe)
        if resolved_sha.is_empty() {
            tracing::warn!("Skipping conflict tracking for '{}': no resolved commit", name);
            return;
        }

        // Determine parent resources using the reverse dependency map populated by previously
        // processed parents (topological order ensures parents run first).
        let current_dep_ref =
            LockfileDependencyRef::local(resource_type, locked_entry.name.clone(), None)
                .to_string();

        // DEADLOCK PREVENTION: Collect all data needed for inserts BEFORE inserting.
        // This two-phase approach ensures we never hold iteration locks while inserting.
        //
        // Phase 1: Collect parent metadata (may iterate resolved_deps_for_conflict_check)
        let parent_entries: Vec<(String, Option<String>, Option<String>)> =
            if let Some(required_by_list) = self.reverse_dependency_map.get(&current_dep_ref) {
                // Clone the list to release the DashMap guard before lookup_parent_metadata
                let required_by_vec: Vec<String> = required_by_list.value().to_vec();
                drop(required_by_list); // Explicitly release guard

                // Now safe to iterate resolved_deps_for_conflict_check in lookup_parent_metadata
                required_by_vec
                    .into_iter()
                    .map(|required_by| {
                        let (parent_version, parent_sha) =
                            self.lookup_parent_metadata(&required_by);
                        (required_by, parent_version, parent_sha)
                    })
                    .collect()
            } else {
                vec![]
            };
        // All DashMap guards from Phase 1 are now released

        // Phase 2: Perform inserts (no iteration locks held)
        if parent_entries.is_empty() {
            // Direct dependency from manifest - no parent
            tracing::debug!(
                "TRACK: DIRECT resource_id='{}' required_by='manifest' version='{}' SHA={}",
                resource_id,
                version_constraint,
                &resolved_sha[..8.min(resolved_sha.len())]
            );

            let key = (resource_id.clone(), "manifest".to_string(), name.to_string());
            let dependency_info = ResolvedDependencyInfo {
                version_constraint: version_constraint.to_string(),
                resolved_sha: resolved_sha.to_string(),
                parent_version: None,
                parent_sha: None,
                resolution_mode: dep.resolution_mode(),
            };
            self.resolved_deps_for_conflict_check.insert(key, dependency_info);
        } else {
            // Transitive dependency - track all parents with their metadata
            for (required_by, parent_version, parent_sha) in parent_entries {
                tracing::debug!(
                    "TRACK: TRANSITIVE resource_id='{}' required_by='{}' version='{}' SHA={} parent_version={:?} parent_sha={:?}",
                    resource_id,
                    required_by,
                    version_constraint,
                    &resolved_sha[..8.min(resolved_sha.len())],
                    parent_version,
                    parent_sha.as_ref().map(|s| &s[..8.min(s.len())])
                );

                let key = (resource_id.clone(), required_by.clone(), name.to_string());
                let dependency_info = ResolvedDependencyInfo {
                    version_constraint: version_constraint.to_string(),
                    resolved_sha: resolved_sha.to_string(),
                    parent_version,
                    parent_sha,
                    resolution_mode: dep.resolution_mode(),
                };
                self.resolved_deps_for_conflict_check.insert(key, dependency_info);
            }
        }

        tracing::debug!(
            "Tracked for conflict detection: '{}' (resource_id: {}, constraint: {}, sha: {})",
            name,
            resource_id,
            version_constraint,
            &resolved_sha[..8.min(resolved_sha.len())],
        );

        // Record reverse dependency relationships for future child lookups.
        // This is safe: we're not iterating while inserting here.
        for child_ref in &locked_entry.dependencies {
            self.reverse_dependency_map
                .entry(child_ref.clone())
                .or_default()
                .value_mut()
                .push(current_dep_ref.clone());
        }
    }

    /// Look up parent resource metadata from already-tracked dependencies.
    ///
    /// Searches for parent entry by resource ID (e.g., "agents/agent-a").
    ///
    /// # Safety
    ///
    /// This function iterates over `resolved_deps_for_conflict_check`, which holds
    /// shard read locks during iteration. Callers MUST NOT insert into
    /// `resolved_deps_for_conflict_check` while this iteration is in progress,
    /// or a deadlock may occur. The caller (`track_resolved_dependency_for_conflicts`)
    /// ensures this by collecting all lookup results before performing any inserts.
    ///
    /// Returns (parent_version_constraint, parent_resolved_sha) if found.
    pub(super) fn lookup_parent_metadata(
        &self,
        parent_id: &str,
    ) -> (Option<String>, Option<String>) {
        // Normalize the parent ID to just the dependency path without extensions.
        // Handles formats like "snippet:snippets/foo", "source/snippet:snippets/foo@v1", etc.
        let normalized_parent_path = LockfileDependencyRef::from_str(parent_id)
            .map(|dep| dep.path)
            .unwrap_or_else(|_| {
                parent_id
                    .split('@')
                    .next()
                    .and_then(|s| s.split(':').next_back())
                    .unwrap_or(parent_id)
                    .to_string()
            })
            .trim_end_matches(".md")
            .trim_end_matches(".json")
            .to_string();

        // Search for an entry where resource_id name matches the parent path
        // E.g., for parent_path = "agents/agent-a", look for resource_id with name = "agents/agent-a"
        for entry in self.resolved_deps_for_conflict_check.iter() {
            let ((resource_id, _required_by, _name), dependency_info) =
                (entry.key(), entry.value());
            let ResolvedDependencyInfo {
                version_constraint,
                resolved_sha,
                parent_version: _,
                parent_sha: _,
                resolution_mode: _,
            } = dependency_info;

            // The ResourceId name is the canonical resource name (e.g., "agents/agent-a")
            // Compare directly with normalized parent path
            if resource_id.name() == normalized_parent_path {
                return (Some(version_constraint.clone()), Some(resolved_sha.clone()));
            }
        }

        // Not found - parent might not have been tracked yet (ordering issue)
        (None, None)
    }

    /// Build unique resource identity for conflict tracking.
    ///
    /// Includes name/tool/variant disambiguators so distinct variants don't collide.
    ///
    /// **Note**: Excludes `manifest_alias` intentionally. Different aliases pointing
    /// to the same resource with different versions should be detected as conflicts.
    pub(super) fn build_resource_identity(
        dep: &ResourceDependency,
        locked_entry: &LockedResource,
        resource_type: ResourceType,
    ) -> crate::lockfile::ResourceId {
        let source = locked_entry.source.as_deref().or_else(|| dep.get_source());
        let tool = locked_entry.tool.clone().or_else(|| dep.get_tool().map(str::to_string));
        let variant_hash = locked_entry.variant_inputs.hash().to_string();

        crate::lockfile::ResourceId::new(
            &locked_entry.name,
            source,
            tool.as_deref(),
            resource_type,
            variant_hash,
        )
    }

    /// Apply backtracking updates to resolver state.
    ///
    /// Updates resolver state to reflect new versions found during backtracking:
    /// 1. Parse resource IDs ("source:required_by")
    /// 2. Get source URL
    /// 3. Create worktrees for updated SHAs
    /// 4. Update PreparedSourceVersion entries
    ///
    /// Maintains consistency across worktree paths and resolved SHAs.
    pub(super) async fn apply_backtracking_updates(
        &mut self,
        updates: &[backtracking::VersionUpdate],
    ) -> Result<()> {
        tracing::debug!("Applying {} backtracking update(s)", updates.len());

        for update in updates {
            // Parse resource_id: "source:required_by"
            let parts: Vec<&str> = update.resource_id.splitn(2, ':').collect();
            if parts.len() != 2 {
                tracing::warn!("Invalid resource_id format: {}", update.resource_id);
                continue;
            }
            let source_name = parts[0];
            let _required_by = parts[1];

            // Get source URL
            let source_url = self
                .core
                .source_manager()
                .get_source_url(source_name)
                .ok_or_else(|| anyhow::anyhow!("Source '{}' not found", source_name))?;

            // Create worktree for the new SHA
            tracing::debug!(
                "Creating worktree for {}@{} (SHA: {})",
                source_name,
                update.new_version,
                &update.new_sha[..8.min(update.new_sha.len())]
            );

            let worktree_path = self
                .core
                .cache()
                .get_or_create_worktree_for_sha(
                    source_name,
                    &source_url,
                    &update.new_sha,
                    Some(source_name),
                )
                .await?;

            // Update PreparedSourceVersion in version service
            // The key format is "source::version_constraint"
            // We need to update entries that match this source and old version
            let prepared_versions = self.version_service.prepared_versions();

            // Find and update the entry
            // Note: The key uses the constraint, not the resolved version
            // We need to find which constraint resolved to the old version
            for mut entry in prepared_versions.iter_mut() {
                let key = entry.key().clone();
                if let super::version_resolver::PreparedVersionState::Ready(prepared) =
                    entry.value_mut()
                {
                    if key.starts_with(&format!("{}::", source_name))
                        && prepared.resolved_commit == update.old_sha
                    {
                        tracing::debug!("Updating prepared version key: {}", key);
                        prepared.worktree_path = worktree_path.clone();
                        prepared.resolved_version = Some(update.new_version.clone());
                        prepared.resolved_commit = update.new_sha.clone();
                    }
                }
            }
        }

        Ok(())
    }

    /// Update lockfile entries after backtracking.
    ///
    /// Finds entries with old SHAs and updates to new SHAs.
    pub(super) fn update_lockfile_entries(
        &self,
        lockfile: &mut LockFile,
        updates: &[backtracking::VersionUpdate],
    ) -> Result<()> {
        tracing::debug!("Updating lockfile entries for {} backtracking update(s)", updates.len());

        for update in updates {
            // Parse resource_id: "source:required_by"
            let parts: Vec<&str> = update.resource_id.splitn(2, ':').collect();
            if parts.len() != 2 {
                tracing::warn!("Invalid resource_id format: {}", update.resource_id);
                continue;
            }
            let source_name = parts[0];

            // Find all lockfile entries with the old SHA
            // Update them to use the new SHA and worktree path
            for resource_type in [
                ResourceType::Agent,
                ResourceType::Snippet,
                ResourceType::Command,
                ResourceType::Script,
                ResourceType::Hook,
                ResourceType::McpServer,
                ResourceType::Skill,
            ] {
                let resources = lockfile.get_resources_mut(&resource_type);

                for resource in resources.iter_mut() {
                    // Check if this resource matches: same source and old SHA
                    let matches = resource.source.as_deref() == Some(source_name)
                        && resource.resolved_commit.as_deref() == Some(&update.old_sha);

                    if matches {
                        tracing::debug!(
                            "Updating lockfile entry: {} (SHA: {} → {})",
                            resource.name,
                            &update.old_sha[..8.min(update.old_sha.len())],
                            &update.new_sha[..8.min(update.new_sha.len())]
                        );

                        // Update the resolved commit
                        resource.resolved_commit = Some(update.new_sha.clone());

                        // Update the version if present
                        if resource.version.is_some() {
                            resource.version = Some(update.new_version.clone());
                        }

                        // Note: installed_at path doesn't change - it's the target path
                        // The source path is implicitly from the updated worktree
                    }
                }
            }
        }

        Ok(())
    }
}
