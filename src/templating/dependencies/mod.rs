//! Dependency handling for template context building.
//!
//! This module provides a two-phase architecture for resolving and building
//! dependency data structures used in template rendering:
//!
//! # Architecture
//!
//! ## Extractors (`extractors.rs`)
//!
//! The extractor submodule handles **parsing and extraction** of dependency metadata
//! from resource files:
//!
//! - **`DependencyExtractor` trait**: Core abstraction for accessing lockfile data,
//!   caches, and project configuration
//! - **`extract_dependency_custom_names()`**: Parses frontmatter to extract custom
//!   alias names for dependencies (e.g., `name: my_helper` in YAML frontmatter)
//! - **`extract_dependency_specs()`**: Extracts complete `DependencySpec` objects
//!   including tool, flatten, and install fields
//! - **Caching**: Uses `custom_names_cache` and `dependency_specs_cache` to avoid
//!   repeated file I/O and parsing operations
//!
//! ## Builders (`builders.rs`)
//!
//! The builder submodule handles **data structure construction and rendering**:
//!
//! - **`build_dependencies_data()`**: Orchestrates dependency resolution, content
//!   rendering, and context building for templates
//! - **`add_custom_alias()`**: Mutates dependency maps to add custom name aliases
//! - **Rendering**: Handles recursive template rendering with cycle detection
//! - **Cache Management**: Manages render cache for already-processed dependencies
//!
//! # Separation of Concerns
//!
//! The split between extraction and building serves several purposes:
//!
//! 1. **Performance**: Extraction results are cached, avoiding repeated file I/O
//! 2. **Clarity**: Parsing logic (extractors) is separate from data structure
//!    construction (builders)
//! 3. **Testability**: Each phase can be tested independently
//! 4. **File Size**: Keeps each module under 650 lines vs 1200+ line monolith
//!
//! # Usage
//!
//! This module is primarily used by [`crate::templating::context::TemplateContextBuilder`],
//! which implements the `DependencyExtractor` trait and delegates to these submodules:
//!
//! ```text
//! TemplateContextBuilder
//!   ├─> extract_dependency_custom_names()  [extractors.rs]
//!   ├─> extract_dependency_specs()         [extractors.rs]
//!   └─> build_dependencies_data()          [builders.rs]
//!         └─> renders dependencies recursively
//! ```
//!
//! # Caching Strategy
//!
//! Two levels of caching improve performance:
//!
//! - **Custom Names Cache**: Maps `ResourceId` → `BTreeMap<dep_ref, custom_name>`
//!   - Avoids re-parsing frontmatter for custom name extraction
//!   - Invalidated when resource content changes (via checksum)
//!
//! - **Dependency Specs Cache**: Maps `ResourceId` → `BTreeMap<dep_ref, DependencySpec>`
//!   - Avoids re-parsing frontmatter for full spec extraction
//!   - Includes tool, flatten, install, and template_vars fields
//!
//! - **Render Cache**: Maps `RenderCacheKey` → `String`
//!   - Avoids re-rendering already-processed dependency content
//!   - Includes tool, commit hash, and variant hash in cache key
//!
//! # Example
//!
//! ```no_run
//! use crate::templating::dependencies::DependencyExtractor;
//!
//! // Typically used via TemplateContextBuilder which implements DependencyExtractor
//! // The builder coordinates extraction and building:
//! //
//! // 1. Extract custom names from frontmatter (cached)
//! // 2. Extract dependency specs from frontmatter (cached)
//! // 3. Build dependency data structure with rendered content
//! // 4. Add custom name aliases to the final map
//! ```

pub mod builders;
pub mod extractors;

// Re-export the main trait and helper functions for external use
pub(crate) use builders::build_dependencies_data;
pub(crate) use extractors::DependencyExtractor;
