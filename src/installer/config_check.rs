//! Configuration validation for AGPM installations.
//!
//! Validates that the project is correctly configured for AGPM:
//! - Required entries exist in .gitignore
//! - Claude Code settings allow access to gitignored files

use std::collections::HashSet;
use std::path::Path;

use crate::core::ResourceType;
use crate::lockfile::LockFile;

/// Result of configuration validation.
#[derive(Debug, Default)]
pub struct ConfigValidation {
    /// Missing .gitignore entries.
    pub missing_gitignore_entries: Vec<String>,
    /// Whether Claude Code settings are correctly configured.
    pub claude_settings_ok: bool,
    /// Warning message for Claude Code settings (if not ok).
    pub claude_settings_warning: Option<String>,
}

impl ConfigValidation {
    /// Returns true if all configuration is valid.
    pub fn is_valid(&self) -> bool {
        self.missing_gitignore_entries.is_empty() && self.claude_settings_ok
    }

    /// Print warnings for any configuration issues.
    pub fn print_warnings(&self) {
        if !self.missing_gitignore_entries.is_empty() {
            eprintln!("\nWarning: The following entries are missing from .gitignore:");
            for entry in &self.missing_gitignore_entries {
                eprintln!("  {}", entry);
            }
            eprintln!("\nAdd them to prevent AGPM artifacts from being committed.");
        }

        if let Some(warning) = &self.claude_settings_warning {
            eprintln!("\n{}", warning);
        }
    }
}

/// Validate project configuration for AGPM.
///
/// Checks:
/// 1. Required .gitignore entries based on installed resource types (if gitignore_enabled)
/// 2. Claude Code settings for accessing gitignored files
///
/// # Arguments
///
/// * `project_dir` - Path to the project directory
/// * `lockfile` - The lockfile containing installed resources
/// * `gitignore_enabled` - Whether gitignore validation is enabled (from manifest)
pub fn validate_config(
    project_dir: &Path,
    lockfile: &LockFile,
    gitignore_enabled: bool,
) -> ConfigValidation {
    // Check gitignore entries only if enabled
    let missing_gitignore_entries = if gitignore_enabled {
        check_gitignore_entries(project_dir, lockfile)
    } else {
        Vec::new()
    };

    // Check Claude Code settings
    let (claude_settings_ok, claude_settings_warning) = check_claude_settings(project_dir);

    ConfigValidation {
        missing_gitignore_entries,
        claude_settings_ok,
        claude_settings_warning,
    }
}

/// Check if required .gitignore entries exist.
///
/// Returns list of missing entries.
fn check_gitignore_entries(project_dir: &Path, lockfile: &LockFile) -> Vec<String> {
    let gitignore_path = project_dir.join(".gitignore");
    let gitignore_content = match std::fs::read_to_string(&gitignore_path) {
        Ok(content) => content,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => {
            tracing::warn!("Failed to read .gitignore: {}", e);
            return Vec::new();
        }
    };

    // Parse gitignore into a set of entries (normalized)
    let existing_entries: HashSet<String> = gitignore_content
        .lines()
        .map(|line| line.trim())
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(normalize_gitignore_entry)
        .collect();

    let mut missing = Vec::new();

    // Determine which resource types are installed
    let installed_types = get_installed_resource_types(lockfile);

    // Check Claude Code entries
    if installed_types.contains(&ResourceType::Agent) {
        check_entry(&existing_entries, ".claude/agents/agpm/", &mut missing);
    }
    if installed_types.contains(&ResourceType::Command) {
        check_entry(&existing_entries, ".claude/commands/agpm/", &mut missing);
    }
    if installed_types.contains(&ResourceType::Snippet) {
        check_entry(&existing_entries, ".claude/snippets/agpm/", &mut missing);
        check_entry(&existing_entries, ".agpm/snippets/", &mut missing);
    }
    if installed_types.contains(&ResourceType::Script) {
        check_entry(&existing_entries, ".claude/scripts/agpm/", &mut missing);
    }

    // Always check for private config files
    check_entry(&existing_entries, "agpm.private.toml", &mut missing);
    check_entry(&existing_entries, "agpm.private.lock", &mut missing);

    missing
}

fn normalize_gitignore_entry(entry: &str) -> String {
    // Remove leading/trailing slashes for comparison
    entry.trim_matches('/').to_string()
}

fn check_entry(existing: &HashSet<String>, expected: &str, missing: &mut Vec<String>) {
    let normalized = normalize_gitignore_entry(expected);

    // First check for exact match
    if existing.contains(&normalized) {
        return;
    }

    // Check if any existing pattern matches the expected entry using glob
    for pattern in existing {
        // Check if pattern contains glob characters
        if pattern.contains('*') || pattern.contains('?') || pattern.contains('[') {
            // Use glob matching: pattern matches expected
            if let Ok(glob_pattern) = glob::Pattern::new(pattern) {
                if glob_pattern.matches(&normalized) {
                    return;
                }
            }
        }
    }

    missing.push(expected.to_string());
}

fn get_installed_resource_types(lockfile: &LockFile) -> HashSet<ResourceType> {
    let mut types = HashSet::new();

    if !lockfile.agents.is_empty() {
        types.insert(ResourceType::Agent);
    }
    if !lockfile.snippets.is_empty() {
        types.insert(ResourceType::Snippet);
    }
    if !lockfile.commands.is_empty() {
        types.insert(ResourceType::Command);
    }
    if !lockfile.scripts.is_empty() {
        types.insert(ResourceType::Script);
    }

    types
}

/// Check if Claude Code settings are configured for AGPM.
///
/// Returns (is_ok, optional_warning_message)
fn check_claude_settings(project_dir: &Path) -> (bool, Option<String>) {
    let settings_path = project_dir.join(".claude/settings.json");

    if !settings_path.exists() {
        return (
            false,
            Some(
                "Warning: Claude Code settings not configured for AGPM.\n\n\
                 Create .claude/settings.json with:\n\
                 {\n  \"respectGitIgnore\": false\n}\n\n\
                 This allows Claude Code to access gitignored AGPM resources."
                    .to_string(),
            ),
        );
    }

    // Read and parse settings
    let content = match std::fs::read_to_string(&settings_path) {
        Ok(c) => c,
        Err(_) => {
            return (false, Some("Warning: Could not read .claude/settings.json".to_string()));
        }
    };

    let json: serde_json::Value = match serde_json::from_str(&content) {
        Ok(v) => v,
        Err(_) => {
            return (false, Some("Warning: Invalid JSON in .claude/settings.json".to_string()));
        }
    };

    // Check for respectGitIgnore: false
    match json.get("respectGitIgnore") {
        Some(serde_json::Value::Bool(false)) => (true, None),
        Some(serde_json::Value::Bool(true)) => (
            false,
            Some(
                "Warning: .claude/settings.json has \"respectGitIgnore\": true\n\n\
                 Change to \"respectGitIgnore\": false to allow Claude Code to access\n\
                 gitignored AGPM resources."
                    .to_string(),
            ),
        ),
        None => (
            false,
            Some(
                "Warning: .claude/settings.json missing \"respectGitIgnore\": false\n\n\
                 Add this setting to allow Claude Code to access gitignored AGPM resources."
                    .to_string(),
            ),
        ),
        _ => (false, Some("Warning: Invalid value for respectGitIgnore in settings".to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_missing_gitignore_entries() {
        let temp = TempDir::new().unwrap();
        let gitignore = temp.path().join(".gitignore");
        std::fs::write(&gitignore, "# empty\n").unwrap();

        let lockfile = LockFile::default(); // Empty lockfile
        let result = check_gitignore_entries(temp.path(), &lockfile);

        // Should always check for private config
        assert!(result.contains(&"agpm.private.toml".to_string()));
        assert!(result.contains(&"agpm.private.lock".to_string()));
    }

    #[test]
    fn test_claude_settings_missing() {
        let temp = TempDir::new().unwrap();
        let (ok, warning) = check_claude_settings(temp.path());
        assert!(!ok);
        assert!(warning.is_some());
    }

    #[test]
    fn test_claude_settings_correct() {
        let temp = TempDir::new().unwrap();
        let claude_dir = temp.path().join(".claude");
        std::fs::create_dir_all(&claude_dir).unwrap();
        std::fs::write(claude_dir.join("settings.json"), r#"{"respectGitIgnore": false}"#).unwrap();

        let (ok, warning) = check_claude_settings(temp.path());
        assert!(ok);
        assert!(warning.is_none());
    }

    #[test]
    fn test_gitignore_entries_with_agents() {
        use crate::resolver::lockfile_builder::VariantInputs;
        use std::collections::BTreeMap;

        let temp = TempDir::new().unwrap();
        let gitignore = temp.path().join(".gitignore");
        std::fs::write(&gitignore, "# empty\n").unwrap();

        let mut lockfile = LockFile::default();
        lockfile.agents.push(crate::lockfile::LockedResource {
            name: "test".to_string(),
            source: None,
            url: None,
            version: None,
            path: "agents/test.md".to_string(),
            resolved_commit: None,
            checksum: "sha256:test".to_string(),
            context_checksum: None,
            installed_at: ".claude/agents/agpm/test.md".to_string(),
            dependencies: vec![],
            resource_type: ResourceType::Agent,
            tool: Some("claude-code".to_string()),
            manifest_alias: None,
            variant_inputs: VariantInputs::default(),
            applied_patches: BTreeMap::new(),
            install: None,
            is_private: false,
        });

        let result = check_gitignore_entries(temp.path(), &lockfile);

        // Should require agent gitignore entry
        assert!(result.contains(&".claude/agents/agpm/".to_string()));
    }

    #[test]
    fn test_gitignore_entries_satisfied() {
        use crate::resolver::lockfile_builder::VariantInputs;
        use std::collections::BTreeMap;

        let temp = TempDir::new().unwrap();
        let gitignore = temp.path().join(".gitignore");
        std::fs::write(&gitignore, ".claude/agents/agpm/\nagpm.private.toml\nagpm.private.lock\n")
            .unwrap();

        let mut lockfile = LockFile::default();
        lockfile.agents.push(crate::lockfile::LockedResource {
            name: "test".to_string(),
            source: None,
            url: None,
            version: None,
            path: "agents/test.md".to_string(),
            resolved_commit: None,
            checksum: "sha256:test".to_string(),
            context_checksum: None,
            installed_at: ".claude/agents/agpm/test.md".to_string(),
            dependencies: vec![],
            resource_type: ResourceType::Agent,
            tool: Some("claude-code".to_string()),
            manifest_alias: None,
            variant_inputs: VariantInputs::default(),
            applied_patches: BTreeMap::new(),
            install: None,
            is_private: false,
        });

        let result = check_gitignore_entries(temp.path(), &lockfile);

        // All required entries present
        assert!(result.is_empty());
    }
}
