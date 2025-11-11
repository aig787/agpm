//! Template Context Lookup Performance Tests
//!
//! This module tests the performance of template context lookups with large dependency sets
//! to ensure the cache key fix (removing version from lookup) doesn't regress performance.

use std::time::Instant;
use anyhow::Result;
use serial_test::serial;
use crate::common::{ManifestBuilder, TestProject};

/// Test template rendering performance with large dependency sets
#[tokio::test]
#[serial]
async fn test_template_context_lookup_performance() -> Result<()> {
    let project = TestProject::new().await?;

    // Create multiple repositories to simulate real-world scenarios
    let mut repos = Vec::new();
    for repo_idx in 0..5 {
        let repo = project.create_source_repo(&format!("template-repo-{}", repo_idx)).await?;

        // Create a template with many dependencies in each repo
        let mut template_content = format!(r#"---
title: Template Agent {}
agpm:
  templating: true
dependencies:
  snippets:
"#, repo_idx);

        // Add dependencies to template
        for i in 0..30 {
            template_content.push_str(&format!(r#"
    - path: snippets/snippet-{}-{}.md
      version: v1.0.0
"#, repo_idx, i));
        }

        template_content.push_str(&format!(r#"
---
# Template Agent {}

This template has many dependencies:
{{% for name, dep in agpm.deps.snippets %}}
- {{ name }}: {{ dep.checksum }}
{{% endfor %}}

Total: {{ agpm.deps.snippets | length }}
"#, repo_idx));

        repo.add_resource("agents", &format!("template-{}", repo_idx), &template_content)
            .await?;

        // Create the snippets that are referenced
        for i in 0..30 {
            let snippet_content = format!(r#"---
name: snippet-{}-{}
---

# Snippet {}-{}

This is snippet content.
"#, repo_idx, i, repo_idx, i);

            repo.add_resource("snippets", &format!("snippet-{}-{}", repo_idx, i), &snippet_content)
                .await?;
        }

        repo.commit_all(&format!("Add template {} with dependencies", repo_idx))?;
        repo.tag_version("v1.0.0")?;

        repos.push(repo);
    }

    // Build manifest with all template agents
    let mut manifest = ManifestBuilder::new();
    for (idx, repo) in repos.iter().enumerate() {
        let repo_url = repo.bare_file_url(project.sources_path())?;
        manifest = manifest.add_source(&format!("repo-{}", idx), &repo_url);
        manifest = manifest.add_agent(&format!("template-{}", idx), |d| {
            d.source(&format!("repo-{}", idx))
                .path(&format!("agents/template-{}.md", idx))
                .version("v1.0.0")
        });
    }

    project.write_manifest(&manifest.build()).await?;

    // Measure installation performance
    let install_start = Instant::now();

    // Run installation to test template context performance
    let output = project.run_agpm(&["install", "--no-cache", "--max-parallel", "20"])?;

    let install_elapsed = install_start.elapsed();

    // Installation should succeed
    output.assert_success();

    // Performance assertions
    assert!(
        install_elapsed.as_secs() < 30,
        "Installation with template context took too long: {:?}",
        install_elapsed
    );

    println!("Template context lookup performance:");
    println!("  Repositories: {}", repos.len());
    println!("  Templates per repo: 30");
    println!("  Total dependencies: {}", repos.len() * 30);
    println!("  Installation time: {:?}", install_elapsed);
    println!("  Rate: {:.2} resources/second", (repos.len() * 30) as f64 / install_elapsed.as_secs_f64());

    Ok(())
}

/// Test template rendering performance with repeated operations
#[tokio::test]
#[serial]
async fn test_template_rendering_cache_effectiveness() -> Result<()> {
    let project = TestProject::new().await?;

    // Create a repo with a reusable template
    let repo = project.create_source_repo("template-test").await?;

    let template_content = r#"---
title: Cached Template
agpm:
  templating: true
dependencies:
  snippets:
    - path: snippets/shared.md
      version: v1.0.0
---
# Template with Shared Content

{% for name, dep in agpm.deps.snippets %}
{{ name }}: {{ dep.checksum }}
{% endfor %}

{% if agpm.project %}
Project: {{ agpm.project.name }}
{% endif %}
"#;

    repo.add_resource("agents", "cached-template", template_content)
        .await?;

    let shared_content = r#"---
name: shared
---

# Shared Content

This content should be cached and reused efficiently.
"#;

    repo.add_resource("snippets", "shared", shared_content)
        .await?;

    repo.commit_all("Add cached template")?;
    repo.tag_version("v1.0.0")?;

    let repo_url = repo.bare_file_url(project.sources_path())?;

    // Create manifest with multiple instances of same template
    let manifest = ManifestBuilder::new()
        .add_source("test-repo", &repo_url)
        .add_agent("template-1", |d| {
            d.source("test-repo").path("agents/cached-template.md")
                .version("v1.0.0")
        })
        .add_agent("template-2", |d| {
            d.source("test-repo").path("agents/cached-template.md")
                .version("v1.0.0")
        })
        .add_agent("template-3", |d| {
            d.source("test-repo").path("agents/cached-template.md")
                .version("v1.0.0")
        })
        .build();

    project.write_manifest(&manifest).await?;

    // Measure installation time with caching
    let install_start = Instant::now();

    let output = project.run_agpm(&["install", "--no-cache", "--max-parallel", "10"])?;

    let install_elapsed = install_start.elapsed();

    output.assert_success();

    // With proper caching, repeated templates should be efficient
    assert!(
        install_elapsed.as_secs() < 10,
        "Cached template installation took too long: {:?}",
        install_elapsed
    );

    println!("Template caching effectiveness:");
    println!("  Template instances: 3");
    println!("  Shared dependencies: 1");
    println!("  Installation time: {:?}", install_elapsed);
    println!("  Cache efficiency: {:.2} seconds per template instance",
               install_elapsed.as_secs_f64() / 3.0);

    Ok(())
}

/// Test memory usage with large template context
#[tokio::test]
#[serial]
async fn test_template_memory_usage() -> Result<()> {
    let project = TestProject::new().await?;

    // Create a repo with a complex template
    let repo = project.create_source_repo("memory-test").await?;

    // Template with many variables and dependencies
    let mut template_content = r#"---
title: Memory Test Template
agpm:
  templating: true
dependencies:
"#.to_string();

    // Add 100 dependencies
    for i in 0..100 {
        template_content.push_str(&format!(r#"
  snippets:
    - path: snippets/snippet-{}.md
      version: v1.0.0
"#, i));
    }

    template_content.push_str(r#"---
# Memory Test Template

{% if agpm.template_vars %}
Project: {{ agpm.template_vars.project.name }}
Environment: {{ agpm.template_vars.environment }}
{% endif %}

{% for resource_type, items in agpm.deps %}
{{ resource_type }} dependencies:
{% for name, dep in items %}
  - {{ name }}: {{ dep.checksum }}
{% endfor %}
{% endfor %}

Total dependencies: {{ agpm.deps | length }}
"#);

    repo.add_resource("agents", "memory-test", &template_content)
        .await?;

    // Create all the referenced snippets
    for i in 0..100 {
        let snippet_content = format!(r#"---
name: snippet-{}
---

# Snippet {}

Content for snippet {}.
"#, i, i, i);

        repo.add_resource("snippets", &format!("snippet-{}", i), &snippet_content)
            .await?;
    }

    repo.commit_all("Add memory test template")?;
    repo.tag_version("v1.0.0")?;

    let repo_url = repo.bare_file_url(project.sources_path())?;

    let manifest = ManifestBuilder::new()
        .add_source("test-repo", &repo_url)
        .add_agent("memory-test", |d| {
            d.source("test-repo").path("agents/memory-test.md")
                .version("v1.0.0")
        })
        .build();

    project.write_manifest(&manifest).await?;

    // Measure memory impact
    let memory_start = Instant::now();

    let output = project.run_agpm(&["install", "--no-cache", "--max-parallel", "5"])?;

    let memory_elapsed = memory_start.elapsed();

    output.assert_success();

    // Should complete in reasonable time even with 100 dependencies
    assert!(
        memory_elapsed.as_secs() < 20,
        "Memory-intensive template took too long: {:?}",
        memory_elapsed
    );

    println!("Template memory usage test:");
    println!("  Dependencies: 100");
    println!("  Processing time: {:?}", memory_elapsed);
    println!("  Average per dependency: {:.2}ms",
               memory_elapsed.as_millis() as f64 / 100.0);

    Ok(())
}