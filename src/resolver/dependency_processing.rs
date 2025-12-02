//! Dependency processing and resolution for individual dependencies.
//!
//! This module contains the core logic for resolving individual dependencies
//! to locked resources, handling both local and Git-based dependencies.

use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::core::ResourceType;
use crate::lockfile::LockedResource;
use crate::manifest::ResourceDependency;

use super::lockfile_builder;
use super::path_resolver as install_path_resolver;
use super::source_context::SourceContext;
use super::{DependencyResolver, ResolutionCore, generate_dependency_name};

impl DependencyResolver {
    /// Resolve a single dependency to a lockfile entry.
    ///
    /// Delegates to specialized resolvers based on dependency type.
    pub(super) async fn resolve_dependency(
        &self,
        name: &str,
        dep: &ResourceDependency,
        resource_type: ResourceType,
    ) -> Result<LockedResource> {
        tracing::debug!(
            "resolve_dependency: name={}, path={}, source={:?}, is_local={}",
            name,
            dep.get_path(),
            dep.get_source(),
            dep.is_local()
        );

        if dep.is_local() {
            self.resolve_local_dependency(name, dep, resource_type)
        } else {
            self.resolve_git_dependency(name, dep, resource_type).await
        }
    }

    /// Determine filename for a dependency.
    ///
    /// Returns custom filename if specified, otherwise extracts from path.
    pub(super) fn resolve_filename(dep: &ResourceDependency) -> String {
        dep.get_filename().map_or_else(
            || super::extract_meaningful_path(Path::new(dep.get_path())),
            |f| f.to_string(),
        )
    }

    /// Get tool/artifact type for a dependency.
    ///
    /// Returns explicit tool or default for resource type.
    pub(super) fn resolve_tool(
        &self,
        dep: &ResourceDependency,
        resource_type: ResourceType,
    ) -> String {
        dep.get_tool()
            .map(|s| s.to_string())
            .unwrap_or_else(|| self.core.manifest().get_default_tool(resource_type))
    }

    /// Determine manifest_alias for a dependency.
    ///
    /// Returns Some for direct/pattern dependencies, None for transitive.
    pub(super) fn resolve_manifest_alias(
        &self,
        name: &str,
        resource_type: ResourceType,
    ) -> Option<String> {
        let has_pattern_alias = self.get_pattern_alias_for_dependency(name, resource_type);
        let is_in_manifest = self
            .core
            .manifest()
            .get_dependencies(resource_type)
            .is_some_and(|deps| deps.contains_key(name));

        if let Some(pattern_alias) = has_pattern_alias {
            // Pattern-expanded dependency - use pattern name as manifest_alias
            Some(pattern_alias)
        } else if is_in_manifest {
            // Direct manifest dependency - use name as manifest_alias
            Some(name.to_string())
        } else {
            // Transitive dependency - no manifest_alias
            None
        }
    }

    /// Resolve local file system dependency to locked resource.
    pub(super) fn resolve_local_dependency(
        &self,
        name: &str,
        dep: &ResourceDependency,
        resource_type: ResourceType,
    ) -> Result<LockedResource> {
        use crate::utils::normalize_path_for_storage;

        let filename = Self::resolve_filename(dep);
        let artifact_type_string = self.resolve_tool(dep, resource_type);
        let artifact_type = artifact_type_string.as_str();

        let installed_at = install_path_resolver::resolve_install_path(
            self.core.manifest(),
            dep,
            artifact_type,
            resource_type,
            &filename,
        )?;

        let manifest_alias = self.resolve_manifest_alias(name, resource_type);

        tracing::debug!(
            "Local dependency: name={}, path={}, manifest_alias={:?}",
            name,
            dep.get_path(),
            manifest_alias
        );

        let applied_patches = lockfile_builder::get_patches_for_resource(
            self.core.manifest(),
            resource_type,
            name,
            manifest_alias.as_deref(),
        );

        // Generate canonical name for local dependencies
        // For transitive dependencies (manifest_alias=None), use the name as-is since it's
        // already the correct relative path computed by the transitive resolver
        // For direct dependencies (manifest_alias=Some), normalize the path
        let canonical_name = self.compute_local_canonical_name(name, dep, &manifest_alias)?;

        let variant_inputs = lockfile_builder::VariantInputs::new(
            lockfile_builder::build_merged_variant_inputs(self.core.manifest(), dep),
        );

        // Determine if this is a private dependency
        // Use the original manifest name (not canonical name) for the lookup
        let is_private = manifest_alias.as_ref().is_some_and(|alias| {
            self.core.manifest().is_private_dependency(&resource_type.to_string(), alias)
        });

        // Transform path for private dependencies
        let final_installed_at = if is_private {
            install_path_resolver::transform_path_for_private(&installed_at)
        } else {
            installed_at
        };

        Ok(LockedResource {
            name: canonical_name,
            source: None,
            url: None,
            path: normalize_path_for_storage(dep.get_path()),
            version: None,
            resolved_commit: None,
            checksum: String::new(),
            installed_at: final_installed_at,
            dependencies: self.get_dependencies_for(
                name,
                None,
                resource_type,
                Some(&artifact_type_string),
                variant_inputs.hash(),
            ),
            resource_type,
            tool: Some(artifact_type_string),
            manifest_alias,
            applied_patches,
            install: dep.get_install(),
            variant_inputs,
            context_checksum: None,
            is_private,
        })
    }

    /// Compute canonical name for local dependencies.
    ///
    /// Transitive: returns name as-is. Direct: normalizes path relative to manifest.
    pub(super) fn compute_local_canonical_name(
        &self,
        name: &str,
        dep: &ResourceDependency,
        manifest_alias: &Option<String>,
    ) -> Result<String> {
        if manifest_alias.is_none() {
            // Transitive dependency - name is already correct (e.g., "../snippets/agents/backend-engineer")
            Ok(name.to_string())
        } else if let Some(manifest_dir) = self.core.manifest().manifest_dir.as_ref() {
            // Direct dependency - normalize path relative to manifest
            let full_path = if Path::new(dep.get_path()).is_absolute() {
                PathBuf::from(dep.get_path())
            } else {
                manifest_dir.join(dep.get_path())
            };

            // Normalize the path to handle ../ and ./ components deterministically
            let canonical_path = crate::utils::fs::normalize_path(&full_path);

            let source_context = SourceContext::local(manifest_dir);
            Ok(generate_dependency_name(&canonical_path.to_string_lossy(), &source_context))
        } else {
            // Fallback to name if manifest_dir is not available
            Ok(name.to_string())
        }
    }

    /// Resolve Git-based dependency to locked resource.
    pub(super) async fn resolve_git_dependency(
        &self,
        name: &str,
        dep: &ResourceDependency,
        resource_type: ResourceType,
    ) -> Result<LockedResource> {
        use crate::utils::normalize_path_for_storage;

        let source_name = dep
            .get_source()
            .ok_or_else(|| anyhow::anyhow!("Dependency '{}' has no source specified", name))?;

        // Generate canonical name using remote source context
        let source_context = SourceContext::remote(source_name);
        let canonical_name = generate_dependency_name(dep.get_path(), &source_context);

        let source_url = self
            .core
            .source_manager()
            .get_source_url(source_name)
            .ok_or_else(|| anyhow::anyhow!("Source '{}' not found", source_name))?;

        let version_key = dep.get_version().map_or_else(|| "HEAD".to_string(), |v| v.to_string());
        let group_key = format!("{}::{}", source_name, version_key);

        let prepared = self.version_service.get_prepared_version(&group_key).ok_or_else(|| {
            anyhow::anyhow!(
                "Prepared state missing for source '{}' @ '{}'",
                source_name,
                version_key
            )
        })?;

        let filename = Self::resolve_filename(dep);
        let artifact_type_string = self.resolve_tool(dep, resource_type);
        let artifact_type = artifact_type_string.as_str();

        let installed_at = install_path_resolver::resolve_install_path(
            self.core.manifest(),
            dep,
            artifact_type,
            resource_type,
            &filename,
        )?;

        let manifest_alias = self.resolve_manifest_alias(name, resource_type);

        let applied_patches = lockfile_builder::get_patches_for_resource(
            self.core.manifest(),
            resource_type,
            name,
            manifest_alias.as_deref(),
        );

        let variant_inputs = lockfile_builder::VariantInputs::new(
            lockfile_builder::build_merged_variant_inputs(self.core.manifest(), dep),
        );

        // Extract data from prepared before storing variant_inputs
        let resolved_version = prepared.resolved_version.clone();
        let resolved_commit = prepared.resolved_commit.clone();

        // Store variant_inputs in PreparedSourceVersion for backtracking
        // DashMap allows concurrent inserts, so we don't need mutable access
        let resource_id = format!("{}:{}", source_name, dep.get_path());
        prepared.resource_variants.insert(resource_id, Some(variant_inputs.json().clone()));

        // Determine if this is a private dependency
        let is_private = manifest_alias.as_ref().is_some_and(|alias| {
            self.core.manifest().is_private_dependency(&resource_type.to_string(), alias)
        });

        // Transform path for private dependencies
        let final_installed_at = if is_private {
            install_path_resolver::transform_path_for_private(&installed_at)
        } else {
            installed_at
        };

        Ok(LockedResource {
            name: canonical_name,
            source: Some(source_name.to_string()),
            url: Some(source_url.clone()),
            path: normalize_path_for_storage(dep.get_path()),
            version: resolved_version,
            resolved_commit: Some(resolved_commit),
            checksum: String::new(),
            installed_at: final_installed_at,
            dependencies: self.get_dependencies_for(
                name,
                Some(source_name),
                resource_type,
                Some(&artifact_type_string),
                variant_inputs.hash(),
            ),
            resource_type,
            tool: Some(artifact_type_string),
            manifest_alias,
            applied_patches,
            install: dep.get_install(),
            variant_inputs,
            context_checksum: None,
            is_private,
        })
    }

    /// Resolve pattern dependency to multiple locked resources.
    ///
    /// Delegates to local or Git pattern resolvers.
    pub(super) async fn resolve_pattern_dependency(
        &self,
        name: &str,
        dep: &ResourceDependency,
        resource_type: ResourceType,
    ) -> Result<Vec<LockedResource>> {
        if !dep.is_pattern() {
            return Err(anyhow::anyhow!(
                "Expected pattern dependency but no glob characters found in path"
            ));
        }

        if dep.is_local() {
            self.resolve_local_pattern(name, dep, resource_type)
        } else {
            self.resolve_git_pattern(name, dep, resource_type).await
        }
    }

    /// Resolve local pattern dependency to multiple locked resources.
    pub(super) fn resolve_local_pattern(
        &self,
        name: &str,
        dep: &ResourceDependency,
        resource_type: ResourceType,
    ) -> Result<Vec<LockedResource>> {
        use crate::pattern::PatternResolver;

        let pattern = dep.get_path();
        let (base_path, pattern_str) = install_path_resolver::parse_pattern_base_path(pattern);
        let pattern_resolver = PatternResolver::new();
        let matches = pattern_resolver.resolve(&pattern_str, &base_path)?;

        let artifact_type_string = self.resolve_tool(dep, resource_type);
        let artifact_type = artifact_type_string.as_str();

        // Compute variant inputs once for all matched files in the pattern
        let variant_inputs = lockfile_builder::VariantInputs::new(
            lockfile_builder::build_merged_variant_inputs(self.core.manifest(), dep),
        );

        // Determine if this pattern is a private dependency
        let is_private =
            self.core.manifest().is_private_dependency(&resource_type.to_string(), name);

        let mut resources = Vec::new();
        for matched_path in matches {
            let resource_name = crate::pattern::extract_resource_name(&matched_path);
            let full_relative_path =
                install_path_resolver::construct_full_relative_path(&base_path, &matched_path);
            let filename =
                install_path_resolver::extract_pattern_filename(&base_path, &matched_path);

            let installed_at = install_path_resolver::resolve_install_path(
                self.core.manifest(),
                dep,
                artifact_type,
                resource_type,
                &filename,
            )?;

            // Transform path for private dependencies
            let final_installed_at = if is_private {
                install_path_resolver::transform_path_for_private(&installed_at)
            } else {
                installed_at
            };

            resources.push(LockedResource {
                name: resource_name.clone(),
                source: None,
                url: None,
                path: full_relative_path,
                version: None,
                resolved_commit: None,
                checksum: String::new(),
                installed_at: final_installed_at,
                dependencies: vec![],
                resource_type,
                tool: Some(artifact_type_string.clone()),
                manifest_alias: Some(name.to_string()),
                applied_patches: lockfile_builder::get_patches_for_resource(
                    self.core.manifest(),
                    resource_type,
                    &resource_name, // Use canonical resource name
                    Some(name),     // Use manifest_alias for patch lookups
                ),
                install: dep.get_install(),
                variant_inputs: variant_inputs.clone(),
                context_checksum: None,
                is_private,
            });
        }

        Ok(resources)
    }

    /// Resolve Git-based pattern dependency to multiple locked resources.
    pub(super) async fn resolve_git_pattern(
        &self,
        name: &str,
        dep: &ResourceDependency,
        resource_type: ResourceType,
    ) -> Result<Vec<LockedResource>> {
        use crate::pattern::PatternResolver;
        use crate::utils::{compute_relative_install_path, normalize_path_for_storage};

        let pattern = dep.get_path();
        let pattern_name = name;

        let source_name = dep.get_source().ok_or_else(|| {
            anyhow::anyhow!("Pattern dependency '{}' has no source specified", name)
        })?;

        let source_url = self
            .core
            .source_manager()
            .get_source_url(source_name)
            .ok_or_else(|| anyhow::anyhow!("Source '{}' not found", source_name))?;

        let version_key = dep.get_version().map_or_else(|| "HEAD".to_string(), |v| v.to_string());
        let group_key = format!("{}::{}", source_name, version_key);

        let prepared = self.version_service.get_prepared_version(&group_key).ok_or_else(|| {
            anyhow::anyhow!(
                "Prepared state missing for source '{}' @ '{}'",
                source_name,
                version_key
            )
        })?;

        // Extract data from prepared before mutable borrow (needed for loop)
        let worktree_path = prepared.worktree_path.clone();
        let resolved_version = prepared.resolved_version.clone();
        let resolved_commit = prepared.resolved_commit.clone();

        let repo_path = Path::new(&worktree_path);
        let pattern_resolver = PatternResolver::new();
        let matches = pattern_resolver.resolve(pattern, repo_path)?;

        let artifact_type_string = self.resolve_tool(dep, resource_type);
        let artifact_type = artifact_type_string.as_str();

        // Compute variant inputs once for all matched files in the pattern
        let variant_inputs = lockfile_builder::VariantInputs::new(
            lockfile_builder::build_merged_variant_inputs(self.core.manifest(), dep),
        );

        // Determine if this pattern is a private dependency
        let is_private =
            self.core.manifest().is_private_dependency(&resource_type.to_string(), pattern_name);

        let mut resources = Vec::new();
        for matched_path in matches {
            let resource_name = crate::pattern::extract_resource_name(&matched_path);

            // Compute installation path
            let installed_at = match resource_type {
                ResourceType::Hook | ResourceType::McpServer => {
                    install_path_resolver::resolve_merge_target_path(
                        self.core.manifest(),
                        artifact_type,
                        resource_type,
                    )
                }
                _ => {
                    let artifact_path = self
                        .core
                        .manifest()
                        .get_artifact_resource_path(artifact_type, resource_type)
                        .ok_or_else(|| {
                            anyhow::anyhow!(
                                "Resource type '{}' is not supported by tool '{}'",
                                resource_type,
                                artifact_type
                            )
                        })?;

                    let dep_flatten = dep.get_flatten();
                    let tool_flatten = self
                        .core
                        .manifest()
                        .get_tool_config(artifact_type)
                        .and_then(|config| config.resources.get(resource_type.to_plural()))
                        .and_then(|resource_config| resource_config.flatten);

                    let flatten = dep_flatten.or(tool_flatten).unwrap_or(false);

                    let base_target = if let Some(custom_target) = dep.get_target() {
                        // Strip leading path separators (both Unix and Windows) to ensure relative path
                        PathBuf::from(artifact_path.display().to_string())
                            .join(custom_target.trim_start_matches(['/', '\\']))
                    } else {
                        artifact_path.to_path_buf()
                    };

                    let filename = repo_path.join(&matched_path).to_string_lossy().to_string();
                    let relative_path =
                        compute_relative_install_path(&base_target, Path::new(&filename), flatten);
                    // Convert directly to Unix format for lockfile storage (forward slashes only)
                    normalize_path_for_storage(base_target.join(relative_path))
                }
            };

            // Store variant_inputs in PreparedSourceVersion for backtracking
            // DashMap allows concurrent inserts, so we access through regular get()
            let resource_id = format!("{}:{}", source_name, matched_path.to_string_lossy());
            if let Some(prepared_ref) = self.version_service.get_prepared_version(&group_key) {
                prepared_ref
                    .resource_variants
                    .insert(resource_id, Some(variant_inputs.json().clone()));
            }

            // Transform path for private dependencies
            let final_installed_at = if is_private {
                install_path_resolver::transform_path_for_private(&installed_at)
            } else {
                installed_at
            };

            resources.push(LockedResource {
                name: resource_name.clone(),
                source: Some(source_name.to_string()),
                url: Some(source_url.clone()),
                path: normalize_path_for_storage(matched_path.to_string_lossy().to_string()),
                version: resolved_version.clone(),
                resolved_commit: Some(resolved_commit.clone()),
                checksum: String::new(),
                installed_at: final_installed_at,
                dependencies: vec![],
                resource_type,
                tool: Some(artifact_type_string.clone()),
                manifest_alias: Some(pattern_name.to_string()),
                applied_patches: lockfile_builder::get_patches_for_resource(
                    self.core.manifest(),
                    resource_type,
                    &resource_name,     // Use canonical resource name
                    Some(pattern_name), // Use manifest_alias for patch lookups
                ),
                install: dep.get_install(),
                variant_inputs: variant_inputs.clone(),
                context_checksum: None,
                is_private,
            });
        }

        Ok(resources)
    }
}

/// Helpers for dependency resolution context.
impl ResolutionCore {
    /// Get the manifest directory for resolving relative paths.
    pub fn manifest_dir(&self) -> Option<&std::path::Path> {
        self.manifest().manifest_dir.as_deref()
    }
}
