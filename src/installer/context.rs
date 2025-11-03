//! Installation context and helper utilities.

use anyhow::Result;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;

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
    pub gitignore_lock: Option<&'a Arc<Mutex<()>>>,
    pub max_content_file_size: Option<u64>,
    /// Shared template context builder for all resources
    pub template_context_builder: Arc<crate::templating::TemplateContextBuilder>,
}

/// Builder for creating InstallContext instances with a fluent API.
pub struct InstallContextBuilder<'a> {
    // Required parameters
    project_dir: &'a Path,
    cache: &'a Cache,

    // Optional with sensible defaults
    force_refresh: bool,
    verbose: bool,

    // Truly optional parameters
    manifest: Option<&'a Manifest>,
    lockfile: Option<&'a Arc<LockFile>>,
    old_lockfile: Option<&'a LockFile>,
    project_patches: Option<&'a crate::manifest::ManifestPatches>,
    private_patches: Option<&'a crate::manifest::ManifestPatches>,
    gitignore_lock: Option<&'a Arc<Mutex<()>>>,
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
            manifest: None,
            lockfile: None,
            old_lockfile: None,
            project_patches: None,
            private_patches: None,
            gitignore_lock: None,
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

    /// Set gitignore lock for coordinating gitignore updates.
    pub fn gitignore_lock(mut self, lock: &'a Arc<Mutex<()>>) -> Self {
        self.gitignore_lock = Some(lock);
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
    /// * `gitignore_lock` - Optional lock for gitignore coordination
    pub fn with_common_options(
        mut self,
        force_refresh: bool,
        verbose: bool,
        manifest: Option<&'a Manifest>,
        lockfile: Option<&'a Arc<LockFile>>,
        gitignore_lock: Option<&'a Arc<Mutex<()>>>,
    ) -> Self {
        self.force_refresh = force_refresh;
        self.verbose = verbose;
        self.manifest = manifest;
        self.lockfile = lockfile;
        self.gitignore_lock = gitignore_lock;
        self
    }

    /// Set gitignore lock for coordinating gitignore updates.
    /// If None, gitignore lock is not set.
    pub fn gitignore_lock_option(mut self, gitignore_lock: Option<&'a Arc<Mutex<()>>>) -> Self {
        self.gitignore_lock = gitignore_lock;
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
            gitignore_lock: self.gitignore_lock,
            max_content_file_size: self.max_content_file_size,
            template_context_builder,
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
    /// * `gitignore_lock` - Optional lock for gitignore coordination
    /// * `old_lockfile` - Optional previous lockfile for early-exit optimization
    pub fn with_common_options(
        project_dir: &'a Path,
        cache: &'a Cache,
        manifest: Option<&'a Manifest>,
        lockfile: Option<&'a Arc<LockFile>>,
        force_refresh: bool,
        verbose: bool,
        gitignore_lock: Option<&'a Arc<Mutex<()>>>,
        old_lockfile: Option<&'a LockFile>,
    ) -> Self {
        let mut builder = Self::builder(project_dir, cache)
            .force_refresh(force_refresh)
            .verbose(verbose)
            .gitignore_lock_option(gitignore_lock);

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

/// Read a file with retry logic to handle cross-process filesystem cache coherency issues.
///
/// This function wraps `tokio::fs::read_to_string` with retry logic to handle cases where
/// files created by Git subprocesses are not immediately visible to the parent Rust process
/// due to filesystem cache propagation delays. This is particularly important in CI
/// environments with network-attached storage where cache coherency delays can be significant.
///
/// # Arguments
///
/// * `path` - The file path to read
///
/// # Returns
///
/// Returns the file content as a `String`, or an error if the file cannot be read after retries.
///
/// # Retry Strategy
///
/// - Initial delay: 10ms
/// - Max delay: 500ms
/// - Factor: 2x (exponential backoff)
/// - Max attempts: 10
/// - Total max time: ~10 seconds
///
/// Only `NotFound` errors are retried, as these indicate cache coherency issues.
/// Other errors (permissions, I/O errors) fail immediately by returning Ok to bypass retry.
pub(crate) async fn read_with_cache_retry(path: &Path) -> Result<String> {
    use std::io;

    let retry_strategy = tokio_retry::strategy::ExponentialBackoff::from_millis(10)
        .max_delay(Duration::from_millis(500))
        .factor(2)
        .take(10);

    let path_buf = path.to_path_buf();

    tokio_retry::Retry::spawn(retry_strategy, || {
        let path = path_buf.clone();
        async move {
            tokio::fs::read_to_string(&path).await.map_err(|e| {
                if e.kind() == io::ErrorKind::NotFound {
                    tracing::debug!(
                        "File not yet visible (likely cache coherency issue): {}",
                        path.display()
                    );
                    format!("File not found: {}", path.display())
                } else {
                    // Non-retriable error - return error message that will fail fast
                    format!("I/O error (non-retriable): {}", e)
                }
            })
        }
    })
    .await
    .map_err(|e| anyhow::anyhow!("Failed to read resource file: {}: {}", path.display(), e))
}
