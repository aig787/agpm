//! Template rendering tests
//!
//! Tests for template rendering functionality:
//! - Basic template rendering
//! - Content filter (`{{ 'path' | content }}`) functionality
//! - Project-level template variables
//! - Resource-specific template variables

mod basic;
mod content_filter;
mod project_vars;
mod resource_vars;
