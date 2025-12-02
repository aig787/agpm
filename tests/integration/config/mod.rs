//! Configuration and tool management tests
//!
//! Tests for configuration functionality:
//! - Claude Code hooks integration
//! - Patch/override functionality
//! - Tool enable/disable management
//! - Version conflict detection

mod conflicts;
mod conflicts_backtracking;
mod hooks;
mod patches;
mod private_deps;
mod test_template_vars_storage;
mod tools;
