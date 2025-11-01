//! Template rendering tests
//!
//! Tests for template rendering functionality:
//! - Basic template rendering
//! - Content filter (`{{ 'path' | content }}`) functionality
//! - Project-level template variables
//! - Resource-specific template variables
//! - Transitive dependencies with conditional frontmatter
//! - Enhanced error handling and clarity

mod content_filter;
mod error_clarity;
mod project_vars;
mod resource_vars;
mod test_basic_rendering;
mod test_circular_deps;
mod test_edge_cases;
mod test_template_validation;
mod test_transitive_errors;
mod transitive_conditional_deps;
mod windows_security;
