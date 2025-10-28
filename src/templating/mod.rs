//! Markdown templating engine for AGPM resources.
//!
//! This module provides Tera-based templating functionality for Markdown resources,
//! enabling dynamic content generation during installation. It supports safe, sandboxed
//! template rendering with a rich context containing installation metadata.
//!
//! # Overview
//!
//! The templating system allows resource authors to:
//! - Reference other resources by name and type
//! - Access resolved installation paths and versions
//! - Use conditional logic and loops in templates
//! - Read and embed project-specific files (style guides, best practices, etc.)
//!
//! # Template Context
//!
//! Templates are rendered with a structured context containing:
//! - `agpm.resource`: Current resource information (name, type, install path, etc.)
//! - `agpm.deps`: Nested dependency information by resource type and name
//!
//! # Custom Filters
//!
//! - `content`: Read project-specific files (e.g., `{{ 'docs/guide.md' | content }}`)
//!
//! # Syntax Restrictions
//!
//! For security and safety, the following Tera features are disabled:
//! - `{% include %}` tags (no file system access)
//! - `{% extends %}` tags (no template inheritance)
//! - `{% import %}` tags (no external template imports)
//! - Custom functions that access the file system or network (except content filter)
//!
//! # Supported Features
//!
//! - Variable substitution: `{{ agpm.resource.install_path }}`
//! - Conditional logic: `{% if agpm.resource.source %}...{% endif %}`
//! - Loops: `{% for name, dep in agpm.deps.agents %}...{% endfor %}`
//! - Standard Tera filters (string manipulation, formatting)
//! - Project file embedding: `{{ 'path/to/file.md' | content }}`
//! - Literal blocks: Protect template syntax from rendering for documentation

// Module declarations
pub mod cache;
pub mod content;
pub mod context;
pub mod dependencies;
pub mod error;
pub mod filters;
pub mod renderer;
pub mod utils;

#[cfg(test)]
mod renderer_tests;

// Re-exports for public API
pub use context::{DependencyData, ResourceMetadata, TemplateContextBuilder};
pub use renderer::{DependencyChainEntry, RenderingMetadata, TemplateRenderer};
pub use utils::{deep_merge_json, to_native_path_display};
