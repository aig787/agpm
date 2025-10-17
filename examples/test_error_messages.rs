//! Demonstration of improved template error messages.
//!
//! Before: "Template rendering error: Failed to render '__tera_one_off'"
//! After:  "Template rendering error: Variable `name` not found in context"

use agpm_cli::templating::TemplateRenderer;
use tera::Context as TeraContext;

fn main() {
    println!("Testing improved template error messages...\n");

    // Test 1: Missing variable
    println!("=== Test 1: Missing variable ===");
    let mut renderer = TemplateRenderer::new(true).unwrap();
    let context = TeraContext::new();

    match renderer.render_template("Hello {{ name }}!", &context) {
        Ok(_) => println!("Unexpected success"),
        Err(e) => {
            // Show both the stderr output and the error chain
            println!("Error message:\n{:#}\n", e);
        }
    }

    // Test 2: Syntax error (unclosed tag)
    println!("=== Test 2: Syntax error (unclosed tag) ===");
    match renderer.render_template("Hello {{ name ", &context) {
        Ok(_) => println!("Unexpected success"),
        Err(e) => {
            println!("Error message:\n{:#}\n", e);
        }
    }

    // Test 3: Unknown filter
    println!("=== Test 3: Unknown filter ===");
    let mut context_with_var = TeraContext::new();
    context_with_var.insert("name", "World");
    match renderer.render_template("Hello {{ name | unknown_filter }}!", &context_with_var) {
        Ok(_) => println!("Unexpected success"),
        Err(e) => {
            println!("Error message:\n{:#}\n", e);
        }
    }

    println!("\nAll error messages now show the actual problem instead of '__tera_one_off'!");
}
