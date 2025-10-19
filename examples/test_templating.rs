use agpm_cli::core::ResourceType;
use agpm_cli::lockfile::{LockFile, LockedResource};
use agpm_cli::templating::{TemplateContextBuilder, TemplateRenderer};
use std::sync::Arc;

fn create_test_lockfile() -> LockFile {
    let mut lockfile = LockFile::default();

    // Add a test agent
    lockfile.agents.push(LockedResource {
        name: "test-agent".to_string(),
        source: Some("community".to_string()),
        url: Some("https://github.com/example/community.git".to_string()),
        path: "agents/test-agent.md".to_string(),
        version: Some("v1.0.0".to_string()),
        resolved_commit: Some("abc123def456".to_string()),
        checksum: "sha256:testchecksum".to_string(),
        installed_at: ".claude/agents/test-agent.md".to_string(),
        dependencies: vec![],
        resource_type: ResourceType::Agent,
        tool: Some("claude-code".to_string()),
        manifest_alias: None,
        applied_patches: std::collections::HashMap::new(),
        install: None,
    });

    lockfile
}

fn main() {
    let lockfile = create_test_lockfile();

    let cache = agpm_cli::cache::Cache::new().unwrap();
    let project_dir = std::env::current_dir().unwrap();
    let builder = TemplateContextBuilder::new(
        Arc::new(lockfile),
        None,
        Arc::new(cache),
        project_dir,
    );

    let context = builder.build_context("test-agent", ResourceType::Agent).unwrap();

    let mut renderer = TemplateRenderer::new(true).unwrap();

    let template_content = r#"---
title: Test Agent
---
# {{ agpm.resource.name }}

This agent is installed at: `{{ agpm.resource.install_path }}`
Version: {{ agpm.resource.version }}
"#;

    let rendered = renderer.render_template(template_content, &context).unwrap();

    println!("=== RENDERED CONTENT ===");
    println!("{}", rendered);
    println!("=== END CONTENT ===");

    // Check if template variables were substituted
    assert!(rendered.contains("# test-agent"), "Resource name should be substituted");
    assert!(
        rendered.contains("installed at: `.claude/agents/test-agent.md`"),
        "Install path should be substituted"
    );
    assert!(rendered.contains("Version: v1.0.0"), "Version should be substituted");

    // Verify original template syntax is gone
    assert!(!rendered.contains("{{ agpm"), "Template syntax should be replaced");

    println!("✅ All template substitution tests passed!");
}
