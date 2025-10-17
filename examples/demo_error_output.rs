//! Demonstration of improved template error messages.
//!
//! This example shows exactly what users will see when template errors occur.

use agpm_cli::templating::TemplateRenderer;
use agpm_cli::core::error::user_friendly_error;
use tera::Context as TeraContext;
use anyhow::Context as AnyhowContext;

fn main() {
    println!("=== DEMONSTRATION: Template Error Messages ===\n");

    // Scenario 1: Missing variable error
    println!("Scenario 1: Resource with missing template variable");
    println!("─────────────────────────────────────────────────\n");

    let mut renderer = TemplateRenderer::new(true).unwrap();
    let context = TeraContext::new();

    let template_content = "# {{ agpm.resource.name }}\n\nThis depends on: {{ agpm.deps.missing_resource.path }}";

    match renderer.render_template(template_content, &context) {
        Ok(_) => println!("Unexpected success"),
        Err(e) => {
            // Simulate the full error context as it appears in installer.rs
            let error_with_context = e.context(
                "Failed to render template for 'my-awesome-agent' (source: community, path: agents/awesome.md)"
            );

            // Show what user_friendly_error produces
            let friendly = user_friendly_error(error_with_context);

            println!("┌─ Error Display ─────────────────────────────────────────┐");
            println!("│ {}", friendly.error);
            if let Some(ref suggestion) = friendly.suggestion {
                println!("│");
                println!("│ 💡 Suggestion:");
                for line in suggestion.lines() {
                    println!("│    {}", line);
                }
            }
            println!("└──────────────────────────────────────────────────────────┘");
        }
    }

    println!("\n");

    // Scenario 2: Syntax error
    println!("Scenario 2: Resource with syntax error (unclosed tag)");
    println!("─────────────────────────────────────────────────────────\n");

    let bad_template = "# Agent\n\nVersion: {{ agpm.resource.version ";

    match renderer.render_template(bad_template, &context) {
        Ok(_) => println!("Unexpected success"),
        Err(e) => {
            let error_with_context = e.context(
                "Failed to render template for 'syntax-error-agent' (source: local, path: agents/bad.md)"
            );

            let friendly = user_friendly_error(error_with_context);

            println!("┌─ Error Display ─────────────────────────────────────────┐");
            println!("│ {}", friendly.error);
            println!("└──────────────────────────────────────────────────────────┘");
        }
    }

    println!("\n");

    // Scenario 3: Unknown filter
    println!("Scenario 3: Resource using unknown filter");
    println!("──────────────────────────────────────────\n");

    let mut ctx = TeraContext::new();
    ctx.insert("name", "TestAgent");
    let filter_template = "# {{ name | capitalize | unknown_filter }}";

    match renderer.render_template(filter_template, &ctx) {
        Ok(_) => println!("Unexpected success"),
        Err(e) => {
            let error_with_context = e.context(
                "Failed to render template for 'filter-test-agent' (source: community, path: agents/filter.md)"
            );

            let friendly = user_friendly_error(error_with_context);

            println!("┌─ Error Display ─────────────────────────────────────────┐");
            println!("│ {}", friendly.error);
            println!("└──────────────────────────────────────────────────────────┘");
        }
    }

    println!("\n=== Key Improvements ===");
    println!("✓ No '__tera_one_off' internal names exposed");
    println!("✓ Actual resource names shown (not 'template')");
    println!("✓ Clear, actionable error messages");
    println!("✓ Helpful suggestions included");
}
