//! Installation context and helper utilities.
//!
//! This module provides the [`InstallContext`] type and its builder for managing
//! installation parameters throughout the AGPM installation pipeline.
//!
//! # Cross-Process Safety
//!
//! Cross-process coordination (e.g., gitignore updates) is handled at the command
//! level via `ProjectLock`. This context no longer carries mutex fields.
//!
//! # Examples
//!
//! Basic usage with the builder pattern:
//!
//! ```rust,no_run
//! use agpm_cli::installer::InstallContext;
//! use agpm_cli::cache::Cache;
//! use std::path::Path;
//!
//! # async fn example() -> anyhow::Result<()> {
//! let project_dir = Path::new(".");
//! let cache = Cache::new()?;
//!
//! // Create a basic context
//! let context = InstallContext::builder(&project_dir, &cache)
//!     .force_refresh(true)
//!     .verbose(false)
//!     .build();
//!
//! // With manifest and lockfile
//! # use agpm_cli::manifest::Manifest;
//! # use agpm_cli::lockfile::LockFile;
//! # use std::sync::Arc;
//! # let manifest = Manifest::default();
//! # let lockfile = Arc::new(LockFile::default());
//! let context = InstallContext::builder(&project_dir, &cache)
//!     .manifest(&manifest)
//!     .lockfile(&lockfile)
//!     .force_refresh(false)
//!     .build();
//! # Ok(())
//! # }
//! ```

use std::path::Path;
use std::sync::Arc;

use crate::cache::Cache;
use crate::lockfile::LockFile;
use crate::manifest::Manifest;

/// Installation context containing common parameters for resource installation.
///
/// This struct bundles frequently-used installation parameters to reduce
/// function parameter counts and improve code readability. It's used throughout
/// the installation pipeline to pass configuration and context information.
///
/// # Fields
///
/// * `project_dir` - Root directory of the project where resources will be installed
/// * `cache` - Cache instance for managing Git repositories and worktrees
/// * `force_refresh` - Whether to force refresh of cached worktrees
/// * `manifest` - Optional reference to the project manifest for template context
/// * `lockfile` - Optional reference to the lockfile for template context
/// * `old_lockfile` - Optional reference to the previous lockfile for early-exit optimization
/// * `project_patches` - Optional project-level patches from agpm.toml
/// * `private_patches` - Optional user-level patches from agpm.private.toml
pub struct InstallContext<'a> {
    pub project_dir: &'a Path,
    pub cache: &'a Cache,
    pub force_refresh: bool,
    pub verbose: bool,
    pub manifest: Option<&'a Manifest>,
    pub lockfile: Option<&'a Arc<LockFile>>,
    pub old_lockfile: Option<&'a LockFile>,
    pub project_patches: Option<&'a crate::manifest::ManifestPatches>,
    pub private_patches: Option<&'a crate::manifest::ManifestPatches>,
    pub max_content_file_size: Option<u64>,
    /// Shared template context builder for all resources
    pub template_context_builder: Arc<crate::templating::TemplateContextBuilder>,
    /// Trust lockfile checksums without recomputing (ultra-fast path optimization).
    ///
    /// When enabled and all inputs match the old lockfile entry, skip file I/O and
    /// return the stored checksum. Safe for immutable dependencies (tags/SHAs).
    ///
    /// See module-level docs in [`crate::cli::install`] for optimization tier details.
    pub trust_lockfile_checksums: bool,
}

/// Builder for creating InstallContext instances with a fluent API.
pub struct InstallContextBuilder<'a> {
    // Required parameters
    project_dir: &'a Path,
    cache: &'a Cache,

    // Optional with sensible defaults
    force_refresh: bool,
    verbose: bool,
    trust_lockfile_checksums: bool,

    // Truly optional parameters
    manifest: Option<&'a Manifest>,
    lockfile: Option<&'a Arc<LockFile>>,
    old_lockfile: Option<&'a LockFile>,
    project_patches: Option<&'a crate::manifest::ManifestPatches>,
    private_patches: Option<&'a crate::manifest::ManifestPatches>,
    max_content_file_size: Option<u64>,
}

impl<'a> InstallContextBuilder<'a> {
    /// Create a new builder with required parameters.
    pub fn new(project_dir: &'a Path, cache: &'a Cache) -> Self {
        Self {
            project_dir,
            cache,
            force_refresh: false,
            verbose: false,
            trust_lockfile_checksums: false,
            manifest: None,
            lockfile: None,
            old_lockfile: None,
            project_patches: None,
            private_patches: None,
            max_content_file_size: None,
        }
    }

    /// Set whether to force refresh of cached worktrees.
    pub fn force_refresh(mut self, value: bool) -> Self {
        self.force_refresh = value;
        self
    }

    /// Set verbose output.
    pub fn verbose(mut self, value: bool) -> Self {
        self.verbose = value;
        self
    }

    /// Trust lockfile checksums without recomputing (fast path optimization).
    ///
    /// When enabled, if a file exists and all inputs match the old lockfile,
    /// we return the stored checksum without reading/hashing the file.
    pub fn trust_lockfile_checksums(mut self, value: bool) -> Self {
        self.trust_lockfile_checksums = value;
        self
    }

    /// Set the project manifest for template context.
    pub fn manifest(mut self, manifest: &'a Manifest) -> Self {
        self.manifest = Some(manifest);
        self
    }

    /// Set the lockfile for template context.
    pub fn lockfile(mut self, lockfile: &'a Arc<LockFile>) -> Self {
        self.lockfile = Some(lockfile);
        self
    }

    /// Set the previous lockfile for early-exit optimization.
    pub fn old_lockfile(mut self, old_lockfile: &'a LockFile) -> Self {
        self.old_lockfile = Some(old_lockfile);
        self
    }

    /// Set project-level patches from agpm.toml.
    pub fn project_patches(mut self, patches: &'a crate::manifest::ManifestPatches) -> Self {
        self.project_patches = Some(patches);
        self
    }

    /// Set user-level patches from agpm.private.toml.
    pub fn private_patches(mut self, patches: &'a crate::manifest::ManifestPatches) -> Self {
        self.private_patches = Some(patches);
        self
    }

    /// Set maximum content file size for embedding.
    pub fn max_content_file_size(mut self, size: u64) -> Self {
        self.max_content_file_size = Some(size);
        self
    }

    /// Set commonly used options in a single call.
    ///
    /// This method groups frequently used options to reduce the number of
    /// builder method calls in common installation scenarios.
    ///
    /// # Arguments
    ///
    /// * `force_refresh` - Whether to force refresh cached worktrees
    /// * `verbose` - Whether to enable verbose output
    /// * `manifest` - Optional project manifest
    /// * `lockfile` - Optional lockfile for template context
    pub fn with_common_options(
        mut self,
        force_refresh: bool,
        verbose: bool,
        manifest: Option<&'a Manifest>,
        lockfile: Option<&'a Arc<LockFile>>,
    ) -> Self {
        self.force_refresh = force_refresh;
        self.verbose = verbose;
        self.manifest = manifest;
        self.lockfile = lockfile;
        self
    }

    /// Build the InstallContext with the configured parameters.
    #[must_use] // The context is needed for installation, ignoring it defeats the purpose
    pub fn build(self) -> InstallContext<'a> {
        // Create shared template context builder
        // Use lockfile if available, otherwise create with empty lockfile
        let (lockfile_for_builder, project_config) = if let Some(lf) = self.lockfile {
            (lf.clone(), self.manifest.and_then(|m| m.project.clone()))
        } else {
            // No lockfile - create an empty one for the builder
            (Arc::new(LockFile::default()), None)
        };

        // Clone cache and wrap in Arc for TemplateContextBuilder
        // The clone is necessary because we have &Cache but need Arc<Cache>
        // Cache cloning is relatively cheap (Arc'd internals) and only happens once per installation
        let template_context_builder = Arc::new(crate::templating::TemplateContextBuilder::new(
            lockfile_for_builder,
            project_config,
            Arc::new(self.cache.clone()),
            self.project_dir.to_path_buf(),
        ));

        InstallContext {
            project_dir: self.project_dir,
            cache: self.cache,
            force_refresh: self.force_refresh,
            verbose: self.verbose,
            manifest: self.manifest,
            lockfile: self.lockfile,
            old_lockfile: self.old_lockfile,
            project_patches: self.project_patches,
            private_patches: self.private_patches,
            max_content_file_size: self.max_content_file_size,
            template_context_builder,
            trust_lockfile_checksums: self.trust_lockfile_checksums,
        }
    }
}

impl<'a> InstallContext<'a> {
    /// Create a new builder for InstallContext.
    pub fn builder(project_dir: &'a Path, cache: &'a Cache) -> InstallContextBuilder<'a> {
        InstallContextBuilder::new(project_dir, cache)
    }

    /// Create an InstallContext with common options for parallel installation.
    ///
    /// This helper function reduces code duplication by handling the common pattern
    /// of setting up InstallContext with frequently used options.
    ///
    /// # Arguments
    ///
    /// * `project_dir` - Root directory of the project
    /// * `cache` - Cache instance for managing Git repositories
    /// * `manifest` - Optional project manifest
    /// * `lockfile` - Lockfile for template context
    /// * `force_refresh` - Whether to force refresh cached worktrees
    /// * `verbose` - Whether to enable verbose output
    /// * `old_lockfile` - Optional previous lockfile for early-exit optimization
    pub fn with_common_options(
        project_dir: &'a Path,
        cache: &'a Cache,
        manifest: Option<&'a Manifest>,
        lockfile: Option<&'a Arc<LockFile>>,
        force_refresh: bool,
        verbose: bool,
        old_lockfile: Option<&'a LockFile>,
    ) -> Self {
        Self::with_common_options_and_trust(
            project_dir,
            cache,
            manifest,
            lockfile,
            force_refresh,
            verbose,
            old_lockfile,
            false, // trust_lockfile_checksums defaults to false
        )
    }

    /// Create an InstallContext with common options including trust flag.
    ///
    /// This is the full version that allows specifying `trust_lockfile_checksums`.
    #[allow(clippy::too_many_arguments)]
    pub fn with_common_options_and_trust(
        project_dir: &'a Path,
        cache: &'a Cache,
        manifest: Option<&'a Manifest>,
        lockfile: Option<&'a Arc<LockFile>>,
        force_refresh: bool,
        verbose: bool,
        old_lockfile: Option<&'a LockFile>,
        trust_lockfile_checksums: bool,
    ) -> Self {
        let mut builder = Self::builder(project_dir, cache)
            .force_refresh(force_refresh)
            .verbose(verbose)
            .trust_lockfile_checksums(trust_lockfile_checksums);

        // Add optional fields only if present
        if let Some(m) = manifest {
            builder = builder.manifest(m);
            // Add patches from manifest if available
            if !m.project_patches.is_empty() {
                builder = builder.project_patches(&m.project_patches);
            }
            if !m.private_patches.is_empty() {
                builder = builder.private_patches(&m.private_patches);
            }
        }

        if let Some(lf) = lockfile {
            builder = builder.lockfile(lf);
        }

        if let Some(old_lf) = old_lockfile {
            builder = builder.old_lockfile(old_lf);
        }

        builder.build()
    }
}
