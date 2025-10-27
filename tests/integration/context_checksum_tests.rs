//! Tests for context checksum functionality

use agpm_cli::tests::common::TestProject;
use anyhow::Result;
use tokio::fs as fs;

/// Test that context checksums are generated for templated resources
#[tokio::test]
async fn test_context_checksum_generation() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;
    let test_repo = project.create_source_repo("test-repo").await?;

    // Create a templated resource
    test_repo
        .add_resource(
            "agents",
            "templated",
            r#"---
title: "{{ project.name }}"
version: "{{ config.version }}"
agpm:
  templating: true
---
# {{ project.name }} v{{ config.version }}

This is a templated agent.
"#,
        )
        .await?;

    // Create a non-templated resource
    test_repo
        .add_resource(
            "agents",
            "plain",
            r#"---
title: Plain Agent
version: "1.0.0"
---
# Plain Agent

This is a plain agent without templating.
"#,
        )
        .await?;

    test_repo.commit_all("Initial version")?;
    test_repo.tag_version("v1.0.0")?;

    let repo_url = test_repo.bare_file_url(project.sources_path())?;

    let manifest = format!(
        r#"[sources]
test-repo = "{}"

[agents]
templated = {{ source = "test-repo", path = "agents/templated.md", version = "v1.0.0", template_vars = {{ project = {{ name = "MyProject" }}, config = {{ version = "2.0" }} }} }}
plain = {{ source = "test-repo", path = "agents/plain.md", version = "v1.0.0" }}
"#,
        repo_url
    );

    project.write_manifest(&manifest).await?;

    // Install resources
    let output = project.run_agpm(&["install"])?;
    assert!(output.success, "Install should succeed. Stderr: {}", output.stderr);

    // Read lockfile
    let lockfile_content = project.read_lockfile().await?;

    // Verify context checksum is present for templated resource
    assert!(
        lockfile_content.contains("context_checksum"),
        "Lockfile should contain context_checksum for templated resources"
    );

    // Parse and verify specific sections
    let lines: Vec<&str> = lockfile_content.lines().collect();
    let mut templated_context_checksum = None;
    let mut plain_context_checksum = None;
    let mut in_templated_section = false;
    let mut in_plain_section = false;

    for line in lines {
        if line.trim() == "name = \"templated\"" {
            in_templated_section = true;
            in_plain_section = false;
        } else if line.trim() == "name = \"plain\"" {
            in_templated_section = false;
            in_plain_section = true;
        } else if line.trim().starts_with('[') {
            in_templated_section = false;
            in_plain_section = false;
        } else if line.trim().starts_with("context_checksum") {
            if in_templated_section {
                templated_context_checksum = Some(line.trim().to_string());
            } else if in_plain_section {
                plain_context_checksum = Some(line.trim().to_string());
            }
        }
    }

    // Templated resource should have context checksum
    assert!(
        templated_context_checksum.is_some(),
        "Templated resource should have context checksum"
    );

    // Plain resource should NOT have context checksum (None)
    assert!(
        plain_context_checksum.is_none(),
        "Plain resource should not have context checksum"
    );

    // Verify context checksum format
    if let Some(checksum_line) = templated_context_checksum {
        assert!(
            checksum_line.starts_with("context_checksum = \"sha256:"),
            "Context checksum should have proper format: {}",
            checksum_line
        );

        let checksum = checksum_line
            .strip_prefix("context_checksum = \"")
            .unwrap()
            .strip_suffix("\"")
            .unwrap();

        assert!(
            checksum.starts_with("sha256:"),
            "Checksum should start with sha256: prefix"
        );

        let hash_part = &checksum[7..]; // Remove "sha256:" prefix
        assert_eq!(
            hash_part.len(),
            64,
            "SHA-256 hash should be 64 characters: {}",
            hash_part
        );
        assert!(
            hash_part.chars().all(|c| c.is_ascii_hexdigit()),
            "SHA-256 hash should be hex digits: {}",
            hash_part
        );
    }

    Ok(())
}

/// Test that different template variables produce different context checksums
#[tokio::test]
async fn test_context_checksum_uniqueness() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;
    let test_repo = project.create_source_repo("test-repo").await?;

    // Create a simple templated resource
    test_repo
        .add_resource(
            "snippets",
            "configurable",
            r#"---
title: "{{ config.title }}"
env: "{{ config.env }}"
agpm:
  templating: true
---
# {{ config.title }}

Environment: {{ config.env }}
"#,
        )
        .await?;

    test_repo.commit_all("Initial version")?;
    test_repo.tag_version("v1.0.0")?;

    let repo_url = test_repo.bare_file_url(project.sources_path())?;

    // First configuration
    let manifest1 = format!(
        r#"[sources]
test-repo = "{}"

[snippets]
config1 = {{ source = "test-repo", path = "snippets/configurable.md", version = "v1.0.0", template_vars = {{ config = {{ title = "Development", env = "dev" }} }} }}
"#,
        repo_url
    );

    project.write_manifest(&manifest1).await?;
    let output1 = project.run_agpm(&["install"])?;
    assert!(output1.success, "First install should succeed");

    let lockfile1_content = project.read_lockfile().await?;

    // Clean up for second test
    let lockfile_path = project.project_path().join("agpm.lock");
    fs::remove_file(&lockfile_path).await?;

    // Second configuration (different template variables)
    let manifest2 = format!(
        r#"[sources]
test-repo = "{}"

[snippets]
config2 = {{ source = "test-repo", path = "snippets/configurable.md", version = "v1.0.0", template_vars = {{ config = {{ title = "Production", env = "prod" }} }} }}
"#,
        repo_url
    );

    project.write_manifest(&manifest2).await?;
    let output2 = project.run_agpm(&["install"])?;
    assert!(output2.success, "Second install should succeed");

    let lockfile2_content = project.read_lockfile().await?;

    // Extract context checksums
    let extract_checksum = |content: &str, name: &str| -> Option<String> {
        let lines: Vec<&str> = content.lines().collect();
        let mut in_target_section = false;

        for line in lines {
            if line.trim() == &format!("name = \"{}\"", name) {
                in_target_section = true;
            } else if line.trim().starts_with('[') {
                in_target_section = false;
            } else if in_target_section && line.trim().starts_with("context_checksum") {
                return Some(line.trim().to_string());
            }
        }
        None
    };

    let checksum1 = extract_checksum(&lockfile1_content, "config1");
    let checksum2 = extract_checksum(&lockfile2_content, "config2");

    assert!(
        checksum1.is_some(),
        "Should find context checksum for config1"
    );
    assert!(
        checksum2.is_some(),
        "Should find context checksum for config2"
    );

    // Context checksums should be different
    assert_ne!(
        checksum1, checksum2,
        "Different template variables should produce different context checksums. Config1: {}, Config2: {}",
        checksum1.unwrap(),
        checksum2.unwrap()
    );

    Ok(())
}

/// Test that same template variables produce same context checksums
#[tokio::test]
async fn test_context_checksum_consistency() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;
    let test_repo = project.create_source_repo("test-repo").await?;

    // Create a templated resource
    test_repo
        .add_resource(
            "agents",
            "consistent",
            r#"---
title: "{{ project.title }}"
author: "{{ project.author }}"
agpm:
  templating: true
---
# {{ project.title }} by {{ project.author }}

Consistent agent.
"#,
        )
        .await?;

    test_repo.commit_all("Initial version")?;
    test_repo.tag_version("v1.0.0")?;

    let repo_url = test_repo.bare_file_url(project.sources_path())?;

    // Define template variables
    let manifest_template = format!(
        r#"[sources]
test-repo = "{}"

[agents]
consistent = {{ source = "test-repo", path = "agents/consistent.md", version = "v1.0.0", template_vars = {{ project = {{ title = "{}", author = "{}" }} }} }}
"#,
        repo_url, "{}", "{}"
    );

    let template_vars = vec![
        ("MyProject".to_string(), "Alice".to_string()),
        ("MyProject".to_string(), "Alice".to_string()), // Same as above
        ("DifferentProject".to_string(), "Alice".to_string()),
        ("MyProject".to_string(), "Bob".to_string()),
    ];

    let mut checksums = Vec::new();

    for (title, author) in template_vars {
        // Clean lockfile
        let lockfile_path = project.project_path().join("agpm.lock");
        if lockfile_path.exists() {
            fs::remove_file(&lockfile_path).await?;
        }

        // Install with template variables
        let manifest = manifest_template.replace("{}", "{}").replace("{}", "{}")
            .replace("{}", &title)
            .replace("{}", &author);

        // This is getting complex, let me simplify
        let manifest = format!(
            r#"[sources]
test-repo = "{}"

[agents]
consistent = {{ source = "test-repo", path = "agents/consistent.md", version = "v1.0.0", template_vars = {{ project = {{ title = "{}", author = "{}" }} }} }}
"#,
            repo_url, title, author
        );

        project.write_manifest(&manifest).await?;
        let output = project.run_agpm(&["install"])?;
        assert!(output.success, "Install should succeed for {} by {}", title, author);

        let lockfile_content = project.read_lockfile().await?;

        // Extract context checksum
        let lines: Vec<&str> = lockfile_content.lines().collect();
        let mut context_checksum = None;

        for line in lines {
            if line.trim().starts_with("context_checksum") {
                context_checksum = Some(line.trim().to_string());
                break;
            }
        }

        assert!(
            context_checksum.is_some(),
            "Should find context checksum for {} by {}",
            title, author
        );

        checksums.push(context_checksum.unwrap());
    }

    // First two should be identical (same title and author)
    assert_eq!(
        checksums[0], checksums[1],
        "Same template variables should produce same context checksum: {}",
        checksums[0]
    );

    // Others should be different
    assert_ne!(checksums[0], checksums[2], "Different titles should produce different checksums");
    assert_ne!(checksums[0], checksums[3], "Different authors should produce different checksums");
    assert_ne!(checksums[2], checksums[3], "Different combinations should produce different checksums");

    Ok(())
}

/// Test context checksum with complex nested structures
#[tokio::test]
async fn test_context_checksum_complex_structures() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;
    let test_repo = project.create_source_repo("test-repo").await?;

    // Create a template with complex nested structures
    test_repo
        .add_resource(
            "commands",
            "complex-command",
            r#"---
config:
  database:
    host: "{{ db.host }}"
    port: {{ db.port }}
    ssl: {{ db.ssl }}
  features:
    {% for feature in features %}
    - {{ feature }}
    {% endfor %}
  timeouts:
    connect: {{ timeouts.connect }}
    read: {{ timeouts.read }}
agpm:
  templating: true
---
# Complex Command

Database: {{ db.host }}:{{ db.port }}
Features: {{ features | join(", ") }}
Timeouts: connect={{ timeouts.connect }}s, read={{ timeouts.read }}s
"#,
        )
        .await?;

    test_repo.commit_all("Initial version")?;
    test_repo.tag_version("v1.0.0")?;

    let repo_url = test_repo.bare_file_url(project.sources_path())?;

    // Complex template variables with nested structures
    let manifest = format!(
        r#"[sources]
test-repo = "{}"

[commands]
complex = {{ source = "test-repo", path = "commands/complex-command.md", version = "v1.0.0", template_vars = {{ db = {{ host = "db.example.com", port = 5432, ssl = true }}, features = ["auth", "logging", "monitoring"], timeouts = {{ connect = 10, read = 30 }} }} }}
"#,
        repo_url
    );

    project.write_manifest(&manifest).await?;

    // Install
    let output = project.run_agpm(&["install"])?;
    assert!(output.success, "Install should succeed. Stderr: {}", output.stderr);

    // Verify context checksum is generated
    let lockfile_content = project.read_lockfile().await?;
    assert!(
        lockfile_content.contains("context_checksum"),
        "Complex template should generate context checksum"
    );

    // Verify context checksum format
    let lines: Vec<&str> = lockfile_content.lines().collect();
    for line in lines {
        if line.trim().starts_with("context_checksum") {
            assert!(
                line.starts_with("context_checksum = \"sha256:"),
                "Context checksum should have proper format: {}",
                line
            );
            break;
        }
    }

    // Verify the command was rendered correctly
    let command_path = project.project_path().join(".claude/commands/complex.md");
    assert!(
        command_path.exists(),
        "Complex command should be installed"
    );

    let command_content = fs::read_to_string(&command_path).await?;
    assert!(
        command_content.contains("db.example.com:5432"),
        "Command should contain rendered database info"
    );
    assert!(
        command_content.contains("auth, logging, monitoring"),
        "Command should contain rendered features"
    );
    assert!(
        command_content.contains("connect=10s, read=30s"),
        "Command should contain rendered timeouts"
    );

    Ok(())
}