//! Worktree management for resolved dependency versions.
//!
//! This module handles the creation and management of Git worktrees for
//! resolved dependency versions, enabling parallel operations and efficient
//! resource utilization.

use crate::cache::Cache;
use crate::core::AgpmError;
use crate::source::SourceManager;
use anyhow::Result;
use futures::future::join_all;
use std::collections::HashMap;

use super::version_resolver::VersionResolver;

/// Represents a prepared source version with worktree information.
#[derive(Clone, Debug, Default)]
pub struct PreparedSourceVersion {
    /// Path to the worktree for this version
    pub worktree_path: std::path::PathBuf,
    /// The resolved version reference (tag, branch, etc.)
    pub resolved_version: Option<String>,
    /// The commit SHA for this version
    pub resolved_commit: String,
}

/// Manages worktree creation for resolved dependency versions.
pub struct WorktreeManager<'a> {
    cache: &'a Cache,
    source_manager: &'a SourceManager,
    version_resolver: &'a VersionResolver,
}

impl<'a> WorktreeManager<'a> {
    /// Create a new worktree manager.
    pub fn new(
        cache: &'a Cache,
        source_manager: &'a SourceManager,
        version_resolver: &'a VersionResolver,
    ) -> Self {
        Self {
            cache,
            source_manager,
            version_resolver,
        }
    }

    /// Create a group key for identifying source-version combinations.
    pub fn group_key(source: &str, version: &str) -> String {
        format!("{source}::{version}")
    }

    /// Create worktrees for all resolved versions in parallel.
    ///
    /// This function takes the resolved versions from the VersionResolver
    /// and creates Git worktrees for each unique commit SHA, enabling
    /// efficient parallel access to dependency resources.
    ///
    /// # Returns
    ///
    /// A map of group keys to prepared source versions containing worktree paths.
    pub async fn create_worktrees_for_resolved_versions(
        &self,
    ) -> Result<HashMap<String, PreparedSourceVersion>> {
        let resolved_full = self.version_resolver.get_all_resolved_full().clone();
        let mut prepared_versions = HashMap::new();

        // Build futures for parallel worktree creation
        let mut futures = Vec::new();

        for ((source_name, version_key), resolved_version) in resolved_full {
            let sha = resolved_version.sha;
            let resolved_ref = resolved_version.resolved_ref;
            let repo_key = Self::group_key(&source_name, &version_key);
            let cache_clone = self.cache.clone();
            let source_name_clone = source_name.clone();

            // Get the source URL for this source
            let source_url_clone = self
                .source_manager
                .get_source_url(&source_name)
                .ok_or_else(|| AgpmError::SourceNotFound {
                    name: source_name.to_string(),
                })?
                .to_string();

            let sha_clone = sha.clone();
            let resolved_ref_clone = resolved_ref.clone();

            let future = async move {
                // Use SHA-based worktree creation
                // The version resolver has already handled fetching and SHA resolution
                let worktree_path = cache_clone
                    .get_or_create_worktree_for_sha(
                        &source_name_clone,
                        &source_url_clone,
                        &sha_clone,
                        Some(&source_name_clone), // context for logging
                    )
                    .await?;

                Ok::<_, anyhow::Error>((
                    repo_key,
                    PreparedSourceVersion {
                        worktree_path,
                        resolved_version: Some(resolved_ref_clone),
                        resolved_commit: sha_clone,
                    },
                ))
            };

            futures.push(future);
        }

        // Execute all futures concurrently and collect results
        let results = join_all(futures).await;

        // Process results and build the map
        for result in results {
            let (key, prepared) = result?;
            prepared_versions.insert(key, prepared);
        }

        Ok(prepared_versions)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_group_key() {
        assert_eq!(WorktreeManager::group_key("source", "version"), "source::version");
        assert_eq!(WorktreeManager::group_key("community", "v1.0.0"), "community::v1.0.0");
    }
}
