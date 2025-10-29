//! Dependency handling for template context building.
//!
//! This module provides functionality for extracting dependency information,
//! custom names, and building the dependency data structure for template rendering.

pub mod builders;
pub mod extractors;

// Re-export the main trait and helper functions for external use
pub(crate) use builders::build_dependencies_data;
pub(crate) use extractors::DependencyExtractor;
