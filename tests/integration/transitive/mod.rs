//! Transitive dependency resolution tests
//!
//! Tests for transitive dependency resolution across various scenarios:
//! - Basic transitive dependency chains
//! - Pattern expansion in transitive dependencies
//! - Local file transitive dependencies
//! - Version conflict resolution
//! - Cross-type and cross-source dependencies
//! - Complex dependency graphs (diamond patterns, cycles)
//! - Dependency merging and deduplication
//! - Direct dependencies overriding transitive ones

mod basic;
mod complex;
mod cross_type;
mod local;
mod merged;
mod overrides;
mod patterns;
mod tool_inheritance;
mod versions;
