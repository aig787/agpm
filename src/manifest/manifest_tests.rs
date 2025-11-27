#[cfg(test)]
mod tests {

    use crate::manifest::{
        DetailedDependency, Manifest, ProjectConfig, ResourceDependency, expand_url,
        find_manifest_from, json_value_to_toml, toml_value_to_json,
    };
    use anyhow::Result;

    use tempfile::tempdir;

    #[test]
    fn test_manifest_new() {
        let manifest = Manifest::new();
        assert!(manifest.sources.is_empty());
        assert!(manifest.agents.is_empty());
        assert!(manifest.snippets.is_empty());
        assert!(manifest.commands.is_empty());
        assert!(manifest.mcp_servers.is_empty());
    }

    #[test]
    fn test_manifest_load_save() -> Result<()> {
        let temp = tempdir()?;
        let manifest_path = temp.path().join("agpm.toml");

        let mut manifest = Manifest::new();
        manifest.add_source(
            "official".to_string(),
            "https://github.com/example-org/agpm-official.git".to_string(),
        );
        manifest.add_dependency(
            "test-agent".to_string(),
            ResourceDependency::Detailed(Box::new(DetailedDependency {
                source: Some("official".to_string()),
                path: "agents/test.md".to_string(),
                version: Some("v1.0.0".to_string()),
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
            true,
        );

        manifest.save(&manifest_path)?;

        let loaded = Manifest::load(&manifest_path)?;
        assert_eq!(loaded.sources.len(), 1);
        assert_eq!(loaded.agents.len(), 1);
        assert!(loaded.has_dependency("test-agent"));
        Ok(())
    }

    #[test]
    fn test_manifest_validation() -> Result<()> {
        let mut manifest = Manifest::new();

        // Add dependency without source - should be valid (local dependency)
        manifest.add_dependency(
            "local-agent".to_string(),
            ResourceDependency::Simple("../local/agent.md".to_string()),
            true,
        );
        manifest.validate()?;

        // Add dependency with undefined source - should fail validation
        manifest.add_dependency(
            "remote-agent".to_string(),
            ResourceDependency::Detailed(Box::new(DetailedDependency {
                source: Some("undefined".to_string()),
                path: "agent.md".to_string(),
                version: Some("v1.0.0".to_string()),
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
            true,
        );
        assert!(manifest.validate().is_err());

        // Add the source - should now be valid
        manifest
            .add_source("undefined".to_string(), "https://github.com/test/repo.git".to_string());
        manifest.validate()?;
        Ok(())
    }

    #[test]
    fn test_dependency_helpers() {
        let simple_dep = ResourceDependency::Simple("path/to/file.md".to_string());
        assert!(simple_dep.is_local());
        assert_eq!(simple_dep.get_path(), "path/to/file.md");
        assert!(simple_dep.get_source().is_none());

        let detailed_dep = ResourceDependency::Detailed(Box::new(DetailedDependency {
            source: Some("official".to_string()),
            path: "agents/test.md".to_string(),
            version: Some("v1.0.0".to_string()),
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
        }));
        assert!(!detailed_dep.is_local());
        assert_eq!(detailed_dep.get_path(), "agents/test.md");
        assert_eq!(detailed_dep.get_source(), Some("official"));
        assert_eq!(detailed_dep.get_version(), Some("v1.0.0"));
    }

    #[test]
    fn test_find_manifest_basic() -> Result<()> {
        let temp = tempdir()?;
        let manifest_path = temp.path().join("agpm.toml");
        std::fs::write(&manifest_path, "[sources]\n")?;

        // Should find the manifest we just created
        let found = find_manifest_from(temp.path().to_path_buf())?;
        // Canonicalize both paths to handle macOS /var -> /private/var symlink
        assert_eq!(found.canonicalize()?, manifest_path.canonicalize()?);
        Ok(())
    }

    #[test]
    fn test_find_manifest_parent() -> Result<()> {
        let temp = tempdir()?;
        let manifest_path = temp.path().join("agpm.toml");
        std::fs::write(&manifest_path, "[sources]\n")?;

        // Create a subdirectory
        let subdir = temp.path().join("subdir");
        std::fs::create_dir(&subdir)?;

        // Should find the manifest in parent directory
        let found = find_manifest_from(subdir)?;
        // Canonicalize both paths to handle macOS /var -> /private/var symlink
        assert_eq!(found.canonicalize()?, manifest_path.canonicalize()?);
        Ok(())
    }

    #[test]
    fn test_expand_url_basic() -> Result<()> {
        let url = "https://github.com/example/repo.git";
        let expanded = expand_url(url)?;
        assert_eq!(expanded, url);
        Ok(())
    }

    #[test]
    fn test_toml_value_to_json() {
        // String
        let toml_string = toml::Value::String("test".to_string());
        let json_string = toml_value_to_json(&toml_string);
        assert_eq!(json_string, serde_json::Value::String("test".to_string()));

        // Integer
        let toml_int = toml::Value::Integer(42);
        let json_int = toml_value_to_json(&toml_int);
        assert_eq!(json_int, serde_json::Value::Number(42.into()));

        // Boolean
        let toml_bool = toml::Value::Boolean(true);
        let json_bool = toml_value_to_json(&toml_bool);
        assert_eq!(json_bool, serde_json::Value::Bool(true));

        // Array
        let toml_array = toml::Value::Array(vec![
            toml::Value::String("a".to_string()),
            toml::Value::String("b".to_string()),
        ]);
        let json_array = toml_value_to_json(&toml_array);
        assert_eq!(
            json_array,
            serde_json::Value::Array(vec![
                serde_json::Value::String("a".to_string()),
                serde_json::Value::String("b".to_string()),
            ])
        );

        // Table
        let mut table = toml::map::Map::new();
        table.insert("key".to_string(), toml::Value::String("value".to_string()));
        let toml_table = toml::Value::Table(table);
        let json_table = toml_value_to_json(&toml_table);
        let mut expected_map = serde_json::Map::new();
        expected_map.insert("key".to_string(), serde_json::Value::String("value".to_string()));
        assert_eq!(json_table, serde_json::Value::Object(expected_map));
    }

    #[test]
    fn test_json_value_to_toml() {
        // String
        let json_string = serde_json::Value::String("test".to_string());
        let toml_string = json_value_to_toml(&json_string);
        assert_eq!(toml_string, toml::Value::String("test".to_string()));

        // Number (integer)
        let json_int = serde_json::Value::Number(42.into());
        let toml_int = json_value_to_toml(&json_int);
        assert_eq!(toml_int, toml::Value::Integer(42));

        // Number (float)
        let json_float = serde_json::Value::Number(serde_json::Number::from_f64(3.15).unwrap());
        let toml_float = json_value_to_toml(&json_float);
        assert_eq!(toml_float, toml::Value::Float(3.15));

        // Boolean
        let json_bool = serde_json::Value::Bool(true);
        let toml_bool = json_value_to_toml(&json_bool);
        assert_eq!(toml_bool, toml::Value::Boolean(true));

        // Null (converts to empty string in TOML)
        let json_null = serde_json::Value::Null;
        let toml_null = json_value_to_toml(&json_null);
        assert_eq!(toml_null, toml::Value::String(String::new()));

        // Array
        let json_array = serde_json::Value::Array(vec![
            serde_json::Value::String("a".to_string()),
            serde_json::Value::String("b".to_string()),
        ]);
        let toml_array = json_value_to_toml(&json_array);
        assert_eq!(
            toml_array,
            toml::Value::Array(vec![
                toml::Value::String("a".to_string()),
                toml::Value::String("b".to_string()),
            ])
        );

        // Object
        let mut obj = serde_json::Map::new();
        obj.insert("key".to_string(), serde_json::Value::String("value".to_string()));
        let json_obj = serde_json::Value::Object(obj);
        let toml_obj = json_value_to_toml(&json_obj);
        let mut expected_table = toml::map::Map::new();
        expected_table.insert("key".to_string(), toml::Value::String("value".to_string()));
        assert_eq!(toml_obj, toml::Value::Table(expected_table));
    }

    #[test]
    fn test_project_config() {
        let mut config_map = toml::map::Map::new();
        config_map.insert("style_guide".to_string(), toml::Value::String("docs/STYLE.md".into()));
        config_map.insert("max_line_length".to_string(), toml::Value::Integer(100));

        let config = ProjectConfig::from(config_map);
        let json = config.to_json_value();

        match json {
            serde_json::Value::Object(map) => {
                assert_eq!(
                    map.get("style_guide"),
                    Some(&serde_json::Value::String("docs/STYLE.md".to_string()))
                );
                assert_eq!(
                    map.get("max_line_length"),
                    Some(&serde_json::Value::Number(100.into()))
                );
            }
            _ => panic!("Expected JSON object"),
        }
    }

    #[test]
    fn test_get_default_tool() {
        let manifest = Manifest::new();

        // Test default values
        assert_eq!(manifest.get_default_tool(crate::core::ResourceType::Agent), "claude-code");
        assert_eq!(manifest.get_default_tool(crate::core::ResourceType::Snippet), "agpm");
        assert_eq!(manifest.get_default_tool(crate::core::ResourceType::Command), "claude-code");
    }

    #[test]
    fn test_get_default_tool_with_overrides() {
        let mut manifest = Manifest::new();
        manifest.default_tools.insert("snippets".to_string(), "claude-code".to_string());
        manifest.default_tools.insert("agents".to_string(), "opencode".to_string());

        assert_eq!(manifest.get_default_tool(crate::core::ResourceType::Agent), "opencode");
        assert_eq!(manifest.get_default_tool(crate::core::ResourceType::Snippet), "claude-code");
        assert_eq!(manifest.get_default_tool(crate::core::ResourceType::Command), "claude-code");
    }

    #[test]
    fn test_manifest_with_project_config() -> Result<()> {
        let temp = tempdir()?;
        let manifest_path = temp.path().join("agpm.toml");

        let toml_content = r#"
[sources]
community = "https://github.com/example/agpm-community.git"

[project]
style_guide = "docs/STYLE_GUIDE.md"
max_line_length = 100

[project.paths]
architecture = "docs/ARCHITECTURE.md"
"#;
        std::fs::write(&manifest_path, toml_content)?;

        let manifest = Manifest::load(&manifest_path)?;
        assert!(manifest.project.is_some());

        let project_json = manifest.project.as_ref().unwrap().to_json_value();
        match project_json {
            serde_json::Value::Object(map) => {
                assert_eq!(
                    map.get("style_guide"),
                    Some(&serde_json::Value::String("docs/STYLE_GUIDE.md".to_string()))
                );
                assert_eq!(
                    map.get("max_line_length"),
                    Some(&serde_json::Value::Number(100.into()))
                );

                // Check nested paths
                if let Some(serde_json::Value::Object(paths)) = map.get("paths") {
                    assert_eq!(
                        paths.get("architecture"),
                        Some(&serde_json::Value::String("docs/ARCHITECTURE.md".to_string()))
                    );
                } else {
                    panic!("Expected paths object");
                }
            }
            _ => panic!("Expected JSON object"),
        }
        Ok(())
    }

    #[test]
    fn test_manifest_with_default_tools() -> Result<()> {
        let temp = tempdir()?;
        let manifest_path = temp.path().join("agpm.toml");

        let toml_content = r#"
[sources]
community = "https://github.com/example/agpm-community.git"

[default-tools]
snippets = "claude-code"
agents = "opencode"
"#;
        std::fs::write(&manifest_path, toml_content)?;

        let manifest = Manifest::load(&manifest_path)?;
        assert_eq!(manifest.default_tools.get("snippets"), Some(&"claude-code".to_string()));
        assert_eq!(manifest.default_tools.get("agents"), Some(&"opencode".to_string()));

        assert_eq!(manifest.get_default_tool(crate::core::ResourceType::Snippet), "claude-code");
        assert_eq!(manifest.get_default_tool(crate::core::ResourceType::Agent), "opencode");
        Ok(())
    }

    #[test]
    fn test_is_resource_supported() {
        let manifest = Manifest::new();

        // Claude Code tool should support all resource types
        assert!(manifest.is_resource_supported("claude-code", crate::core::ResourceType::Agent));
        assert!(manifest.is_resource_supported("claude-code", crate::core::ResourceType::Snippet));
        assert!(manifest.is_resource_supported("claude-code", crate::core::ResourceType::Command));
        assert!(manifest.is_resource_supported("claude-code", crate::core::ResourceType::Script));
        assert!(manifest.is_resource_supported("claude-code", crate::core::ResourceType::Hook));
        assert!(
            manifest.is_resource_supported("claude-code", crate::core::ResourceType::McpServer)
        );

        // OpenCode tool should support some resource types
        assert!(manifest.is_resource_supported("opencode", crate::core::ResourceType::Agent));
        assert!(manifest.is_resource_supported("opencode", crate::core::ResourceType::Snippet));
        assert!(manifest.is_resource_supported("opencode", crate::core::ResourceType::Command));
        assert!(!manifest.is_resource_supported("opencode", crate::core::ResourceType::Script));
        assert!(!manifest.is_resource_supported("opencode", crate::core::ResourceType::Hook));
        assert!(manifest.is_resource_supported("opencode", crate::core::ResourceType::McpServer));

        // AGPM tool should only support snippets
        assert!(!manifest.is_resource_supported("agpm", crate::core::ResourceType::Agent));
        assert!(manifest.is_resource_supported("agpm", crate::core::ResourceType::Snippet));
        assert!(!manifest.is_resource_supported("agpm", crate::core::ResourceType::Command));
        assert!(!manifest.is_resource_supported("agpm", crate::core::ResourceType::Script));
        assert!(!manifest.is_resource_supported("agpm", crate::core::ResourceType::Hook));
        assert!(!manifest.is_resource_supported("agpm", crate::core::ResourceType::McpServer));
    }

    #[test]
    fn test_all_dependencies_with_types() {
        let mut manifest = Manifest::new();

        // Add various dependencies
        manifest.add_dependency(
            "agent1".to_string(),
            ResourceDependency::Simple("../local/agent.md".to_string()),
            true,
        );

        manifest.snippets.insert(
            "snippet1".to_string(),
            ResourceDependency::Simple("../local/snippet.md".to_string()),
        );

        manifest.commands.insert(
            "command1".to_string(),
            ResourceDependency::Simple("../local/command.md".to_string()),
        );

        let deps = manifest.all_dependencies_with_types();

        // Should have 3 dependencies
        assert_eq!(deps.len(), 3);

        // Check that we have one of each type
        let agent_deps: Vec<_> = deps
            .iter()
            .filter(|(_, _, rtype)| *rtype == crate::core::ResourceType::Agent)
            .collect();
        assert_eq!(agent_deps.len(), 1);

        let snippet_deps: Vec<_> = deps
            .iter()
            .filter(|(_, _, rtype)| *rtype == crate::core::ResourceType::Snippet)
            .collect();
        assert_eq!(snippet_deps.len(), 1);

        let command_deps: Vec<_> = deps
            .iter()
            .filter(|(_, _, rtype)| *rtype == crate::core::ResourceType::Command)
            .collect();
        assert_eq!(command_deps.len(), 1);
    }

    #[test]
    fn test_get_dependency_mut() -> Result<()> {
        let mut manifest = Manifest::new();

        manifest.add_dependency(
            "agent1".to_string(),
            ResourceDependency::Simple("../local/agent.md".to_string()),
            true,
        );

        let deps = manifest
            .get_dependencies_mut(crate::core::ResourceType::Agent)
            .ok_or_else(|| anyhow::anyhow!("Agent dependencies not found"))?;
        assert_eq!(deps.len(), 1);
        assert!(deps.contains_key("agent1"));
        Ok(())
    }

    #[test]
    fn test_add_typed_dependency() -> Result<()> {
        let mut manifest = Manifest::new();

        manifest.add_typed_dependency(
            "agent1".to_string(),
            ResourceDependency::Simple("../local/agent.md".to_string()),
            crate::core::ResourceType::Agent,
        );

        manifest.add_typed_dependency(
            "snippet1".to_string(),
            ResourceDependency::Simple("../local/snippet.md".to_string()),
            crate::core::ResourceType::Snippet,
        );

        assert!(manifest.has_dependency("agent1"));
        assert!(manifest.has_dependency("snippet1"));

        let agent_deps = manifest
            .get_dependencies_mut(crate::core::ResourceType::Agent)
            .ok_or_else(|| anyhow::anyhow!("Agent dependencies not found"))?;
        assert_eq!(agent_deps.len(), 1);

        let snippet_deps = manifest
            .get_dependencies_mut(crate::core::ResourceType::Snippet)
            .ok_or_else(|| anyhow::anyhow!("Snippet dependencies not found"))?;
        assert_eq!(snippet_deps.len(), 1);
        Ok(())
    }

    #[test]
    fn test_dependency_is_pattern() {
        let simple_dep = ResourceDependency::Simple("path/to/file.md".to_string());
        assert!(!simple_dep.is_pattern());

        let pattern_dep = ResourceDependency::Simple("path/to/*.md".to_string());
        assert!(pattern_dep.is_pattern());

        let detailed_dep = ResourceDependency::Detailed(Box::new(DetailedDependency {
            source: Some("official".to_string()),
            path: "agents/*.md".to_string(),
            version: Some("v1.0.0".to_string()),
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
        }));
        assert!(detailed_dep.is_pattern());
    }

    #[test]
    fn test_get_flatten_default() {
        let dep_with_default = ResourceDependency::Detailed(Box::new(DetailedDependency {
            source: Some("official".to_string()),
            path: "agents/test.md".to_string(),
            version: Some("v1.0.0".to_string()),
            branch: None,
            rev: None,
            command: None,
            args: None,
            target: None,
            filename: None,
            dependencies: None,
            tool: Some("claude-code".to_string()),
            flatten: None, // Not specified
            install: None,
            template_vars: Some(serde_json::Value::Object(serde_json::Map::new())),
        }));
        // When not specified, get_flatten returns None
        assert_eq!(dep_with_default.get_flatten(), None);
    }

    #[test]
    fn test_get_flatten_explicit() {
        let dep_flatten_true = ResourceDependency::Detailed(Box::new(DetailedDependency {
            source: Some("official".to_string()),
            path: "agents/*.md".to_string(),
            version: Some("v1.0.0".to_string()),
            branch: None,
            rev: None,
            command: None,
            args: None,
            target: None,
            filename: None,
            dependencies: None,
            tool: Some("claude-code".to_string()),
            flatten: Some(true),
            install: None,
            template_vars: Some(serde_json::Value::Object(serde_json::Map::new())),
        }));
        assert_eq!(dep_flatten_true.get_flatten(), Some(true));

        let dep_flatten_false = ResourceDependency::Detailed(Box::new(DetailedDependency {
            source: Some("official".to_string()),
            path: "snippets/*.md".to_string(),
            version: Some("v1.0.0".to_string()),
            branch: None,
            rev: None,
            command: None,
            args: None,
            target: None,
            filename: None,
            dependencies: None,
            tool: Some("claude-code".to_string()),
            flatten: Some(false),
            install: None,
            template_vars: Some(serde_json::Value::Object(serde_json::Map::new())),
        }));
        assert_eq!(dep_flatten_false.get_flatten(), Some(false));
    }

    #[test]
    fn test_get_install_default() {
        let dep = ResourceDependency::Detailed(Box::new(DetailedDependency {
            source: Some("official".to_string()),
            path: "agents/test.md".to_string(),
            version: Some("v1.0.0".to_string()),
            branch: None,
            rev: None,
            command: None,
            args: None,
            target: None,
            filename: None,
            dependencies: None,
            tool: Some("claude-code".to_string()),
            flatten: None,
            install: None, // Not specified - defaults to true
            template_vars: Some(serde_json::Value::Object(serde_json::Map::new())),
        }));
        assert_eq!(dep.get_install(), None); // Returns None when not specified
    }

    #[test]
    fn test_get_install_explicit() {
        let dep_install_false = ResourceDependency::Detailed(Box::new(DetailedDependency {
            source: Some("official".to_string()),
            path: "snippets/helper.md".to_string(),
            version: Some("v1.0.0".to_string()),
            branch: None,
            rev: None,
            command: None,
            args: None,
            target: None,
            filename: None,
            dependencies: None,
            tool: Some("claude-code".to_string()),
            flatten: None,
            install: Some(false), // Explicitly disabled
            template_vars: Some(serde_json::Value::Object(serde_json::Map::new())),
        }));
        assert_eq!(dep_install_false.get_install(), Some(false));

        let dep_install_true = ResourceDependency::Detailed(Box::new(DetailedDependency {
            source: Some("official".to_string()),
            path: "snippets/helper.md".to_string(),
            version: Some("v1.0.0".to_string()),
            branch: None,
            rev: None,
            command: None,
            args: None,
            target: None,
            filename: None,
            dependencies: None,
            tool: Some("claude-code".to_string()),
            flatten: None,
            install: Some(true), // Explicitly enabled
            template_vars: Some(serde_json::Value::Object(serde_json::Map::new())),
        }));
        assert_eq!(dep_install_true.get_install(), Some(true));
    }

    #[test]
    fn test_get_template_vars() {
        let dep_no_vars = ResourceDependency::Detailed(Box::new(DetailedDependency {
            source: Some("official".to_string()),
            path: "agents/test.md".to_string(),
            version: Some("v1.0.0".to_string()),
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
            template_vars: None,
        }));
        assert_eq!(dep_no_vars.get_template_vars(), None);

        let vars = serde_json::json!({
            "project": {
                "language": "python"
            }
        });

        let dep_with_vars = ResourceDependency::Detailed(Box::new(DetailedDependency {
            source: Some("official".to_string()),
            path: "agents/test.md".to_string(),
            version: Some("v1.0.0".to_string()),
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
            template_vars: Some(vars.clone()),
        }));
        assert_eq!(dep_with_vars.get_template_vars(), Some(&vars));
    }
}
