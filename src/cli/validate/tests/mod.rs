//! Tests for the validate command module.
//!
//! Tests are organized into logical groups:
//! - `command_tests`: Command execution and CLI behavior
//! - `format_tests`: Output formatting (JSON, text, verbose, quiet, strict)
//! - `integration_tests`: End-to-end scenarios and complex interactions
//! - `lockfile_resolve_tests`: Lockfile validation and dependency resolution
//! - `path_source_tests`: Path validation, source checking, and file references

mod command_tests;
mod format_tests;
mod integration_tests;
mod lockfile_resolve_tests;
mod path_source_tests;
