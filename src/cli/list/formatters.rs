use anyhow::Result;
use colored::Colorize;
use std::collections::{BTreeMap, HashMap};

use crate::cache::Cache;
use crate::lockfile::LockFile;
use crate::lockfile::patch_display::extract_patch_displays;

/// Configuration for output formatting options
#[derive(Debug, Clone)]
pub struct OutputConfig {
    pub title: String,
    pub format: String,
    pub files: bool,
    pub detailed: bool,
    pub verbose: bool,
    pub should_show_agents: bool,
    pub should_show_snippets: bool,
}

impl Default for OutputConfig {
    fn default() -> Self {
        Self {
            title: "Installed Resources".to_string(),
            format: "table".to_string(),
            files: false,
            detailed: false,
            verbose: false,
            should_show_agents: true,
            should_show_snippets: true,
        }
    }
}

/// Internal representation for list items used in various output formats.
///
/// This struct normalizes resource information from both agents and snippets
/// in the lockfile to provide a consistent view for display purposes.
#[derive(Debug, Clone)]
pub struct ListItem {
    /// The name/key of the resource as defined in the manifest
    pub name: String,
    /// The source repository name (if from a Git source)
    pub source: Option<String>,
    /// The version/tag/branch of the resource
    pub version: Option<String>,
    /// The path within the source repository
    pub path: Option<String>,
    /// The type of resource ("agent" or "snippet")
    pub resource_type: String,
    /// The local installation path where the resource file is located
    pub installed_at: Option<String>,
    /// The SHA-256 checksum of the installed resource file
    pub checksum: Option<String>,
    /// The resolved Git commit hash
    pub resolved_commit: Option<String>,
    /// The tool ("claude-code", "opencode", "agpm", or custom)
    pub tool: Option<String>,
    /// Patches that were applied to this resource
    pub applied_patches: std::collections::BTreeMap<String, toml::Value>,
}

/// Output items in the specified format
pub fn output_items(items: &[ListItem], config: &OutputConfig) -> Result<()> {
    if items.is_empty() {
        if config.format == "json" {
            println!("{{}}");
        } else {
            println!("No installed resources found.");
        }
        return Ok(());
    }

    match config.format.as_str() {
        "json" => output_json(items)?,
        "yaml" => output_yaml(items)?,
        "compact" => output_compact(items),
        "simple" => output_simple(items),
        _ => output_table(items, config),
    }

    Ok(())
}

/// Output items in detailed mode with patch comparisons
pub async fn output_items_detailed(
    items: &[ListItem],
    title: &str,
    lockfile: &LockFile,
    cache: Option<&Cache>,
    should_show_agents: bool,
    should_show_snippets: bool,
) -> Result<()> {
    if items.is_empty() {
        println!("{{}}");
        return Ok(());
    }

    println!("{}", title.bold());
    println!();

    // Group by resource type
    if should_show_agents {
        let agents: Vec<_> = items.iter().filter(|i| i.resource_type == "agent").collect();
        if !agents.is_empty() {
            println!("{}:", "Agents".cyan().bold());
            for item in agents {
                print_item_detailed(item, lockfile, cache).await;
            }
            println!();
        }
    }

    if should_show_snippets {
        let snippets: Vec<_> = items.iter().filter(|i| i.resource_type == "snippet").collect();
        if !snippets.is_empty() {
            println!("{}:", "Snippets".cyan().bold());
            for item in snippets {
                print_item_detailed(item, lockfile, cache).await;
            }
        }
    }

    println!("{}: {} resources", "Total".green().bold(), items.len());

    Ok(())
}

/// Output in JSON format
fn output_json(items: &[ListItem]) -> Result<()> {
    let json_items: Vec<serde_json::Value> = items
        .iter()
        .map(|item| {
            let mut obj = serde_json::json!({
                "name": item.name,
                "type": item.resource_type,
                "tool": item.tool
            });

            if let Some(ref source) = item.source {
                obj["source"] = serde_json::Value::String(source.clone());
            }
            if let Some(ref version) = item.version {
                obj["version"] = serde_json::Value::String(version.clone());
            }
            if let Some(ref path) = item.path {
                obj["path"] = serde_json::Value::String(path.clone());
            }
            if let Some(ref installed_at) = item.installed_at {
                obj["installed_at"] = serde_json::Value::String(installed_at.clone());
            }
            if let Some(ref checksum) = item.checksum {
                obj["checksum"] = serde_json::Value::String(checksum.clone());
            }

            obj
        })
        .collect();

    println!("{}", serde_json::to_string_pretty(&json_items)?);
    Ok(())
}

/// Output in YAML format
fn output_yaml(items: &[ListItem]) -> Result<()> {
    let yaml_items: Vec<HashMap<String, serde_yaml::Value>> = items
        .iter()
        .map(|item| {
            let mut obj = HashMap::new();
            obj.insert("name".to_string(), serde_yaml::Value::String(item.name.clone()));
            obj.insert("type".to_string(), serde_yaml::Value::String(item.resource_type.clone()));
            obj.insert(
                "tool".to_string(),
                serde_yaml::Value::String(item.tool.clone().expect("Tool should always be set")),
            );

            if let Some(ref source) = item.source {
                obj.insert("source".to_string(), serde_yaml::Value::String(source.clone()));
            }
            if let Some(ref version) = item.version {
                obj.insert("version".to_string(), serde_yaml::Value::String(version.clone()));
            }
            if let Some(ref path) = item.path {
                obj.insert("path".to_string(), serde_yaml::Value::String(path.clone()));
            }
            if let Some(ref installed_at) = item.installed_at {
                obj.insert(
                    "installed_at".to_string(),
                    serde_yaml::Value::String(installed_at.clone()),
                );
            }

            obj
        })
        .collect();

    println!("{}", serde_yaml::to_string(&yaml_items)?);
    Ok(())
}

/// Output in compact format
fn output_compact(items: &[ListItem]) {
    for item in items {
        let source = item.source.as_deref().unwrap_or("local");
        let version = item.version.as_deref().unwrap_or("latest");
        println!("{} {} {}", item.name, version, source);
    }
}

/// Output in simple format
fn output_simple(items: &[ListItem]) {
    for item in items {
        println!("{} ({}))", item.name, item.resource_type);
    }
}

/// Output in table format
fn output_table(items: &[ListItem], config: &OutputConfig) {
    println!("{}", config.title.bold());
    println!();

    // Show headers for table format (but not verbose mode)
    if !items.is_empty() && config.format == "table" && !config.verbose {
        println!(
            "{:<32} {:<15} {:<15} {:<12} {:<15}",
            "Name".cyan().bold(),
            "Version".cyan().bold(),
            "Source".cyan().bold(),
            "Type".cyan().bold(),
            "Artifact".cyan().bold()
        );
        println!("{}", "-".repeat(92).bright_black());
    }

    if config.format == "table" && !config.files && !config.detailed && !config.verbose {
        // Print items directly in table format
        for item in items {
            print_item(item, &config.format, config.files, config.detailed);
        }
    } else {
        // Simple listing
        if config.should_show_agents {
            let agents: Vec<_> = items.iter().filter(|i| i.resource_type == "agent").collect();
            if !agents.is_empty() {
                println!("{}:", "Agents".cyan().bold());
                for item in agents {
                    print_item(item, &config.format, config.files, config.detailed);
                }
                println!();
            }
        }

        if config.should_show_snippets {
            let snippets: Vec<_> = items.iter().filter(|i| i.resource_type == "snippet").collect();
            if !snippets.is_empty() {
                println!("{}:", "Snippets".cyan().bold());
                for item in snippets {
                    print_item(item, &config.format, config.files, config.detailed);
                }
            }
        }
    }

    println!("{}: {} resources", "Total".green().bold(), items.len());
}

/// Print a single item in detailed mode with patch comparison
async fn print_item_detailed(item: &ListItem, lockfile: &LockFile, cache: Option<&Cache>) {
    let source = item.source.as_deref().unwrap_or("local");
    let version = item.version.as_deref().unwrap_or("latest");

    println!("    {}", item.name.bright_white());
    println!("      Source: {}", source.bright_black());
    println!("      Version: {}", version.yellow());
    if let Some(ref path) = item.path {
        println!("      Path: {}", path.bright_black());
    }
    if let Some(ref installed_at) = item.installed_at {
        println!("      Installed at: {}", installed_at.bright_black());
    }
    if let Some(ref checksum) = item.checksum {
        println!("      Checksum: {}", checksum.bright_black());
    }

    // Show patches with original → overridden comparison
    if !item.applied_patches.is_empty() {
        println!("      Applied patches:");

        // If we have cache, try to get original values
        if let Some(cache) = cache {
            // Find the locked resource for this item
            if let Some(locked_resource) = find_locked_resource(item, lockfile) {
                let patch_displays = extract_patch_displays(locked_resource, cache).await;
                for display in patch_displays {
                    let formatted = display.format();
                    // Indent each line of the multi-line diff output
                    for (i, line) in formatted.lines().enumerate() {
                        if i == 0 {
                            // First line: bullet point
                            println!("        • {}", line);
                        } else {
                            // Subsequent lines: indent to align with content
                            println!("          {}", line);
                        }
                    }
                }
            } else {
                // Fallback: just show overridden values
                print_patches_fallback(&item.applied_patches);
            }
        } else {
            // No cache: just show overridden values
            print_patches_fallback(&item.applied_patches);
        }
    }
    println!();
}

/// Fallback patch display without original values
fn print_patches_fallback(patches: &BTreeMap<String, toml::Value>) {
    let mut patch_keys: Vec<_> = patches.keys().collect();
    patch_keys.sort();
    for key in patch_keys {
        let value = &patches[key];
        let formatted_value = format_patch_value(value);
        println!("        • {}: {}", key.blue(), formatted_value);
    }
}

/// Find the locked resource corresponding to a list item
fn find_locked_resource<'a>(
    item: &ListItem,
    lockfile: &'a LockFile,
) -> Option<&'a crate::lockfile::LockedResource> {
    // Determine resource type
    let resource_type = match item.resource_type.as_str() {
        "agent" => crate::core::ResourceType::Agent,
        "snippet" => crate::core::ResourceType::Snippet,
        "command" => crate::core::ResourceType::Command,
        "script" => crate::core::ResourceType::Script,
        "hook" => crate::core::ResourceType::Hook,
        "mcp-server" => crate::core::ResourceType::McpServer,
        _ => return None,
    };

    // Find matching resource in lockfile
    lockfile.get_resources(&resource_type).iter().find(|r| r.name == item.name)
}

/// Print a single item
fn print_item(item: &ListItem, format: &str, files: bool, detailed: bool) {
    let source = item.source.as_deref().unwrap_or("local");
    let version = item.version.as_deref().unwrap_or("latest");

    if format == "table" && !files && !detailed {
        // Table format with columns
        // Build the name field with proper padding before adding colors
        let name_with_indicator = if !item.applied_patches.is_empty() {
            format!("{} (patched)", item.name)
        } else {
            item.name.clone()
        };

        // Apply padding to plain text, then colorize
        let name_field = format!("{:<32}", name_with_indicator);
        let colored_name = name_field.bright_white();

        println!(
            "{} {:<15} {:<15} {:<12} {:<15}",
            colored_name,
            version.yellow(),
            source.bright_black(),
            item.resource_type.bright_white(),
            item.tool.clone().expect("Tool should always be set").bright_black()
        );
    } else if files {
        if let Some(ref installed_at) = item.installed_at {
            println!("    {}", installed_at.bright_black());
        } else if let Some(ref path) = item.path {
            println!("    {}", path.bright_black());
        }
    } else if detailed {
        println!("    {}", item.name.bright_white());
        println!("      Source: {}", source.bright_black());
        println!("      Version: {}", version.yellow());
        if let Some(ref path) = item.path {
            println!("      Path: {}", path.bright_black());
        }
        if let Some(ref installed_at) = item.installed_at {
            println!("      Installed at: {}", installed_at.bright_black());
        }
        if let Some(ref checksum) = item.checksum {
            println!("      Checksum: {}", checksum.bright_black());
        }
        if !item.applied_patches.is_empty() {
            println!("      {}", "Patches:".cyan());
            let mut patch_keys: Vec<_> = item.applied_patches.keys().collect();
            patch_keys.sort(); // Sort for consistent display
            for key in patch_keys {
                let value = &item.applied_patches[key];
                let formatted_value = format_patch_value(value);
                println!("        {}: {}", key.yellow(), formatted_value.green());
            }
        }
        println!();
    } else {
        let commit_info = if let Some(ref commit) = item.resolved_commit {
            format!("@{}", &commit[..7.min(commit.len())])
        } else {
            String::new()
        };

        println!(
            "    {} {} {} {}",
            item.name.bright_white(),
            format!("({source}))").bright_black(),
            version.yellow(),
            commit_info.bright_black()
        );

        if let Some(ref installed_at) = item.installed_at {
            println!("      → {}", installed_at.bright_black());
        }
    }
}

/// Format a toml::Value for display in patch output.
///
/// Produces clean, readable output:
/// - Strings: wrapped in quotes `"value"`
/// - Numbers/Booleans: plain text
/// - Arrays/Tables: formatted as TOML syntax
pub fn format_patch_value(value: &toml::Value) -> String {
    match value {
        toml::Value::String(s) => format!("\"{}\"", s),
        toml::Value::Integer(i) => i.to_string(),
        toml::Value::Float(f) => f.to_string(),
        toml::Value::Boolean(b) => b.to_string(),
        toml::Value::Array(arr) => {
            let elements: Vec<String> = arr.iter().map(format_patch_value).collect();
            format!("[{}]", elements.join(", "))
        }
        toml::Value::Table(_) | toml::Value::Datetime(_) => {
            // For complex types, use to_string() as fallback
            value.to_string()
        }
    }
}
