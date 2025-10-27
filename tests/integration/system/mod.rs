//! System-level and infrastructure tests
//!
//! Tests for system-level functionality:
//! - Cache and worktree management
//! - Cross-platform compatibility (Windows, macOS, Linux)
//! - file:// URL support
//! - Parallelism and concurrency control
//! - .gitignore management
//! - Error handling and edge cases

mod cache;
mod cross_platform;
mod errors;
mod file_url;
mod gitignore;
mod parallelism;
