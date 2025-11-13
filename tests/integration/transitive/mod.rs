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
//! - Checksum-based conflict detection for local dependencies
//! - Parallel processing and concurrent operations

mod basic;
mod checksum_conflicts;
mod complex;
mod cross_type;
mod install_false_conflicts;
mod local;
mod merged;
mod overrides;
mod parallel_processing_tests;
mod patterns;
mod tool_inheritance;
mod version_conflicts;
mod versions;
