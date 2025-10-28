#[cfg(test)]
mod tests {

    use crate::manifest::{
        DetailedDependency, Manifest, ProjectConfig, ResourceDependency, expand_url, find_manifest,
        json_value_to_toml, toml_value_to_json,
    };

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
    fn test_manifest_load_save() {
        let temp = tempdir().unwrap();
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

        manifest.save(&manifest_path).unwrap();

        let loaded = Manifest::load(&manifest_path).unwrap();
        assert_eq!(loaded.sources.len(), 1);
        assert_eq!(loaded.agents.len(), 1);
        assert!(loaded.has_dependency("test-agent"));
    }

    #[test]
    fn test_manifest_validation() {
        let mut manifest = Manifest::new();

        // Add dependency without source - should be valid (local dependency)
        manifest.add_dependency(
            "local-agent".to_string(),
            ResourceDependency::Simple("../local/agent.md".to_string()),
            true,
        );
        assert!(manifest.validate().is_ok());

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
        assert!(manifest.validate().is_ok());
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
    fn test_find_manifest_basic() {
        let temp = tempdir().unwrap();
        let manifest_path = temp.path().join("agpm.toml");
        std::fs::write(&manifest_path, "[sources]\n").unwrap();

        // Should find the manifest we just created
        std::env::set_current_dir(temp.path()).unwrap();
        let found = find_manifest().unwrap();
        // Canonicalize both paths to handle macOS /var -> /private/var symlink
        assert_eq!(found.canonicalize().unwrap(), manifest_path.canonicalize().unwrap());
    }

    #[test]
    fn test_find_manifest_parent() {
        let temp = tempdir().unwrap();
        let manifest_path = temp.path().join("agpm.toml");
        std::fs::write(&manifest_path, "[sources]\n").unwrap();

        // Create a subdirectory
        let subdir = temp.path().join("subdir");
        std::fs::create_dir(&subdir).unwrap();

        // Should find the manifest in parent directory
        std::env::set_current_dir(&subdir).unwrap();
        let found = find_manifest().unwrap();
        // Canonicalize both paths to handle macOS /var -> /private/var symlink
        assert_eq!(found.canonicalize().unwrap(), manifest_path.canonicalize().unwrap());
    }

    #[test]
    fn test_expand_url_basic() {
        let url = "https://github.com/example/repo.git";
        let expanded = expand_url(url).unwrap();
        assert_eq!(expanded, url);
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
    fn test_manifest_with_project_config() {
        let temp = tempdir().unwrap();
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
        std::fs::write(&manifest_path, toml_content).unwrap();

        let manifest = Manifest::load(&manifest_path).unwrap();
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
    }

    #[test]
    fn test_manifest_with_default_tools() {
        let temp = tempdir().unwrap();
        let manifest_path = temp.path().join("agpm.toml");

        let toml_content = r#"
[sources]
community = "https://github.com/example/agpm-community.git"

[default-tools]
snippets = "claude-code"
agents = "opencode"
"#;
        std::fs::write(&manifest_path, toml_content).unwrap();

        let manifest = Manifest::load(&manifest_path).unwrap();
        assert_eq!(manifest.default_tools.get("snippets"), Some(&"claude-code".to_string()));
        assert_eq!(manifest.default_tools.get("agents"), Some(&"opencode".to_string()));

        assert_eq!(manifest.get_default_tool(crate::core::ResourceType::Snippet), "claude-code");
        assert_eq!(manifest.get_default_tool(crate::core::ResourceType::Agent), "opencode");
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
    fn test_get_dependency_mut() {
        let mut manifest = Manifest::new();

        manifest.add_dependency(
            "agent1".to_string(),
            ResourceDependency::Simple("../local/agent.md".to_string()),
            true,
        );

        let deps = manifest.get_dependencies_mut(crate::core::ResourceType::Agent).unwrap();
        assert_eq!(deps.len(), 1);
        assert!(deps.contains_key("agent1"));
    }

    #[test]
    fn test_add_typed_dependency() {
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

        let agent_deps = manifest.get_dependencies_mut(crate::core::ResourceType::Agent).unwrap();
        assert_eq!(agent_deps.len(), 1);

        let snippet_deps =
            manifest.get_dependencies_mut(crate::core::ResourceType::Snippet).unwrap();
        assert_eq!(snippet_deps.len(), 1);
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

#[cfg(test)]
mod tool_tests {

    use crate::manifest::Manifest;
    use std::path::PathBuf;

    #[test]
    fn test_tools_config_default() {
        let manifest = Manifest::new();
        let tools = manifest.get_tools_config();

        // Should have default tools in the types map
        assert!(tools.types.contains_key("claude-code"));
        assert!(tools.types.contains_key("opencode"));
        assert!(tools.types.contains_key("agpm"));
    }

    #[test]
    fn test_get_artifact_resource_path_agents() {
        let manifest = Manifest::new();

        let path = manifest
            .get_artifact_resource_path("claude-code", crate::core::ResourceType::Agent)
            .unwrap();
        // Use platform-specific path comparison
        #[cfg(windows)]
        assert_eq!(path.to_str().unwrap(), r".claude\agents");
        #[cfg(not(windows))]
        assert_eq!(path.to_str().unwrap(), ".claude/agents");

        let path = manifest
            .get_artifact_resource_path("opencode", crate::core::ResourceType::Agent)
            .unwrap();
        #[cfg(windows)]
        assert_eq!(path.to_str().unwrap(), r".opencode\agent");
        #[cfg(not(windows))]
        assert_eq!(path.to_str().unwrap(), ".opencode/agent");
    }

    #[test]
    fn test_get_artifact_resource_path_snippets() {
        let manifest = Manifest::new();

        let path = manifest
            .get_artifact_resource_path("agpm", crate::core::ResourceType::Snippet)
            .unwrap();
        #[cfg(windows)]
        assert_eq!(path.to_str().unwrap(), r".agpm\snippets");
        #[cfg(not(windows))]
        assert_eq!(path.to_str().unwrap(), ".agpm/snippets");

        // Claude Code also supports snippets (for override cases)
        let path = manifest
            .get_artifact_resource_path("claude-code", crate::core::ResourceType::Snippet)
            .unwrap();
        #[cfg(windows)]
        assert_eq!(path.to_str().unwrap(), r".claude\snippets");
        #[cfg(not(windows))]
        assert_eq!(path.to_str().unwrap(), ".claude/snippets");
    }

    #[test]
    fn test_get_artifact_resource_path_commands() {
        let manifest = Manifest::new();

        let path = manifest
            .get_artifact_resource_path("claude-code", crate::core::ResourceType::Command)
            .unwrap();
        #[cfg(windows)]
        assert_eq!(path.to_str().unwrap(), r".claude\commands");
        #[cfg(not(windows))]
        assert_eq!(path.to_str().unwrap(), ".claude/commands");

        let path = manifest
            .get_artifact_resource_path("opencode", crate::core::ResourceType::Command)
            .unwrap();
        #[cfg(windows)]
        assert_eq!(path.to_str().unwrap(), r".opencode\command");
        #[cfg(not(windows))]
        assert_eq!(path.to_str().unwrap(), ".opencode/command");
    }

    #[test]
    fn test_get_artifact_resource_path_unsupported() {
        let manifest = Manifest::new();

        // AGPM doesn't support agents
        let result = manifest.get_artifact_resource_path("agpm", crate::core::ResourceType::Agent);
        assert!(result.is_none());

        // OpenCode doesn't support scripts
        let result =
            manifest.get_artifact_resource_path("opencode", crate::core::ResourceType::Script);
        assert!(result.is_none());
    }

    #[test]
    fn test_get_merge_target_hooks() {
        let manifest = Manifest::new();

        let merge_target =
            manifest.get_merge_target("claude-code", crate::core::ResourceType::Hook).unwrap();
        assert_eq!(merge_target, PathBuf::from(".claude/settings.local.json"));
    }

    #[test]
    fn test_get_merge_target_mcp_servers() {
        let manifest = Manifest::new();

        let merge_target =
            manifest.get_merge_target("claude-code", crate::core::ResourceType::McpServer).unwrap();
        assert_eq!(merge_target, PathBuf::from(".mcp.json"));

        let merge_target =
            manifest.get_merge_target("opencode", crate::core::ResourceType::McpServer).unwrap();
        assert_eq!(merge_target, PathBuf::from(".opencode/opencode.json"));
    }

    #[test]
    fn test_get_merge_target_non_mergeable() {
        let manifest = Manifest::new();

        // Agents don't have merge targets
        let result = manifest.get_merge_target("claude-code", crate::core::ResourceType::Agent);
        assert!(result.is_none());

        // Commands don't have merge targets
        let result = manifest.get_merge_target("opencode", crate::core::ResourceType::Command);
        assert!(result.is_none());
    }
}

#[cfg(test)]
mod flatten_tests {

    use crate::manifest::{DetailedDependency, ResourceDependency};

    #[test]
    fn test_flatten_field_agents() {
        let dep = ResourceDependency::Detailed(Box::new(DetailedDependency {
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
            flatten: Some(false), // Override default
            install: None,
            template_vars: Some(serde_json::Value::Object(serde_json::Map::new())),
        }));

        assert_eq!(dep.get_flatten(), Some(false));
    }

    #[test]
    fn test_flatten_field_snippets() {
        let dep = ResourceDependency::Detailed(Box::new(DetailedDependency {
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
            tool: Some("agpm".to_string()),
            flatten: Some(true), // Override default
            install: None,
            template_vars: Some(serde_json::Value::Object(serde_json::Map::new())),
        }));

        assert_eq!(dep.get_flatten(), Some(true));
    }
}

#[cfg(test)]
mod validation_tests {

    use crate::manifest::{DetailedDependency, Manifest, ResourceDependency};
    use tempfile::tempdir;

    #[test]
    fn test_validate_patches_success() {
        let temp = tempdir().unwrap();
        let manifest_path = temp.path().join("agpm.toml");

        // Create manifest with valid patches
        let toml_content = r#"
[sources]
community = "https://github.com/example/agpm-community.git"

[agents]
test-agent = { source = "community", path = "agents/test.md", version = "v1.0.0" }

[patch.agents.test-agent]
model = "claude-3-haiku"
temperature = "0.8"
"#;
        std::fs::write(&manifest_path, toml_content).unwrap();

        let manifest = Manifest::load(&manifest_path).unwrap();
        assert!(manifest.validate().is_ok());
    }

    #[test]
    fn test_validate_patches_unknown_dependency() {
        let temp = tempdir().unwrap();
        let manifest_path = temp.path().join("agpm.toml");

        // Create manifest with patch for non-existent dependency
        let toml_content = r#"
[sources]
community = "https://github.com/example/agpm-community.git"

[agents]
test-agent = { source = "community", path = "agents/test.md", version = "v1.0.0" }

[patch.agents.non-existent]
model = "claude-3-haiku"
"#;
        std::fs::write(&manifest_path, toml_content).unwrap();

        // load() now calls validate() automatically, so it should fail
        let result = Manifest::load(&manifest_path);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Patch references unknown"));
    }

    #[test]
    fn test_validate_sources() {
        let mut manifest = Manifest::new();

        // Add dependency without source
        manifest.add_dependency(
            "local".to_string(),
            ResourceDependency::Simple("../local/agent.md".to_string()),
            true,
        );
        assert!(manifest.validate().is_ok());

        // Add dependency with undefined source
        manifest.add_dependency(
            "remote".to_string(),
            ResourceDependency::Detailed(Box::new(DetailedDependency {
                source: Some("undefined".to_string()),
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
        assert!(manifest.validate().is_err());

        // Add the source
        manifest
            .add_source("undefined".to_string(), "https://github.com/test/repo.git".to_string());
        assert!(manifest.validate().is_ok());
    }

    #[test]
    fn test_validate_version_constraints() {
        let mut manifest = Manifest::new();
        manifest.add_source("test".to_string(), "https://github.com/test/repo.git".to_string());

        // Remote dependency without version is now OK (defaults to HEAD)
        manifest.add_dependency(
            "no-version".to_string(),
            ResourceDependency::Detailed(Box::new(DetailedDependency {
                source: Some("test".to_string()),
                path: "agents/test.md".to_string(),
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
            true,
        );
        assert!(manifest.validate().is_ok()); // Git deps default to HEAD now

        // Adding version should fix it
        manifest.agents.remove("no-version");
        manifest.add_dependency(
            "with-version".to_string(),
            ResourceDependency::Detailed(Box::new(DetailedDependency {
                source: Some("test".to_string()),
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
        assert!(manifest.validate().is_ok());
    }
}

#[cfg(test)]
mod template_vars_tests {

    use crate::manifest::{DetailedDependency, Manifest, ResourceDependency};
    use serde_json::json;
    use tempfile::tempdir;

    #[test]
    fn test_template_vars_in_dependency() {
        let vars = json!({
            "project": {
                "language": "python"
            }
        });

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
            install: None,
            template_vars: Some(vars.clone()),
        }));

        assert_eq!(dep.get_template_vars(), Some(&vars));
    }

    #[test]
    fn test_template_vars_serialization() {
        let temp = tempdir().unwrap();
        let manifest_path = temp.path().join("agpm.toml");

        let toml_content = r#"
[sources]
community = "https://github.com/example/agpm-community.git"

[agents.python-dev]
source = "community"
path = "agents/generic-dev.md"
version = "v1.0.0"
tool = "claude-code"

[agents.python-dev.template_vars]
project = { language = "python", framework = "fastapi" }
"#;
        std::fs::write(&manifest_path, toml_content).unwrap();

        let manifest = Manifest::load(&manifest_path).unwrap();
        let dep = manifest.agents.get("python-dev").unwrap();

        let vars = dep.get_template_vars().unwrap();
        assert_eq!(
            vars.get("project").and_then(|p| p.get("language")).and_then(|l| l.as_str()),
            Some("python")
        );
        assert_eq!(
            vars.get("project").and_then(|p| p.get("framework")).and_then(|f| f.as_str()),
            Some("fastapi")
        );
    }

    #[test]
    fn test_template_vars_empty() {
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
            install: None,
            template_vars: None,
        }));

        assert_eq!(dep.get_template_vars(), None);
    }

    #[test]
    fn test_template_vars_inline_table_with_multiple_keys() {
        let temp = tempdir().unwrap();
        let manifest_path = temp.path().join("agpm.toml");

        // Test inline table format with multiple top-level keys (project AND config)
        let toml_content = r#"
[sources]
test-repo = "https://example.com/repo.git"

[agents]
templated = { source = "test-repo", path = "agents/templated-agent.md", version = "v1.0.0", template_vars = { project = { name = "Production" }, config = { model = "claude-3-opus", temperature = 0.5 } } }
"#;
        std::fs::write(&manifest_path, toml_content).unwrap();

        let manifest = Manifest::load(&manifest_path).unwrap();
        let dep = manifest.agents.get("templated").unwrap();

        let vars = dep.get_template_vars().unwrap();

        // Debug: print the vars
        println!("Parsed template_vars: {}", serde_json::to_string_pretty(vars).unwrap());

        // Verify project key
        assert!(vars.get("project").is_some(), "project should be present");
        assert_eq!(
            vars.get("project").and_then(|p| p.get("name")).and_then(|n| n.as_str()),
            Some("Production")
        );

        // Verify config key
        assert!(vars.get("config").is_some(), "config should be present in template_vars");
        assert_eq!(
            vars.get("config").and_then(|c| c.get("model")).and_then(|m| m.as_str()),
            Some("claude-3-opus")
        );
        assert_eq!(
            vars.get("config").and_then(|c| c.get("temperature")).and_then(|t| t.as_f64()),
            Some(0.5)
        );
    }
}
