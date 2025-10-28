//! Enhanced template error handling for AGPM
//!
//! This module provides structured error types for template rendering with detailed
//! context information and user-friendly formatting.

use std::path::PathBuf;

use super::renderer::DependencyChainEntry;
use crate::core::ResourceType;

/// Enhanced template errors with detailed context
#[derive(Debug)]
pub enum TemplateError {
    VariableNotFound {
        variable: String,
        available_variables: Box<Vec<String>>,
        suggestions: Box<Vec<String>>,
        location: Box<ErrorLocation>,
    },

    CircularDependency {
        chain: Box<Vec<DependencyChainEntry>>,
    },

    SyntaxError {
        message: String,
        location: Box<ErrorLocation>,
    },

    DependencyRenderFailed {
        dependency: String,
        source: Box<dyn std::error::Error + Send + Sync>,
        location: Box<ErrorLocation>,
    },

    ContentFilterError {
        depth: usize,
        source: Box<dyn std::error::Error + Send + Sync>,
        location: Box<ErrorLocation>,
    },
}

/// Location information for template errors
#[derive(Debug, Clone)]
pub struct ErrorLocation {
    /// Resource where error occurred
    pub resource_name: String,
    pub resource_type: ResourceType,
    /// Full dependency chain to this resource
    pub dependency_chain: Vec<DependencyChainEntry>,
    /// File path if known
    pub file_path: Option<PathBuf>,
    /// Line number if available from Tera
    pub line_number: Option<usize>,
}

impl std::fmt::Display for TemplateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TemplateError::VariableNotFound {
                variable,
                ..
            } => {
                write!(f, "Template variable not found: '{}'", variable)
            }
            TemplateError::SyntaxError {
                message,
                ..
            } => {
                write!(f, "Template syntax error: {}", message)
            }
            TemplateError::CircularDependency {
                chain,
            } => {
                if let Some(first) = chain.first() {
                    write!(f, "Circular dependency detected: {}", first.name)
                } else {
                    write!(f, "Circular dependency detected")
                }
            }
            TemplateError::DependencyRenderFailed {
                dependency,
                source,
                ..
            } => {
                write!(f, "Failed to render dependency '{}': {}", dependency, source)
            }
            TemplateError::ContentFilterError {
                source,
                ..
            } => {
                write!(f, "Content filter error: {}", source)
            }
        }
    }
}

impl std::error::Error for TemplateError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            TemplateError::DependencyRenderFailed {
                source,
                ..
            } => Some(source.as_ref()),
            TemplateError::ContentFilterError {
                source,
                ..
            } => Some(source.as_ref()),
            _ => None,
        }
    }
}

impl TemplateError {
    /// Generate user-friendly error message with context and suggestions
    pub fn format_with_context(&self) -> String {
        match self {
            TemplateError::VariableNotFound {
                variable,
                available_variables,
                suggestions,
                location,
            } => format_variable_not_found_error(
                variable,
                available_variables,
                suggestions,
                location,
            ),
            TemplateError::CircularDependency {
                chain,
            } => format_circular_dependency_error(chain),
            TemplateError::SyntaxError {
                message,
                location,
            } => format_syntax_error(message, location),
            TemplateError::DependencyRenderFailed {
                dependency,
                source,
                location,
            } => format_dependency_render_error(dependency, source.as_ref(), location),
            TemplateError::ContentFilterError {
                depth,
                source,
                location,
            } => format_content_filter_error(*depth, source.as_ref(), location),
        }
    }
}

/// Format a detailed "variable not found" error message
fn format_variable_not_found_error(
    variable: &str,
    available_variables: &[String],
    suggestions: &[String],
    location: &ErrorLocation,
) -> String {
    let mut msg = String::new();

    // Header
    msg.push_str("ERROR: Template Variable Not Found\n\n");

    // Variable info
    msg.push_str(&format!("Variable: {}\n", variable));

    if let Some(line) = location.line_number {
        msg.push_str(&format!("Line: {}\n", line));
    }

    msg.push_str(&format!(
        "Resource: {} ({})\n\n",
        location.resource_name,
        format_resource_type(&location.resource_type)
    ));

    // Dependency chain
    if !location.dependency_chain.is_empty() {
        msg.push_str("Dependency chain:\n");
        for (i, entry) in location.dependency_chain.iter().enumerate() {
            let indent = "  ".repeat(i);
            let arrow = if i > 0 {
                "└─ "
            } else {
                ""
            };
            let warning = if i == location.dependency_chain.len() - 1 {
                " ⚠️ Error occurred here"
            } else {
                ""
            };

            msg.push_str(&format!(
                "{}{}{}: {}{}\n",
                indent,
                arrow,
                format_resource_type(&entry.resource_type),
                entry.name,
                warning
            ));
        }
        msg.push('\n');
    }

    // Suggestions based on variable name analysis
    if variable.starts_with("agpm.deps.") {
        msg.push_str(&format_missing_dependency_suggestion(variable, location));
    } else if !suggestions.is_empty() {
        msg.push_str("Did you mean one of these?\n");
        for suggestion in suggestions.iter() {
            msg.push_str(&format!("  - {}\n", suggestion));
        }
        msg.push('\n');
    }

    // Available variables (truncated list)
    if !available_variables.is_empty() {
        msg.push_str("Available variables in this context:\n");

        // Group by prefix
        let mut grouped = std::collections::BTreeMap::new();
        for var in available_variables.iter() {
            let prefix = var.split('.').next().unwrap_or(var);
            grouped.entry(prefix).or_insert_with(Vec::new).push(var.clone());
        }

        for (prefix, vars) in grouped.iter().take(5) {
            if vars.len() <= 3 {
                for var in vars {
                    msg.push_str(&format!("  {}\n", var));
                }
            } else {
                msg.push_str(&format!("  {}.*  ({} variables)\n", prefix, vars.len()));
            }
        }

        if grouped.len() > 5 {
            msg.push_str(&format!("  ... and {} more\n", grouped.len() - 5));
        }
        msg.push('\n');
    }

    msg
}

/// Format suggestion for missing dependency declaration
fn format_missing_dependency_suggestion(variable: &str, location: &ErrorLocation) -> String {
    // Parse variable name: agpm.deps.<type>.<name>.<property>
    let parts: Vec<&str> = variable.split('.').collect();
    if parts.len() < 4 || parts[0] != "agpm" || parts[1] != "deps" {
        return String::new();
    }

    let dep_type = parts[2]; // "snippets", "agents", etc.
    let dep_name = parts[3]; // "plugin_lifecycle_guide", etc.

    // Convert snake_case back to potential file name
    // (heuristic: replace _ with -)
    let suggested_filename = dep_name.replace('_', "-");

    let mut msg = String::new();
    msg.push_str(&format!(
        "SUGGESTION: '{}' references '{}' but doesn't declare it as a dependency.\n\n",
        location.resource_name, dep_name
    ));

    msg.push_str(&format!("Fix: Add this to {} frontmatter:\n\n", location.resource_name));
    msg.push_str("---\n");
    msg.push_str("agpm:\n");
    msg.push_str("  templating: true\n");
    msg.push_str("dependencies:\n");
    msg.push_str(&format!("  {}:\n", dep_type));
    msg.push_str(&format!("    - path: ./{}.md\n", suggested_filename));
    msg.push_str("      install: false\n");
    msg.push_str("---\n\n");

    msg.push_str("Note: Adjust the path based on actual file location.\n\n");

    msg
}

/// Format circular dependency error
fn format_circular_dependency_error(chain: &[DependencyChainEntry]) -> String {
    let mut msg = String::new();

    msg.push_str("ERROR: Circular Dependency Detected\n\n");
    msg.push_str("A resource is attempting to include itself through a chain of dependencies.\n\n");

    msg.push_str("Circular chain:\n");
    for entry in chain.iter() {
        msg.push_str(&format!(
            "  {} ({})\n",
            entry.name,
            format_resource_type(&entry.resource_type)
        ));
        msg.push_str("  ↓\n");
    }
    msg.push_str(&format!("  {} (circular reference)\n\n", chain[0].name));

    msg.push_str("SUGGESTION: Remove the dependency that creates the cycle.\n");
    msg.push_str("Consider refactoring shared content into a separate resource.\n\n");

    msg
}

/// Format syntax error
fn format_syntax_error(message: &str, location: &ErrorLocation) -> String {
    let mut msg = String::new();

    msg.push_str("ERROR: Template Syntax Error\n\n");
    msg.push_str(&format!("Error: {}\n", message));
    msg.push_str(&format!(
        "Resource: {} ({})\n",
        location.resource_name,
        format_resource_type(&location.resource_type)
    ));

    if let Some(line) = location.line_number {
        msg.push_str(&format!("Line: {}\n", line));
    }

    if !location.dependency_chain.is_empty() {
        msg.push_str("\nDependency chain:\n");
        for entry in &location.dependency_chain {
            msg.push_str(&format!(
                "  {} ({})\n",
                entry.name,
                format_resource_type(&entry.resource_type)
            ));
        }
    }

    msg.push_str("\nSUGGESTION: Check template syntax for unclosed tags or invalid expressions.\n");
    msg.push_str("Common issues:\n");
    msg.push_str("  - Unclosed {{ }} or {% %} delimiters\n");
    msg.push_str("  - Invalid filter names\n");
    msg.push_str("  - Missing quotes around string values\n\n");

    msg
}

/// Format dependency render error
fn format_dependency_render_error(
    dependency: &str,
    source: &(dyn std::error::Error + Send + Sync),
    location: &ErrorLocation,
) -> String {
    let mut msg = String::new();

    msg.push_str("ERROR: Dependency Rendering Failed\n\n");
    msg.push_str(&format!("Dependency: {}\n", dependency));
    msg.push_str(&format!("Error: {}\n", source));
    msg.push_str(&format!(
        "Resource: {} ({})\n",
        location.resource_name,
        format_resource_type(&location.resource_type)
    ));

    msg.push_str("\nSUGGESTION: Check the dependency file for template errors.\n");
    msg.push_str("The dependency may contain invalid template syntax or missing variables.\n\n");

    msg
}

/// Format content filter error
fn format_content_filter_error(
    depth: usize,
    source: &(dyn std::error::Error + Send + Sync),
    location: &ErrorLocation,
) -> String {
    let mut msg = String::new();

    msg.push_str("ERROR: Content Filter Error\n\n");
    msg.push_str(&format!("Depth: {}\n", depth));
    msg.push_str(&format!("Error: {}\n", source));
    msg.push_str(&format!(
        "Resource: {} ({})\n",
        location.resource_name,
        format_resource_type(&location.resource_type)
    ));

    msg.push_str("\nSUGGESTION: Check the file being included by the content filter.\n");
    msg.push_str("The included file may contain template errors or circular dependencies.\n\n");

    msg
}

fn format_resource_type(rt: &ResourceType) -> String {
    match rt {
        ResourceType::Agent => "agent",
        ResourceType::Command => "command",
        ResourceType::Snippet => "snippet",
        ResourceType::Hook => "hook",
        ResourceType::Script => "script",
        ResourceType::McpServer => "mcp-server",
    }
    .to_string()
}
