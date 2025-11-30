//! Integration tests for Claude Skills functionality.
//!
//! These tests verify that skills work correctly with the full AGPM workflow
//! including installation, dependency resolution, patching, and validation.
//!
//! # Test Organization
//!
//! - **basic**: Basic skill installation (single, with patches, patterns)
//! - **lifecycle**: Skill lifecycle tests (transitive deps, validation, list, remove)
//! - **errors**: Error scenario tests (missing files, invalid frontmatter, etc.)
//! - **security**: Security-related tests (size limits, symlinks, path traversal)
//! - **advanced**: Advanced tests (template vars, install:false, circular deps)

mod advanced;
mod basic;
mod errors;
mod lifecycle;
mod security;
