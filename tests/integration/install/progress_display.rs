use agpm_cli::cli::install::InstallCommand;
use agpm_cli::manifest::{DetailedDependency, Manifest, ResourceDependency};
use std::fs;
use tempfile::TempDir;

#[tokio::test]
async fn test_small_installation_display() {
    let temp = TempDir::new().unwrap();
    let manifest_path = temp.path().join("agpm.toml");

    // Create 3 local files
    for i in 1..=3 {
        let file = temp.path().join(format!("agent{}.md", i));
        fs::write(&file, format!("# Agent {}\nBody", i)).unwrap();
    }

    let mut manifest = Manifest::new();
    for i in 1..=3 {
        manifest.agents.insert(
            format!("agent{}", i),
            ResourceDependency::Detailed(Box::new(DetailedDependency {
                source: None,
                path: format!("agent{}.md", i),
                version: None,
                branch: None,
                rev: None,
                command: None,
                args: None,
                target: None,
                filename: None,
                dependencies: None,
                tool: Some("claude-code".to_string()),
                flatten: None,
                install: None,
                template_vars: Some(serde_json::Value::Object(serde_json::Map::new())),
            })),
        );
    }
    manifest.save(&manifest_path).unwrap();

    let cmd = InstallCommand::new();
    let result = cmd.execute_from_path(Some(&manifest_path)).await;
    assert!(result.is_ok());

    // Verify all 3 resources were installed
    assert!(temp.path().join(".claude/agents/agent1.md").exists());
    assert!(temp.path().join(".claude/agents/agent2.md").exists());
    assert!(temp.path().join(".claude/agents/agent3.md").exists());
}

#[tokio::test]
async fn test_large_installation_display() {
    let temp = TempDir::new().unwrap();
    let manifest_path = temp.path().join("agpm.toml");

    // Create 50 local files
    for i in 1..=50 {
        let file = temp.path().join(format!("agent{}.md", i));
        fs::write(&file, format!("# Agent {}\nBody", i)).unwrap();
    }

    let mut manifest = Manifest::new();
    for i in 1..=50 {
        manifest.agents.insert(
            format!("agent{}", i),
            ResourceDependency::Detailed(Box::new(DetailedDependency {
                source: None,
                path: format!("agent{}.md", i),
                version: None,
                branch: None,
                rev: None,
                command: None,
                args: None,
                target: None,
                filename: None,
                dependencies: None,
                tool: Some("claude-code".to_string()),
                flatten: None,
                install: None,
                template_vars: Some(serde_json::Value::Object(serde_json::Map::new())),
            })),
        );
    }
    manifest.save(&manifest_path).unwrap();

    // Use lower concurrency to make window more visible
    let mut cmd = InstallCommand::new();
    cmd.max_parallel = Some(5);

    let result = cmd.execute_from_path(Some(&manifest_path)).await;
    assert!(result.is_ok());

    // Verify all 50 resources were installed
    for i in 1..=50 {
        assert!(temp.path().join(format!(".claude/agents/agent{}.md", i)).exists());
    }
}

#[tokio::test]
async fn test_quiet_mode_no_progress() {
    let temp = TempDir::new().unwrap();
    let manifest_path = temp.path().join("agpm.toml");

    fs::write(temp.path().join("agent.md"), "# Agent\nBody").unwrap();

    let mut manifest = Manifest::new();
    manifest.agents.insert(
        "agent".into(),
        ResourceDependency::Detailed(Box::new(DetailedDependency {
            source: None,
            path: "agent.md".into(),
            version: None,
            branch: None,
            rev: None,
            command: None,
            args: None,
            target: None,
            filename: None,
            dependencies: None,
            tool: Some("claude-code".to_string()),
            flatten: None,
            install: None,
            template_vars: Some(serde_json::Value::Object(serde_json::Map::new())),
        })),
    );
    manifest.save(&manifest_path).unwrap();

    let cmd = InstallCommand {
        quiet: true,
        no_progress: true,
        ..InstallCommand::new()
    };

    let result = cmd.execute_from_path(Some(&manifest_path)).await;
    assert!(result.is_ok());

    // In quiet mode, no progress should be shown (manual verification needed)
}
