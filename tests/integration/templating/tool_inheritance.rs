//! Integration tests for tool inheritance in transitive dependencies
//!
//! This tests the critical bug where:
//! 1. A parent agent has tool: claude-code
//! 2. The agent declares a transitive snippet with explicit tool: agpm in its frontmatter
//! 3. The snippet is resolved and added to lockfile with tool: agpm
//! 4. Template context lookup MUST use the explicit tool, not inherit from parent
//!
//! Bug: Template context was inheriting parent's tool instead of using explicit tool
//! Result: Lookup failed, dependencies not found in context, rendering failed

use crate::common::TestProject;
use anyhow::Result;

/// Test that transitive dependencies with explicit tool specifications are found correctly
///
/// This is a regression test for a bug where template context lookup would inherit
/// the parent resource's tool instead of respecting the explicit tool specified
/// in the dependency's frontmatter declaration.
///
/// Expected behavior:
/// - Parent agent (claude-code) declares dependency on snippet with tool: agpm
/// - Snippet is resolved with tool: agpm and added to lockfile
/// - Template context lookup MUST use tool: agpm (explicit) not tool: claude-code (inherited)
/// - Snippet content is accessible in parent's template
#[tokio::test]
async fn test_explicit_tool_in_transitive_dependency() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;
    let community_repo = project.create_source_repo("community").await?;

    // Create language-specific snippets (to match real-world structure)
    community_repo
        .add_resource(
            "snippets/agents",
            "backend-engineer-python",
            r#"---
agpm:
  templating: false
---
# Backend Engineer Best Practices - Python

- Follow PEP 8 style guide
- Use type hints
- Write comprehensive tests with pytest
- Document all public APIs with docstrings
"#,
        )
        .await?;

    community_repo
        .add_resource(
            "snippets/agents",
            "backend-engineer-rust",
            r#"---
agpm:
  templating: false
---
# Backend Engineer Best Practices - Rust

- Follow Rust idioms
- Use Result<T, E> for error handling
- Write comprehensive tests with #[test]
- Document all public APIs with /// comments
"#,
        )
        .await?;

    // Create an agent with tool: claude-code that depends on the agpm snippet
    // Uses TEMPLATED paths and versions (like real-world case)
    community_repo
        .add_resource(
            "claude-code/agents",
            "backend-engineer",
            r#"---
name: backend-engineer
description: Backend development specialist
color: green
agpm:
  version: "1.0.0"
  templating: true
  dependencies:
    snippets:
      - name: backend-engineer-base
        path: ../../snippets/agents/backend-engineer-{{ agpm.project.language }}.md
        version: "snippet-agent-backend-engineer-{{ agpm.project.language }}-^v1.0.0"
        tool: agpm  # EXPLICIT tool - should NOT inherit claude-code from parent
        install: false
---
{{ agpm.deps.snippets.backend_engineer_base.content }}

## Additional Context

This agent is designed for Claude Code for {{ agpm.project.language }} development.
"#,
        )
        .await?;

    community_repo.commit_all("Add backend resources with cross-tool dependency")?;

    // Create versioned prefix tags (like real-world case)
    community_repo.tag_version("claude-code-agent-backend-engineer-v1.0.0")?;
    community_repo.tag_version("snippet-agent-backend-engineer-python-v1.0.0")?;
    community_repo.tag_version("snippet-agent-backend-engineer-rust-v1.0.0")?;

    // Create manifest with multiple variants (different template_vars)
    // Use versioned prefixes like real-world case
    let source_url = community_repo.bare_file_url(project.sources_path())?;
    let manifest = format!(
        r#"[sources]
community = "{}"

[agents]
backend-engineer-python = {{ source = "community", path = "claude-code/agents/backend-engineer.md", version = "claude-code-agent-backend-engineer-^v1.0.0", filename = "backend-engineer-python.md", template_vars = {{ project = {{ language = "python" }} }} }}
backend-engineer-rust = {{ source = "community", path = "claude-code/agents/backend-engineer.md", version = "claude-code-agent-backend-engineer-^v1.0.0", filename = "backend-engineer-rust.md", template_vars = {{ project = {{ language = "rust" }} }} }}
"#,
        source_url
    );

    project.write_manifest(&manifest).await?;

    // Run install - this should succeed
    let output = project.run_agpm(&["install"])?;

    if !output.success {
        eprintln!("==== INSTALL FAILED ====");
        eprintln!("STDERR:\n{}", output.stderr);
        eprintln!("STDOUT:\n{}", output.stdout);
        eprintln!("=======================");
    }

    // The bug: Before fix, this would fail with:
    // "Variable `agpm.deps.snippets.backend_engineer_base.content` not found in context"
    //
    // Root cause: Template context lookup inherited parent's tool (claude-code)
    // instead of using explicit tool (agpm) from frontmatter
    assert!(
        output.success,
        "Install should succeed when transitive dependency has explicit tool.\n\
         Before fix: Would fail because template context lookup inherited parent's tool \
         (claude-code) instead of using explicit tool (agpm) from frontmatter.\n\
         Stderr:\n{}",
        output.stderr
    );

    // Verify both variants were installed
    let python_agent = tokio::fs::read_to_string(
        project.project_path().join(".claude/agents/backend-engineer-python.md")
    ).await?;
    let rust_agent = tokio::fs::read_to_string(
        project.project_path().join(".claude/agents/backend-engineer-rust.md")
    ).await?;

    // Both should contain the embedded snippet content
    assert!(
        python_agent.contains("Backend Engineer Best Practices - Python"),
        "Python variant should contain embedded python snippet content. Got:\n{}",
        python_agent
    );
    assert!(
        python_agent.contains("Follow PEP 8 style guide"),
        "Python variant should contain specific python content"
    );
    assert!(
        rust_agent.contains("Backend Engineer Best Practices - Rust"),
        "Rust variant should contain embedded rust snippet content. Got:\n{}",
        rust_agent
    );
    assert!(
        rust_agent.contains("Follow Rust idioms"),
        "Rust variant should contain specific rust content"
    );

    // Verify lockfile has both resources with correct tools
    let lockfile_content = project.read_lockfile().await?;

    // Both parent variants should have tool: claude-code
    assert!(
        lockfile_content.contains(r#"name = "claude-code/agents/backend-engineer""#),
        "Lockfile should contain parent agent"
    );
    assert!(
        lockfile_content.contains(r#"tool = "claude-code""#),
        "Parent agent should have tool = claude-code"
    );

    // Should have two variants with different variant_inputs_hash
    let variant_count = lockfile_content.matches(r#"name = "claude-code/agents/backend-engineer""#).count();
    assert!(
        variant_count >= 2,
        "Lockfile should have at least 2 variants of the agent (python and rust). Found {}",
        variant_count
    );

    // Verify transitive snippets are in lockfile with tool: agpm (explicit from frontmatter)
    // Since snippets have install: false, they appear as dependencies but not as separate entries
    assert!(
        lockfile_content.contains(r#"tool = "agpm""#),
        "Lockfile should contain resources with tool = agpm (the transitive snippets)"
    );

    Ok(())
}
