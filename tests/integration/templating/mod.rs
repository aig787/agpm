//! Template rendering tests
//!
//! Tests for template rendering functionality:
//! - Basic template rendering
//! - Content filter (`{{ 'path' | content }}`) functionality
//! - Project-level template variables
//! - Resource-specific template variables
//! - Enhanced error handling and clarity

mod basic;
mod content_filter;
mod error_clarity;
mod project_vars;
mod resource_vars;
