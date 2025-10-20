use anyhow::Result;
use std::fs as sync_fs;
use tokio::fs;

use crate::common::{ManifestBuilder, ResourceConfigBuilder, TestProject};

#[tokio::test]
async fn test_opencode_agent_installation() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;
    let source_repo = project.create_source_repo("test_repo").await?;

    // Create OpenCode agent
    let agents_dir = source_repo.path.join("agents");
    sync_fs::create_dir_all(&agents_dir)?;
    sync_fs::write(
        agents_dir.join("helper.md"),
        "---\ntitle: Helper Agent\n---\n\nThis is an OpenCode helper agent.",
    )?;

    source_repo.commit_all("Add OpenCode agent")?;
    source_repo.tag_version("v1.0.0")?;

    let repo_url = source_repo.bare_file_url(project.sources_path())?;
    let manifest = ManifestBuilder::new()
        .add_source("test_repo", &repo_url)
        .with_tools_config(|t| {
            t.tool("opencode", |tc| {
                tc.path(".opencode")
                    .enabled(true)
                    .agents(ResourceConfigBuilder::default().path("agent"))
            })
        })
        .add_agent("opencode-helper", |d| {
            d.source("test_repo")
                .path("agents/helper.md")
                .version("v1.0.0")
                .tool("opencode")
                .flatten(false)
        })
        .build();

    project.write_manifest(&manifest).await?;
    project.run_agpm(&["install"])?;

    // Verify agent installed to .opencode/agent/agents/ (preserves source structure)
    // Path agents/helper.md doesn't match .opencode/agent (agent != agents), so full path preserved
    let agent_path = project.project_path().join(".opencode/agent/agents/helper.md");
    assert!(agent_path.exists(), "OpenCode agent should be installed to .opencode/agent/agents/");

    let content = fs::read_to_string(&agent_path).await?;
    assert!(content.contains("OpenCode helper agent"));

    // Verify lockfile contains opencode tool type and path
    let lockfile_content = project.read_lockfile().await?;
    assert!(lockfile_content.contains(r#"tool = "opencode""#));
    assert!(lockfile_content.contains(r#"installed_at = ".opencode/agent/agents/helper.md""#));

    Ok(())
}

#[tokio::test]
async fn test_opencode_command_installation() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;
    let source_repo = project.create_source_repo("test_repo").await?;

    // Create OpenCode command
    let commands_dir = source_repo.path.join("commands");
    sync_fs::create_dir_all(&commands_dir)?;
    sync_fs::write(
        commands_dir.join("deploy.md"),
        "---\ntitle: Deploy Command\n---\n\nThis is a deployment command.",
    )?;

    source_repo.commit_all("Add OpenCode command")?;
    source_repo.tag_version("v1.0.0")?;

    let repo_url = source_repo.bare_file_url(project.sources_path())?;
    let manifest = ManifestBuilder::new()
        .add_source("test_repo", &repo_url)
        .with_tools_config(|t| {
            t.tool("opencode", |tc| {
                tc.path(".opencode")
                    .enabled(true)
                    .commands(ResourceConfigBuilder::default().path("command"))
            })
        })
        .add_command("deploy", |d| {
            d.source("test_repo")
                .path("commands/deploy.md")
                .version("v1.0.0")
                .tool("opencode")
                .flatten(false)
        })
        .build();

    project.write_manifest(&manifest).await?;
    project.run_agpm(&["install"])?;

    // Verify command installed to .opencode/command/ (singular)
    let command_path = project.project_path().join(".opencode/command/commands/deploy.md");
    assert!(command_path.exists(), "OpenCode command should be installed to .opencode/command/");

    let content = fs::read_to_string(&command_path).await?;
    assert!(content.contains("deployment command"));

    Ok(())
}

#[tokio::test]
async fn test_opencode_mcp_server_merge() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;
    let source_repo = project.create_source_repo("test_repo").await?;

    // Create MCP server config
    let mcp_dir = source_repo.path.join("mcp-servers");
    sync_fs::create_dir_all(&mcp_dir)?;
    sync_fs::write(
        mcp_dir.join("filesystem.json"),
        r#"{
  "command": "npx",
  "args": ["-y", "@modelcontextprotocol/server-filesystem"],
  "env": {}
}"#,
    )?;

    source_repo.commit_all("Add MCP server")?;
    source_repo.tag_version("v1.0.0")?;

    let repo_url = source_repo.bare_file_url(project.sources_path())?;
    let manifest = ManifestBuilder::new()
        .add_source("test_repo", &repo_url)
        .with_tools_config(|t| {
            t.tool("opencode", |tc| {
                tc.path(".opencode").enabled(true).mcp_servers(
                    ResourceConfigBuilder::default().merge_target(".opencode/opencode.json"),
                )
            })
        })
        .add_mcp_server("filesystem", |d| {
            d.source("test_repo")
                .path("mcp-servers/filesystem.json")
                .version("v1.0.0")
                .tool("opencode")
        })
        .build();

    project.write_manifest(&manifest).await?;
    project.run_agpm(&["install"])?;

    // Verify MCP server merged into opencode.json
    let opencode_config_path = project.project_path().join(".opencode/opencode.json");
    assert!(opencode_config_path.exists(), "opencode.json should be created");

    let config_content = fs::read_to_string(&opencode_config_path).await?;
    let config: serde_json::Value = serde_json::from_str(&config_content)?;

    // Verify mcp section exists with filesystem server
    assert!(config.get("mcp").is_some(), "opencode.json should have mcp section");
    let mcp = config["mcp"].as_object().unwrap();
    assert!(mcp.contains_key("filesystem"), "filesystem server should be in mcp section");

    // Verify server config
    let filesystem = &mcp["filesystem"];
    assert_eq!(filesystem["command"], "npx");
    assert!(filesystem["args"].as_array().unwrap().contains(&serde_json::json!("-y")));

    // Verify AGPM metadata is present
    assert!(filesystem.get("_agpm").is_some(), "Server should have _agpm metadata");
    let agpm_meta = filesystem["_agpm"].as_object().unwrap();
    assert_eq!(agpm_meta["managed"], true);

    Ok(())
}

#[tokio::test]
async fn test_mixed_artifact_types() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;
    let source_repo = project.create_source_repo("test_repo").await?;

    // Create resources for both Claude Code and OpenCode
    let agents_dir = source_repo.path.join("agents");
    sync_fs::create_dir_all(&agents_dir)?;
    sync_fs::write(
        agents_dir.join("claude-agent.md"),
        "---\ntitle: Claude Agent\n---\n\nClaude Code agent.",
    )?;
    sync_fs::write(
        agents_dir.join("opencode-agent.md"),
        "---\ntitle: OpenCode Agent\n---\n\nOpenCode agent.",
    )?;

    let commands_dir = source_repo.path.join("commands");
    sync_fs::create_dir_all(&commands_dir)?;
    sync_fs::write(
        commands_dir.join("claude-cmd.md"),
        "---\ntitle: Claude Command\n---\n\nClaude Code command.",
    )?;
    sync_fs::write(
        commands_dir.join("opencode-cmd.md"),
        "---\ntitle: OpenCode Command\n---\n\nOpenCode command.",
    )?;

    source_repo.commit_all("Add mixed resources")?;
    source_repo.tag_version("v1.0.0")?;

    let repo_url = source_repo.bare_file_url(project.sources_path())?;
    let manifest = ManifestBuilder::new()
        .add_source("test_repo", &repo_url)
        // Configure both Claude Code and OpenCode tools
        .with_tools_config(|t| {
            t.tool("claude-code", |tc| {
                tc.path(".claude")
                    .enabled(true)
                    .agents(ResourceConfigBuilder::default().path("agents"))
                    .commands(ResourceConfigBuilder::default().path("commands"))
            })
            .tool("opencode", |tc| {
                tc.path(".opencode")
                    .enabled(true)
                    .agents(ResourceConfigBuilder::default().path("agent"))
                    .commands(ResourceConfigBuilder::default().path("command"))
            })
        })
        // Claude Code agents
        .add_standard_agent("claude-agent", "test_repo", "agents/claude-agent.md")
        // OpenCode agents (preserve directory structure for cross-tool path testing)
        .add_agent("opencode-agent", |d| {
            d.source("test_repo")
                .path("agents/opencode-agent.md")
                .version("v1.0.0")
                .tool("opencode")
                .flatten(false)
        })
        // Claude Code commands
        .add_standard_command("claude-cmd", "test_repo", "commands/claude-cmd.md")
        // OpenCode commands (preserve directory structure for cross-tool path testing)
        .add_command("opencode-cmd", |d| {
            d.source("test_repo")
                .path("commands/opencode-cmd.md")
                .version("v1.0.0")
                .tool("opencode")
                .flatten(false)
        })
        .build();

    project.write_manifest(&manifest).await?;
    project.run_agpm(&["install"])?;

    // Verify Claude Code resources (prefix stripped: agents/x.md -> x.md)
    assert!(project.project_path().join(".claude/agents/claude-agent.md").exists());
    assert!(project.project_path().join(".claude/commands/claude-cmd.md").exists());

    // Verify OpenCode resources (prefix not stripped: agent != agents, command != commands)
    assert!(project.project_path().join(".opencode/agent/agents/opencode-agent.md").exists());
    assert!(project.project_path().join(".opencode/command/commands/opencode-cmd.md").exists());

    // Verify lockfile has both tool types
    let lockfile_content = project.read_lockfile().await?;
    assert!(lockfile_content.contains(r#"tool = "opencode""#));
    // claude-code is the default and gets omitted in lockfile for brevity

    Ok(())
}

#[tokio::test]
async fn test_artifact_type_validation() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;
    let source_repo = project.create_source_repo("test_repo").await?;

    // Create a snippet (not supported by OpenCode)
    let snippets_dir = source_repo.path.join("snippets");
    sync_fs::create_dir_all(&snippets_dir)?;
    sync_fs::write(
        snippets_dir.join("example.md"),
        "---\ntitle: Example Snippet\n---\n\nExample content.",
    )?;

    source_repo.commit_all("Add snippet")?;
    source_repo.tag_version("v1.0.0")?;

    let repo_url = source_repo.bare_file_url(project.sources_path())?;
    let manifest = ManifestBuilder::new()
        .add_source("test_repo", &repo_url)
        .with_tools_config(|t| {
            t.tool("opencode", |tc| {
                tc.path(".opencode").enabled(true)
                // Intentionally don't configure snippets - we're testing that it's unsupported
            })
        })
        .add_snippet("example", |d| {
            d.source("test_repo").path("snippets/example.md").version("v1.0.0").tool("opencode")
        })
        .build();

    project.write_manifest(&manifest).await?;

    // This should fail validation because OpenCode doesn't support snippets
    let output = project.run_agpm(&["install"])?;
    assert!(!output.success, "Should fail when installing unsupported resource type");

    // Verify error message mentions the unsupported resource type
    let combined_output = format!("{}{}", output.stdout, output.stderr);
    assert!(
        combined_output.contains("snippets") || combined_output.contains("opencode"),
        "Error should mention unsupported resource type. Got:\nSTDOUT:\n{}\nSTDERR:\n{}",
        output.stdout,
        output.stderr
    );

    Ok(())
}

#[tokio::test]
async fn test_claude_code_mcp_handler() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;
    let source_repo = project.create_source_repo("test_repo").await?;

    // Create MCP server config
    let mcp_dir = source_repo.path.join("mcp-servers");
    sync_fs::create_dir_all(&mcp_dir)?;
    sync_fs::write(
        mcp_dir.join("postgres.json"),
        r#"{
  "command": "npx",
  "args": ["-y", "@modelcontextprotocol/server-postgres"],
  "env": {
    "POSTGRES_URL": "postgresql://localhost/mydb"
  }
}"#,
    )?;

    source_repo.commit_all("Add MCP server")?;
    source_repo.tag_version("v1.0.0")?;

    let repo_url = source_repo.bare_file_url(project.sources_path())?;
    let manifest = ManifestBuilder::new()
        .add_source("test_repo", &repo_url)
        .add_mcp_server("postgres", |d| {
            d.source("test_repo").path("mcp-servers/postgres.json").version("v1.0.0")
        })
        .build();

    project.write_manifest(&manifest).await?;
    project.run_agpm(&["install"])?;

    // Verify MCP server configured in .mcp.json (not copied as artifact file)
    let mcp_config_path = project.project_path().join(".mcp.json");
    assert!(mcp_config_path.exists(), ".mcp.json should be created");

    let config_content = fs::read_to_string(&mcp_config_path).await?;
    let config: serde_json::Value = serde_json::from_str(&config_content)?;

    // Verify mcpServers section
    assert!(config.get("mcpServers").is_some());
    let servers = config["mcpServers"].as_object().unwrap();
    assert!(servers.contains_key("postgres"));

    // Verify server config
    let postgres = &servers["postgres"];
    assert_eq!(postgres["command"], "npx");
    assert!(postgres["env"].is_object());

    // Verify AGPM metadata
    assert!(postgres.get("_agpm").is_some());
    let agpm_meta = postgres["_agpm"].as_object().unwrap();
    assert_eq!(agpm_meta["managed"], true);

    Ok(())
}

#[tokio::test]
async fn test_opencode_mcp_preserves_user_servers() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;

    // Create opencode.json with user-managed server
    let opencode_dir = project.project_path().join(".opencode");
    sync_fs::create_dir_all(&opencode_dir)?;
    let user_config = serde_json::json!({
        "mcp": {
            "user-server": {
                "command": "node",
                "args": ["server.js"]
            }
        }
    });
    fs::write(opencode_dir.join("opencode.json"), serde_json::to_string_pretty(&user_config)?)
        .await?;

    let source_repo = project.create_source_repo("test_repo").await?;

    // Create AGPM-managed MCP server
    let mcp_dir = source_repo.path.join("mcp-servers");
    sync_fs::create_dir_all(&mcp_dir)?;
    sync_fs::write(mcp_dir.join("agpm-server.json"), r#"{"command": "agpm", "args": ["serve"]}"#)?;

    source_repo.commit_all("Add AGPM server")?;
    source_repo.tag_version("v1.0.0")?;

    let repo_url = source_repo.bare_file_url(project.sources_path())?;
    let manifest = ManifestBuilder::new()
        .add_source("test_repo", &repo_url)
        .with_tools_config(|t| {
            t.tool("opencode", |tc| {
                tc.path(".opencode").enabled(true).mcp_servers(
                    ResourceConfigBuilder::default().merge_target(".opencode/opencode.json"),
                )
            })
        })
        .add_mcp_server("agpm-server", |d| {
            d.source("test_repo")
                .path("mcp-servers/agpm-server.json")
                .version("v1.0.0")
                .tool("opencode")
        })
        .build();

    project.write_manifest(&manifest).await?;
    project.run_agpm(&["install"])?;

    // Verify both servers exist
    let config_content = fs::read_to_string(opencode_dir.join("opencode.json")).await?;
    let config: serde_json::Value = serde_json::from_str(&config_content)?;

    let mcp = config["mcp"].as_object().unwrap();
    assert!(mcp.contains_key("user-server"), "User server should be preserved");
    assert!(mcp.contains_key("agpm-server"), "AGPM server should be added");

    // Verify user server has no metadata
    assert!(
        mcp["user-server"].get("_agpm").is_none(),
        "User server should not have _agpm metadata"
    );

    // Verify AGPM server has metadata
    assert!(mcp["agpm-server"].get("_agpm").is_some(), "AGPM server should have _agpm metadata");

    Ok(())
}

#[tokio::test]
async fn test_nested_paths_preserve_structure() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;
    let source_repo = project.create_source_repo("test_repo").await?;

    // Create nested agent structure
    let ai_dir = source_repo.path.join("agents/ai");
    sync_fs::create_dir_all(&ai_dir)?;
    sync_fs::write(ai_dir.join("gpt.md"), "---\ntitle: GPT Agent\n---\n\nAI agent.")?;

    source_repo.commit_all("Add nested agent")?;
    source_repo.tag_version("v1.0.0")?;

    let repo_url = source_repo.bare_file_url(project.sources_path())?;

    // Test for both Claude Code and OpenCode
    let manifest = ManifestBuilder::new()
        .add_source("test_repo", &repo_url)
        .with_tools_config(|t| {
            t.tool("claude-code", |tc| {
                tc.path(".claude")
                    .enabled(true)
                    .agents(ResourceConfigBuilder::default().path("agents"))
            })
            .tool("opencode", |tc| {
                tc.path(".opencode")
                    .enabled(true)
                    .agents(ResourceConfigBuilder::default().path("agent"))
            })
        })
        // Claude Code agent (preserves nested structure with flatten=false)
        .add_agent("claude-ai", |d| {
            d.source("test_repo").path("agents/ai/gpt.md").version("v1.0.0").flatten(false)
        })
        // OpenCode agent (preserves nested structure with flatten=false)
        .add_agent("opencode-ai", |d| {
            d.source("test_repo")
                .path("agents/ai/gpt.md")
                .version("v1.0.0")
                .tool("opencode")
                .flatten(false)
        })
        .build();

    project.write_manifest(&manifest).await?;
    project.run_agpm(&["install"])?;

    // Verify Claude Code preserves nested structure
    assert!(
        project.project_path().join(".claude/agents/ai/gpt.md").exists(),
        "Claude Code should preserve ai/ subdirectory"
    );

    // Verify OpenCode preserves full nested structure (agent != agents, so no stripping)
    assert!(
        project.project_path().join(".opencode/agent/agents/ai/gpt.md").exists(),
        "OpenCode should preserve agents/ai/ subdirectory in agent/"
    );

    Ok(())
}

#[tokio::test]
async fn test_agpm_artifact_type() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;
    let source_repo = project.create_source_repo("test_repo").await?;

    // Create snippet for AGPM artifact type
    let snippets_dir = source_repo.path.join("snippets");
    sync_fs::create_dir_all(&snippets_dir)?;
    sync_fs::write(
        snippets_dir.join("config-template.md"),
        "---\ntitle: Config Template\n---\n\nReusable config template.",
    )?;

    source_repo.commit_all("Add AGPM snippet")?;
    source_repo.tag_version("v1.0.0")?;

    let repo_url = source_repo.bare_file_url(project.sources_path())?;
    let manifest = ManifestBuilder::new()
        .add_source("test_repo", &repo_url)
        .add_snippet("config-template", |d| {
            d.source("test_repo").path("snippets/config-template.md").version("v1.0.0").tool("agpm")
        })
        .build();

    project.write_manifest(&manifest).await?;
    project.run_agpm(&["install"])?;

    // Verify snippet installed to .agpm/snippets/
    let snippet_path = project.project_path().join(".agpm/snippets/config-template.md");
    assert!(snippet_path.exists(), "AGPM snippet should be installed to .agpm/snippets/");

    let content = fs::read_to_string(&snippet_path).await?;
    assert!(content.contains("Reusable config template"));

    // Verify lockfile
    let lockfile_content = project.read_lockfile().await?;
    assert!(lockfile_content.contains(r#"tool = "agpm""#));

    Ok(())
}
