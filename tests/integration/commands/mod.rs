//! CLI command tests
//!
//! Tests for AGPM CLI commands:
//! - List command functionality
//! - Dependency tree visualization
//! - Validation command
//! - Self-upgrade functionality
//! - Migration command (CCPM → AGPM, gitignore format)

mod list;
mod migrate;
mod tree;
mod upgrade;
mod validate;
