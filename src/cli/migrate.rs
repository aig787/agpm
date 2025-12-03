//! Migration command for upgrading AGPM installations.
//!
//! This module provides functionality to migrate from:
//! 1. Legacy CCPM (Claude Code Package Manager) naming to AGPM
//! 2. Old gitignore-managed format to new agpm/ subdirectory format
//! 3. Old tools configuration (without /agpm subdirectory paths) to new format
//!
//! All migrations can be performed with a single command, and the tool
//! automatically detects which migrations are needed.
//!
//! **Important**: Only AGPM-managed files (those tracked in the lockfile) are
//! migrated. User-created files in resource directories are not touched.

use anyhow::{Context, Result, bail};
use clap::Parser;
use colored::Colorize;
use std::path::{Path, PathBuf};

use crate::cli::install::InstallCommand;
use crate::lockfile::LockFile;

// Gitignore section markers for migration detection
const AGPM_MANAGED_ENTRIES: &str = "# AGPM managed entries";
const CCPM_MANAGED_ENTRIES: &str = "# CCPM managed entries";
const AGPM_MANAGED_END: &str = "# End of AGPM managed entries";
const CCPM_MANAGED_END: &str = "# End of CCPM managed entries";
const AGPM_MANAGED_PATHS: &str = "# AGPM managed paths";
const AGPM_MANAGED_PATHS_END: &str = "# End of AGPM managed paths";

// Old-style resource paths that need migration (without /agpm subdirectory)
const OLD_STYLE_PATHS: &[&str] = &[
    "\"agents\"",
    "\"commands\"",
    "\"snippets\"",
    "\"scripts\"",
    "\"skills\"",
    "\"agent\"", // OpenCode singular
    "\"command\"",
    "\"snippet\"",
];

/// Commented-out tools section for migration.
/// This replaces explicit [tools] sections so built-in defaults take over.
const COMMENTED_TOOLS_SECTION: &str = r#"# Tool type configurations (multi-tool support)
# Built-in defaults are applied automatically. Uncomment and modify to customize.
#
# [tools.claude-code]
# path = ".claude"
# resources = { agents = { path = "agents/agpm", flatten = true }, commands = { path = "commands/agpm", flatten = true }, hooks = { merge-target = ".claude/settings.local.json" }, mcp-servers = { merge-target = ".mcp.json" }, scripts = { path = "scripts/agpm", flatten = false }, skills = { path = "skills/agpm", flatten = false }, snippets = { path = "snippets/agpm", flatten = false } }
#
# [tools.opencode]
# enabled = false  # Enable if you want to use OpenCode resources
# path = ".opencode"
# resources = { agents = { path = "agent/agpm", flatten = true }, commands = { path = "command/agpm", flatten = true }, mcp-servers = { merge-target = ".opencode/opencode.json" }, snippets = { path = "snippet/agpm", flatten = false } }
#
# [tools.agpm]
# path = ".agpm"
# resources = { snippets = { path = "snippets", flatten = false } }
"#;

/// Detection result for old-format AGPM installations.
///
/// This struct captures evidence of legacy AGPM installations that need migration:
/// - Resource files at old paths (not in agpm/ subdirectory)
/// - AGPM/CCPM managed section in .gitignore
/// - Old-style tools configuration (without /agpm subdirectory paths)
#[derive(Debug, Default)]
pub struct OldFormatDetection {
    /// Resource files found at old paths (not in agpm/ subdirectory).
    pub old_resource_paths: Vec<PathBuf>,
    /// Whether .gitignore has AGPM/CCPM managed section.
    pub has_managed_gitignore_section: bool,
    /// Whether manifest has old-style tools configuration.
    pub has_old_tools_config: bool,
}

impl OldFormatDetection {
    /// Returns true if migration is needed.
    pub fn needs_migration(&self) -> bool {
        !self.old_resource_paths.is_empty()
            || self.has_managed_gitignore_section
            || self.has_old_tools_config
    }
}

/// Detect if manifest has old-style tools configuration.
///
/// Old-style tools configs have resource paths without the /agpm subdirectory,
/// e.g., `path = "agents"` instead of `path = "agents/agpm"`.
fn detect_old_tools_config(project_dir: &Path) -> bool {
    let manifest_path = project_dir.join("agpm.toml");
    if !manifest_path.exists() {
        return false;
    }

    let Ok(content) = std::fs::read_to_string(&manifest_path) else {
        return false;
    };

    // Check if there's a [tools] section with old-style paths
    if !content.contains("[tools") {
        return false;
    }

    // Look for old-style resource paths (without /agpm)
    // These are paths like `path = "agents"` that should be `path = "agents/agpm"`
    for old_path in OLD_STYLE_PATHS {
        // Match patterns like: path = "agents" (not path = "agents/agpm")
        let pattern = format!("path = {}", old_path);
        if content.contains(&pattern) {
            // Make sure it's not already migrated (contains /agpm)
            let migrated_pattern = format!("path = {}/agpm\"", &old_path[..old_path.len() - 1]);
            if !content.contains(&migrated_pattern) {
                return true;
            }
        }
    }

    false
}

/// Check if a path is in the new format or doesn't need migration.
///
/// Returns true for:
/// - Paths with `/agpm/` subdirectory (e.g., `.claude/agents/agpm/file.md`)
/// - Paths in `.agpm/` directory (AGPM's own resources)
/// - Merge targets (hooks, MCP configs) which don't need path migration
fn is_new_format_path(path: &str) -> bool {
    // New format: /agpm/ subdirectory after resource type
    if path.contains("/agpm/") {
        return true;
    }

    // AGPM's own directory (e.g., .agpm/snippets/...)
    if path.starts_with(".agpm/") {
        return true;
    }

    // Merge targets don't need migration - they're config files, not copied resources
    let merge_targets = [".claude/settings.local.json", ".mcp.json", ".opencode/opencode.json"];
    if merge_targets.contains(&path) {
        return true;
    }

    false
}

/// Detect if project has old-format AGPM installation.
///
/// Checks for:
/// - AGPM-managed resources at old paths (not in agpm/ subdirectory)
/// - AGPM/CCPM managed section markers in .gitignore
/// - Old-style tools configuration (without /agpm subdirectory paths)
///
/// **Important**: Only files tracked in the lockfile are considered for migration.
/// User-created files in resource directories are not touched. This includes
/// resources of any file type (not just .md files).
pub fn detect_old_format(project_dir: &Path) -> OldFormatDetection {
    let mut detection = OldFormatDetection::default();

    // Load lockfile to check tracked resources
    let lockfile_path = project_dir.join("agpm.lock");
    if let Ok(lockfile) = LockFile::load(&lockfile_path) {
        for resource in lockfile.all_resources() {
            // Check if installed_at is at old path (doesn't have /agpm/ subdirectory)
            if !is_new_format_path(&resource.installed_at) {
                let full_path = project_dir.join(&resource.installed_at);
                // Only include files that actually exist on disk
                if full_path.exists() {
                    detection.old_resource_paths.push(full_path);
                }
            }
        }
    }

    // Check for AGPM managed section in .gitignore
    let gitignore_path = project_dir.join(".gitignore");
    if gitignore_path.exists() {
        if let Ok(content) = std::fs::read_to_string(&gitignore_path) {
            if content.contains(AGPM_MANAGED_ENTRIES) || content.contains(CCPM_MANAGED_ENTRIES) {
                detection.has_managed_gitignore_section = true;
            }
        }
    }

    // Check for old-style tools configuration
    detection.has_old_tools_config = detect_old_tools_config(project_dir);

    detection
}

/// Migrate from old format to new agpm/ subdirectory format.
///
/// This function:
/// 1. Moves resources to new paths (inserting /agpm/ after resource type directory)
/// 2. Removes AGPM/CCPM managed section from .gitignore
/// 3. Updates lockfile paths
/// 4. Replaces old-style tools configuration with commented-out defaults
pub async fn run_format_migration(project_dir: &Path) -> Result<()> {
    let detection = detect_old_format(project_dir);

    if !detection.needs_migration() {
        println!("✅ {}", "No format migration needed - project already uses new format.".green());
        return Ok(());
    }

    println!("📦 {}", "Migrating AGPM installation to new format...".cyan());

    // 1. Move resources to new paths
    if !detection.old_resource_paths.is_empty() {
        println!(
            "\n  Moving {} resources to agpm/ subdirectories:",
            detection.old_resource_paths.len()
        );

        for old_path in &detection.old_resource_paths {
            // Determine new path (insert /agpm/ after resource type directory)
            if let Some(new_path) = compute_new_path(old_path, project_dir) {
                // Show relative paths for cleaner output
                let old_rel = old_path.strip_prefix(project_dir).unwrap_or(old_path);
                let new_rel = new_path.strip_prefix(project_dir).unwrap_or(&new_path);
                println!("    {} → {}", old_rel.display(), new_rel.display());

                // Create parent directory
                if let Some(parent) = new_path.parent() {
                    std::fs::create_dir_all(parent)?;
                }

                // Move file
                std::fs::rename(old_path, &new_path)?;
            }
        }
    }

    // 2. Replace AGPM managed section in .gitignore with new paths
    if detection.has_managed_gitignore_section {
        println!("\n  Updating .gitignore with new agpm/ subdirectory paths...");
        replace_managed_gitignore_section(project_dir)?;
    }

    // 3. Update lockfile paths
    println!("\n  Updating agpm.lock with new paths...");
    update_lockfile_paths(project_dir)?;

    // 4. Replace old-style tools configuration with commented-out defaults
    if detection.has_old_tools_config {
        println!("\n  Updating tools configuration to use built-in defaults...");
        replace_tools_section(project_dir)?;
    }

    // 5. Print completion message
    println!("\n✅ {}", "Format migration complete!".green().bold());
    println!(
        "\n{} If Claude Code can't find installed resources, run {} in Claude Code",
        "💡".cyan(),
        "/config".bright_white()
    );
    println!("   and set {} to {}.", "Respect .gitignore in file picker".yellow(), "false".green());

    Ok(())
}

fn compute_new_path(old_path: &Path, project_dir: &Path) -> Option<PathBuf> {
    // Get relative path from project dir
    let relative = old_path.strip_prefix(project_dir).ok()?;
    let components: Vec<_> = relative.components().collect();

    // Find the resource type directory and insert "agpm" after it
    for (i, component) in components.iter().enumerate() {
        if let std::path::Component::Normal(name) = component {
            let name_str = name.to_str()?;
            if matches!(
                name_str,
                "agents" | "commands" | "snippets" | "scripts" | "agent" | "command" | "snippet"
            ) {
                // Insert "agpm" after this component
                let mut new_path = project_dir.to_path_buf();
                for (j, c) in components.iter().enumerate() {
                    new_path.push(c);
                    if j == i {
                        new_path.push("agpm");
                    }
                }
                return Some(new_path);
            }
        }
    }

    None
}

/// Replace the old AGPM/CCPM managed section with new agpm/ subdirectory paths.
///
/// The new format uses simple directory patterns instead of individual file paths,
/// making it easier to manage and more resilient to changes.
fn replace_managed_gitignore_section(project_dir: &Path) -> Result<()> {
    let gitignore_path = project_dir.join(".gitignore");
    let content = std::fs::read_to_string(&gitignore_path)?;

    let mut new_lines = Vec::new();
    let mut in_managed_section = false;
    let mut replaced = false;

    for line in content.lines() {
        if line.contains(AGPM_MANAGED_ENTRIES) || line.contains(CCPM_MANAGED_ENTRIES) {
            in_managed_section = true;
            // Insert the new paths in place of the old section
            if !replaced {
                new_lines.push(AGPM_MANAGED_PATHS);
                new_lines.push(".claude/*/agpm/");
                new_lines.push(".opencode/*/agpm/");
                new_lines.push(".agpm/");
                new_lines.push("agpm.private.toml");
                new_lines.push("agpm.private.lock");
                new_lines.push(AGPM_MANAGED_PATHS_END);
                replaced = true;
            }
            continue;
        }
        if in_managed_section
            && (line.contains(AGPM_MANAGED_END) || line.contains(CCPM_MANAGED_END))
        {
            in_managed_section = false;
            continue;
        }
        if !in_managed_section {
            new_lines.push(line);
        }
    }

    // Remove trailing empty lines
    while new_lines.last().is_some_and(|l| l.is_empty()) {
        new_lines.pop();
    }

    std::fs::write(&gitignore_path, new_lines.join("\n") + "\n")?;
    Ok(())
}

fn update_lockfile_paths(project_dir: &Path) -> Result<()> {
    let lockfile_path = project_dir.join("agpm.lock");
    if !lockfile_path.exists() {
        return Ok(());
    }

    let content = std::fs::read_to_string(&lockfile_path)?;

    // Replace paths only if they don't already have /agpm/ subdirectory
    // Use a helper to avoid double-adding agpm/
    let final_content = migrate_installed_at_paths(&content);

    std::fs::write(&lockfile_path, final_content)?;
    Ok(())
}

/// Migrate installed_at paths to include agpm/ subdirectory.
///
/// Only updates paths that don't already have /agpm/ in the expected position.
fn migrate_installed_at_paths(content: &str) -> String {
    let path_patterns = [
        (".claude/agents/", ".claude/agents/agpm/"),
        (".claude/commands/", ".claude/commands/agpm/"),
        (".claude/snippets/", ".claude/snippets/agpm/"),
        (".claude/scripts/", ".claude/scripts/agpm/"),
        (".opencode/agent/", ".opencode/agent/agpm/"),
        (".opencode/command/", ".opencode/command/agpm/"),
        (".opencode/snippet/", ".opencode/snippet/agpm/"),
    ];

    let mut result = content.to_string();
    for (old_prefix, new_prefix) in path_patterns {
        // Only replace if the path doesn't already have /agpm/ after the resource type
        let old_pattern = format!("installed_at = \"{}", old_prefix);
        let new_pattern = format!("installed_at = \"{}", new_prefix);
        let already_migrated = format!("installed_at = \"{}agpm/", old_prefix);

        // Replace old paths, but skip if already migrated
        result = result
            .lines()
            .map(|line| {
                if line.contains(&old_pattern) && !line.contains(&already_migrated) {
                    line.replace(&old_pattern, &new_pattern)
                } else {
                    line.to_string()
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
    }
    result
}

/// Replace old-style tools section with commented-out defaults.
///
/// This removes the explicit `[tools]` section (with old paths like `"agents"`)
/// and replaces it with commented-out defaults. This allows the built-in
/// defaults (with correct `/agpm` paths) to take over while preserving
/// a reference to the default configuration.
fn replace_tools_section(project_dir: &Path) -> Result<()> {
    let manifest_path = project_dir.join("agpm.toml");
    if !manifest_path.exists() {
        return Ok(());
    }

    let content = std::fs::read_to_string(&manifest_path)?;

    // Find and remove the [tools] section and all its subsections
    let mut new_lines = Vec::new();
    let mut in_tools_section = false;
    let mut tools_section_replaced = false;
    let lines: Vec<&str> = content.lines().collect();

    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();

        // Detect start of [tools] section (including [tools.xxx] subsections)
        if trimmed == "[tools]" || trimmed.starts_with("[tools.") {
            if !in_tools_section {
                // First tools section encountered - insert commented defaults
                if !tools_section_replaced {
                    // Add a blank line before if previous line isn't empty
                    if !new_lines.is_empty() && !new_lines.last().unwrap_or(&"").is_empty() {
                        new_lines.push("");
                    }
                    // Add the commented-out tools section
                    for comment_line in COMMENTED_TOOLS_SECTION.lines() {
                        new_lines.push(comment_line);
                    }
                    tools_section_replaced = true;
                }
                in_tools_section = true;
            }
            continue;
        }

        // Check if we've reached a new top-level section (not tools-related)
        if trimmed.starts_with('[') && !trimmed.starts_with("[tools") {
            in_tools_section = false;
        }

        // Skip lines within tools sections
        if in_tools_section {
            // Check if the next line starts a new non-tools section
            // (this handles content between subsections)
            if i + 1 < lines.len() {
                let next_trimmed = lines[i + 1].trim();
                if next_trimmed.starts_with('[') && !next_trimmed.starts_with("[tools") {
                    in_tools_section = false;
                }
            }
            continue;
        }

        new_lines.push(line);
    }

    // Remove excess blank lines (more than 2 consecutive)
    let mut final_lines = Vec::new();
    let mut consecutive_blanks = 0;
    for line in new_lines {
        if line.is_empty() {
            consecutive_blanks += 1;
            if consecutive_blanks <= 2 {
                final_lines.push(line);
            }
        } else {
            consecutive_blanks = 0;
            final_lines.push(line);
        }
    }

    // Remove trailing empty lines
    while final_lines.last().is_some_and(|l| l.is_empty()) {
        final_lines.pop();
    }

    std::fs::write(&manifest_path, final_lines.join("\n") + "\n")?;
    Ok(())
}

/// Migrate AGPM installation to the latest format.
///
/// This command performs three types of migrations:
///
/// 1. **CCPM → AGPM naming**: Renames ccpm.toml and ccpm.lock to agpm.* equivalents
/// 2. **Format migration**: Moves resources from flat paths to agpm/ subdirectories
///    and removes the old gitignore managed section
/// 3. **Tools configuration**: Replaces old-style `[tools]` sections (with paths like
///    `"agents"`) with commented-out defaults, allowing built-in defaults with
///    correct `/agpm` paths to take over
///
/// # Examples
///
/// ```bash
/// # Migrate in current directory (all migrations)
/// agpm migrate
///
/// # Only migrate to new format (skip CCPM check)
/// agpm migrate --format-only
///
/// # Migrate with custom path
/// agpm migrate --path /path/to/project
///
/// # Dry run to see what would change
/// agpm migrate --dry-run
///
/// # Skip automatic installation (for testing)
/// agpm migrate --skip-install
/// ```
#[derive(Parser, Debug)]
#[command(name = "migrate")]
pub struct MigrateCommand {
    /// Path to the directory containing the project.
    ///
    /// Defaults to the current directory if not specified.
    #[arg(short, long)]
    path: Option<PathBuf>,

    /// Show what would be changed without actually modifying files.
    ///
    /// This is useful for previewing the migration before committing to it.
    #[arg(long)]
    dry_run: bool,

    /// Skip automatic installation after migration.
    ///
    /// By default, the migrate command automatically runs `agpm install` after
    /// migration to ensure all artifacts are in the correct locations.
    /// Use this flag to skip the installation step.
    #[arg(long)]
    skip_install: bool,

    /// Only perform format migration (skip CCPM → AGPM naming check).
    ///
    /// Use this when you only need to migrate from the old gitignore-managed
    /// format to the new agpm/ subdirectory format.
    #[arg(long)]
    format_only: bool,
}

impl MigrateCommand {
    /// Create a new migrate command with the given options.
    ///
    /// This is useful for programmatic invocation of the migrate command,
    /// such as from interactive migration prompts.
    ///
    /// # Arguments
    ///
    /// * `path` - Optional path to the directory containing legacy files
    /// * `dry_run` - Whether to perform a dry run without actually renaming
    /// * `skip_install` - Whether to skip automatic installation after migration
    ///
    /// # Returns
    ///
    /// A new `MigrateCommand` instance ready for execution
    #[must_use]
    pub fn new(path: Option<PathBuf>, dry_run: bool, skip_install: bool) -> Self {
        Self {
            path,
            dry_run,
            skip_install,
            format_only: false,
        }
    }

    /// Execute the migrate command.
    ///
    /// Performs both CCPM→AGPM naming migration and format migration
    /// (old gitignore-managed to new agpm/ subdirectory format).
    ///
    /// # Returns
    ///
    /// - `Ok(())` if migration succeeded or no migration was needed
    /// - `Err(anyhow::Error)` if migration failed
    pub async fn execute(self) -> Result<()> {
        let dir = self.path.as_deref().unwrap_or_else(|| Path::new("."));
        let dir = dir.canonicalize().context("Failed to resolve directory path")?;

        let mut any_migration_performed = false;

        // Phase 1: CCPM → AGPM naming migration (unless format_only)
        if !self.format_only {
            any_migration_performed |= self.run_ccpm_migration(&dir).await?;
        }

        // Phase 2: Format migration (old gitignore-managed to new agpm/ subdirectory)
        let format_detection = detect_old_format(&dir);
        if format_detection.needs_migration() {
            println!("\n🔍 Checking for old-format AGPM installation...");

            if !format_detection.old_resource_paths.is_empty() {
                println!(
                    "\n  Found {} resources at old paths:",
                    format_detection.old_resource_paths.len()
                );
                for path in &format_detection.old_resource_paths {
                    let rel = path.strip_prefix(&dir).unwrap_or(path);
                    println!("    • {}", rel.display());
                }
            }

            if format_detection.has_managed_gitignore_section {
                println!("\n  Found AGPM/CCPM managed section in .gitignore");
            }

            if format_detection.has_old_tools_config {
                println!("\n  Found old-style [tools] configuration (without /agpm paths)");
            }

            if self.dry_run {
                println!(
                    "\n{} (use without --dry-run to perform migration)",
                    "Format migration preview complete".yellow()
                );
            } else {
                run_format_migration(&dir).await?;
                any_migration_performed = true;
            }
        } else if !self.format_only {
            println!("\n✅ {}", "Project already uses new agpm/ subdirectory format.".green());
        }

        // Run installation to finalize artifact locations
        if any_migration_performed && !self.skip_install && !self.dry_run {
            println!("\n📦 {}", "Running installation to finalize artifact locations...".cyan());

            let install_cmd = InstallCommand::new();
            let manifest_path = dir.join("agpm.toml");
            match install_cmd.execute_from_path(Some(&manifest_path)).await {
                Ok(()) => {
                    println!("✅ {}", "Artifacts finalized in correct locations".green());
                }
                Err(e) => {
                    eprintln!("\n⚠️  {}", "Warning: Installation failed".yellow());
                    eprintln!("   {}", format!("Error: {}", e).yellow());
                    eprintln!("   {}", "You may need to run 'agpm install' manually".yellow());
                }
            }
        }

        if any_migration_performed && !self.dry_run {
            println!(
                "\n💡 Remember to:\n  • Review the changes\n  • Run {} to verify\n  • Commit the changes to version control",
                "agpm validate".cyan()
            );
        } else if !any_migration_performed {
            println!("\n✅ {}", "No migrations needed - project is up to date.".green());
        }

        Ok(())
    }

    /// Run CCPM → AGPM naming migration.
    ///
    /// Returns true if migration was performed.
    async fn run_ccpm_migration(&self, dir: &Path) -> Result<bool> {
        println!("🔍 Checking for legacy CCPM files in: {}", dir.display());

        let ccpm_toml = dir.join("ccpm.toml");
        let ccpm_lock = dir.join("ccpm.lock");
        let agpm_toml = dir.join("agpm.toml");
        let agpm_lock = dir.join("agpm.lock");

        let ccpm_toml_exists = ccpm_toml.exists();
        let ccpm_lock_exists = ccpm_lock.exists();
        let agpm_toml_exists = agpm_toml.exists();
        let agpm_lock_exists = agpm_lock.exists();

        // Check if there are any CCPM files to migrate
        if !ccpm_toml_exists && !ccpm_lock_exists {
            println!("✅ {}", "No legacy CCPM files found.".green());
            return Ok(false);
        }

        // Check for conflicts
        let mut conflicts = Vec::new();
        if ccpm_toml_exists && agpm_toml_exists {
            conflicts.push("agpm.toml already exists");
        }
        if ccpm_lock_exists && agpm_lock_exists {
            conflicts.push("agpm.lock already exists");
        }

        if !conflicts.is_empty() {
            bail!(
                "Migration conflict: {}. Please resolve conflicts manually.",
                conflicts.join(" and ")
            );
        }

        // Display what will be migrated
        println!("\n📦 CCPM files to migrate:");
        if ccpm_toml_exists {
            println!("  • ccpm.toml → agpm.toml");
        }
        if ccpm_lock_exists {
            println!("  • ccpm.lock → agpm.lock");
        }

        if self.dry_run {
            println!(
                "\n{} (use without --dry-run to perform migration)",
                "CCPM naming migration preview complete".yellow()
            );
            return Ok(false);
        }

        // Perform the migration
        if ccpm_toml_exists {
            std::fs::rename(&ccpm_toml, &agpm_toml)
                .context("Failed to rename ccpm.toml to agpm.toml")?;
            println!("✅ {}", "Renamed ccpm.toml → agpm.toml".green());
        }

        if ccpm_lock_exists {
            std::fs::rename(&ccpm_lock, &agpm_lock)
                .context("Failed to rename ccpm.lock to agpm.lock")?;
            println!("✅ {}", "Renamed ccpm.lock → agpm.lock".green());
        }

        println!("\n🎉 {}", "CCPM naming migration completed successfully!".green().bold());

        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_migrate_no_files() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let cmd = MigrateCommand {
            path: Some(temp_dir.path().to_path_buf()),
            dry_run: false,
            skip_install: true,
            format_only: false,
        };

        cmd.execute().await?;
        Ok(())
    }

    #[tokio::test]
    async fn test_migrate_both_files() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let ccpm_toml = temp_dir.path().join("ccpm.toml");
        let ccpm_lock = temp_dir.path().join("ccpm.lock");

        fs::write(&ccpm_toml, "[sources]\n")?;
        fs::write(&ccpm_lock, "# lockfile\n")?;

        let cmd = MigrateCommand {
            path: Some(temp_dir.path().to_path_buf()),
            dry_run: false,
            skip_install: true,
            format_only: false,
        };

        cmd.execute().await?;

        assert!(!ccpm_toml.exists());
        assert!(!ccpm_lock.exists());
        assert!(temp_dir.path().join("agpm.toml").exists());
        assert!(temp_dir.path().join("agpm.lock").exists());
        Ok(())
    }

    #[tokio::test]
    async fn test_migrate_dry_run() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let ccpm_toml = temp_dir.path().join("ccpm.toml");

        fs::write(&ccpm_toml, "[sources]\n")?;

        let cmd = MigrateCommand {
            path: Some(temp_dir.path().to_path_buf()),
            dry_run: true,
            skip_install: true,
            format_only: false,
        };

        cmd.execute().await?;

        // Files should not be renamed in dry run
        assert!(ccpm_toml.exists());
        assert!(!temp_dir.path().join("agpm.toml").exists());
        Ok(())
    }

    #[tokio::test]
    async fn test_migrate_conflict() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let ccpm_toml = temp_dir.path().join("ccpm.toml");
        let agpm_toml = temp_dir.path().join("agpm.toml");

        fs::write(&ccpm_toml, "[sources]\n")?;
        fs::write(&agpm_toml, "[sources]\n").unwrap();

        let cmd = MigrateCommand {
            path: Some(temp_dir.path().to_path_buf()),
            dry_run: false,
            skip_install: true,
            format_only: false,
        };

        let result = cmd.execute().await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("conflict"));
        Ok(())
    }

    #[tokio::test]
    async fn test_migrate_only_toml() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let ccpm_toml = temp_dir.path().join("ccpm.toml");

        fs::write(&ccpm_toml, "[sources]\n")?;

        let cmd = MigrateCommand {
            path: Some(temp_dir.path().to_path_buf()),
            dry_run: false,
            skip_install: true,
            format_only: false,
        };

        cmd.execute().await?;

        assert!(!ccpm_toml.exists());
        assert!(temp_dir.path().join("agpm.toml").exists());
        Ok(())
    }

    #[tokio::test]
    async fn test_migrate_only_lock() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let ccpm_lock = temp_dir.path().join("ccpm.lock");

        fs::write(&ccpm_lock, "# lockfile\n")?;

        let cmd = MigrateCommand {
            path: Some(temp_dir.path().to_path_buf()),
            dry_run: false,
            skip_install: true,
            format_only: false,
        };

        cmd.execute().await?;

        assert!(!ccpm_lock.exists());
        assert!(temp_dir.path().join("agpm.lock").exists());
        Ok(())
    }

    #[tokio::test]
    async fn test_migrate_with_automatic_installation() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let ccpm_toml = temp_dir.path().join("ccpm.toml");

        // Create a valid manifest with no dependencies (installation will succeed with nothing to install)
        fs::write(&ccpm_toml, "[sources]\n")?;

        let cmd = MigrateCommand {
            path: Some(temp_dir.path().to_path_buf()),
            dry_run: false,
            skip_install: false, // Enable automatic installation
            format_only: false,
        };

        let result = cmd.execute().await;
        assert!(result.is_ok(), "Migration with automatic installation should succeed");

        // Files should be renamed
        assert!(!ccpm_toml.exists());
        assert!(temp_dir.path().join("agpm.toml").exists());

        // Lockfile should be created by installation (even if empty)
        assert!(temp_dir.path().join("agpm.lock").exists());
        Ok(())
    }

    #[tokio::test]
    async fn test_migrate_handles_installation_failure() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let ccpm_toml = temp_dir.path().join("ccpm.toml");

        // Create an invalid manifest that will cause installation to fail
        // (missing source URL for a dependency)
        fs::write(
            &ccpm_toml,
            "[sources]\ntest = \"https://github.com/nonexistent/repo.git\"\n\n\
             [agents]\ntest-agent = { source = \"test\", path = \"agents/test.md\", version = \"v1.0.0\" }",
        )?;

        let cmd = MigrateCommand {
            path: Some(temp_dir.path().to_path_buf()),
            dry_run: false,
            skip_install: false, // Enable automatic installation
            format_only: false,
        };

        // Should succeed - migration doesn't fail even if installation fails
        let result = cmd.execute().await;
        assert!(result.is_ok(), "Migration should succeed even if installation fails");

        // Files should still be renamed despite installation failure
        assert!(!ccpm_toml.exists());
        assert!(temp_dir.path().join("agpm.toml").exists());
        Ok(())
    }

    #[tokio::test]
    async fn test_detect_old_format_no_resources() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let detection = detect_old_format(temp_dir.path());

        assert!(!detection.needs_migration());
        assert!(detection.old_resource_paths.is_empty());
        assert!(!detection.has_managed_gitignore_section);
        Ok(())
    }

    #[tokio::test]
    async fn test_detect_old_format_with_managed_gitignore() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let gitignore = temp_dir.path().join(".gitignore");
        fs::write(
            &gitignore,
            "# AGPM managed entries - do not edit\nsome-entry\n# End of AGPM managed entries\n",
        )?;

        let detection = detect_old_format(temp_dir.path());

        assert!(detection.needs_migration());
        assert!(detection.has_managed_gitignore_section);
        Ok(())
    }

    #[tokio::test]
    async fn test_detect_old_format_with_old_resources() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let agents_dir = temp_dir.path().join(".claude/agents");
        fs::create_dir_all(&agents_dir)?;
        fs::write(agents_dir.join("test.md"), "# Test Agent")?;

        // Create a lockfile that tracks the resource at the old path
        let lockfile = r#"version = 1

[[agents]]
name = "test"
source = "test"
path = "agents/test.md"
version = "v1.0.0"
resolved_commit = "abc123"
checksum = "sha256:abc"
context_checksum = "sha256:def"
installed_at = ".claude/agents/test.md"
dependencies = []
resource_type = "Agent"
tool = "claude-code"
"#;
        fs::write(temp_dir.path().join("agpm.lock"), lockfile)?;

        let detection = detect_old_format(temp_dir.path());

        assert!(detection.needs_migration());
        assert_eq!(detection.old_resource_paths.len(), 1);
        Ok(())
    }

    #[tokio::test]
    async fn test_detect_old_format_ignores_user_files() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let agents_dir = temp_dir.path().join(".claude/agents");
        fs::create_dir_all(&agents_dir)?;

        // Create a user file (not tracked in lockfile)
        fs::write(agents_dir.join("user-agent.md"), "# User Agent")?;

        // Create an empty lockfile (no resources tracked)
        fs::write(temp_dir.path().join("agpm.lock"), "version = 1\n")?;

        let detection = detect_old_format(temp_dir.path());

        // Should NOT detect user files for migration
        assert!(!detection.needs_migration());
        assert!(detection.old_resource_paths.is_empty());
        Ok(())
    }

    #[tokio::test]
    async fn test_detect_old_format_with_extensionless_files() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let agents_dir = temp_dir.path().join(".claude/agents");
        fs::create_dir_all(&agents_dir)?;

        // Create extensionless templated resource (no .md extension)
        fs::write(agents_dir.join("backend-engineer-rust"), "# Agent")?;

        // Create a lockfile that tracks the extensionless file at old path
        let lockfile = r#"version = 1

[[agents]]
name = "backend-engineer-rust"
source = "test"
path = "agents/backend-engineer-rust"
version = "v1.0.0"
resolved_commit = "abc123"
checksum = "sha256:abc"
context_checksum = "sha256:def"
installed_at = ".claude/agents/backend-engineer-rust"
dependencies = []
resource_type = "Agent"
tool = "claude-code"
"#;
        fs::write(temp_dir.path().join("agpm.lock"), lockfile)?;

        let detection = detect_old_format(temp_dir.path());

        // Should detect extensionless files for migration
        assert!(detection.needs_migration());
        assert_eq!(detection.old_resource_paths.len(), 1);
        Ok(())
    }

    #[tokio::test]
    async fn test_detect_old_format_skips_new_format_paths() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let agents_dir = temp_dir.path().join(".claude/agents/agpm");
        fs::create_dir_all(&agents_dir)?;

        // Create resource at NEW path (already migrated)
        fs::write(agents_dir.join("test.md"), "# Test Agent")?;

        // Create a lockfile with new-format path
        let lockfile = r#"version = 1

[[agents]]
name = "test"
source = "test"
path = "agents/test.md"
version = "v1.0.0"
resolved_commit = "abc123"
checksum = "sha256:abc"
context_checksum = "sha256:def"
installed_at = ".claude/agents/agpm/test.md"
dependencies = []
resource_type = "Agent"
tool = "claude-code"
"#;
        fs::write(temp_dir.path().join("agpm.lock"), lockfile)?;

        let detection = detect_old_format(temp_dir.path());

        // Should NOT detect files that are already at new paths
        assert!(!detection.needs_migration());
        assert!(detection.old_resource_paths.is_empty());
        Ok(())
    }

    #[tokio::test]
    async fn test_detect_old_format_skips_agpm_directory() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let snippets_dir = temp_dir.path().join(".agpm/snippets/claude-code/mcp-servers");
        fs::create_dir_all(&snippets_dir)?;

        // Create resource in .agpm/ directory (AGPM's own resources)
        fs::write(snippets_dir.join("context7.json"), "{}")?;

        // Create a lockfile with .agpm/ path
        let lockfile = r#"version = 1

[[snippets]]
name = "context7"
source = "test"
path = "snippets/context7.json"
version = "v1.0.0"
resolved_commit = "abc123"
checksum = "sha256:abc"
context_checksum = "sha256:def"
installed_at = ".agpm/snippets/claude-code/mcp-servers/context7.json"
dependencies = []
resource_type = "Snippet"
tool = "agpm"
"#;
        fs::write(temp_dir.path().join("agpm.lock"), lockfile)?;

        let detection = detect_old_format(temp_dir.path());

        // Should NOT detect files in .agpm/ directory
        assert!(!detection.needs_migration());
        assert!(detection.old_resource_paths.is_empty());
        Ok(())
    }

    #[tokio::test]
    async fn test_detect_old_format_skips_merge_targets() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let claude_dir = temp_dir.path().join(".claude");
        fs::create_dir_all(&claude_dir)?;

        // Create merge target file
        fs::write(claude_dir.join("settings.local.json"), "{}")?;

        // Create a lockfile with merge target path
        let lockfile = r#"version = 1

[[hooks]]
name = "test-hook"
source = "test"
path = "hooks/test.json"
version = "v1.0.0"
resolved_commit = "abc123"
checksum = "sha256:abc"
context_checksum = "sha256:def"
installed_at = ".claude/settings.local.json"
dependencies = []
resource_type = "Hook"
tool = "claude-code"
"#;
        fs::write(temp_dir.path().join("agpm.lock"), lockfile)?;

        let detection = detect_old_format(temp_dir.path());

        // Should NOT detect merge targets
        assert!(!detection.needs_migration());
        assert!(detection.old_resource_paths.is_empty());
        Ok(())
    }

    #[tokio::test]
    async fn test_format_migration_moves_resources() -> Result<()> {
        let temp_dir = TempDir::new()?;

        // Create old-format resource
        let agents_dir = temp_dir.path().join(".claude/agents");
        fs::create_dir_all(&agents_dir)?;
        fs::write(agents_dir.join("test.md"), "# Test Agent")?;

        // Create a lockfile that tracks the resource at the old path
        let lockfile = r#"version = 1

[[agents]]
name = "test"
source = "test"
path = "agents/test.md"
version = "v1.0.0"
resolved_commit = "abc123"
checksum = "sha256:abc"
context_checksum = "sha256:def"
installed_at = ".claude/agents/test.md"
dependencies = []
resource_type = "Agent"
tool = "claude-code"
"#;
        fs::write(temp_dir.path().join("agpm.lock"), lockfile)?;

        // Create managed gitignore section
        let gitignore = temp_dir.path().join(".gitignore");
        fs::write(
            &gitignore,
            "user-entry\n# AGPM managed entries - do not edit\nold-entry\n# End of AGPM managed entries\n",
        )?;

        // Run migration
        run_format_migration(temp_dir.path()).await?;

        // Check resource was moved
        assert!(!agents_dir.join("test.md").exists());
        assert!(agents_dir.join("agpm/test.md").exists());

        // Check gitignore was updated
        let new_gitignore = fs::read_to_string(&gitignore)?;
        assert!(new_gitignore.contains("user-entry"));
        assert!(!new_gitignore.contains("AGPM managed entries - do not edit"));
        // New paths should be added with proper markers
        assert!(new_gitignore.contains("# AGPM managed paths"));
        assert!(new_gitignore.contains(".claude/*/agpm/"));
        assert!(new_gitignore.contains("# End of AGPM managed paths"));
        Ok(())
    }

    #[tokio::test]
    async fn test_detect_old_tools_config_with_old_paths() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let manifest = r#"[sources]

[tools.claude-code]
path = ".claude"
resources = { agents = { path = "agents", flatten = true } }

[agents]
"#;
        fs::write(temp_dir.path().join("agpm.toml"), manifest)?;

        let detection = detect_old_format(temp_dir.path());
        assert!(detection.has_old_tools_config);
        assert!(detection.needs_migration());
        Ok(())
    }

    #[tokio::test]
    async fn test_detect_old_tools_config_with_new_paths() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let manifest = r#"[sources]

[tools.claude-code]
path = ".claude"
resources = { agents = { path = "agents/agpm", flatten = true } }

[agents]
"#;
        fs::write(temp_dir.path().join("agpm.toml"), manifest)?;

        let detection = detect_old_format(temp_dir.path());
        assert!(!detection.has_old_tools_config);
        Ok(())
    }

    #[tokio::test]
    async fn test_detect_old_tools_config_no_tools_section() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let manifest = r#"[sources]

[agents]
"#;
        fs::write(temp_dir.path().join("agpm.toml"), manifest)?;

        let detection = detect_old_format(temp_dir.path());
        assert!(!detection.has_old_tools_config);
        Ok(())
    }

    #[tokio::test]
    async fn test_replace_tools_section() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let manifest = r#"[sources]
community = "https://example.com/repo.git"

[tools]

[tools.claude-code]
path = ".claude"
resources = { agents = { path = "agents", flatten = true }, commands = { path = "commands", flatten = true } }

[tools.opencode]
enabled = false
path = ".opencode"
resources = { agents = { path = "agent", flatten = true } }

[tools.agpm]
path = ".agpm"
resources = { snippets = { path = "snippets", flatten = false } }

[agents]
my-agent = { source = "community", path = "agents/test.md" }
"#;
        fs::write(temp_dir.path().join("agpm.toml"), manifest)?;

        replace_tools_section(temp_dir.path())?;

        let new_content = fs::read_to_string(temp_dir.path().join("agpm.toml"))?;

        // Should contain commented-out tools section
        assert!(new_content.contains("# [tools.claude-code]"));
        assert!(new_content.contains("# path = \".claude\""));
        assert!(new_content.contains("agents/agpm")); // New path in comments

        // Should NOT contain the old explicit tools sections
        assert!(!new_content.contains("[tools.claude-code]\npath"));
        assert!(!new_content.contains("path = \"agents\""));

        // Should preserve other sections
        assert!(new_content.contains("[sources]"));
        assert!(new_content.contains("community"));
        assert!(new_content.contains("[agents]"));
        assert!(new_content.contains("my-agent"));

        Ok(())
    }

    #[tokio::test]
    async fn test_replace_tools_section_preserves_project() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let manifest = r#"[sources]

[project]
language = "rust"

[tools.claude-code]
path = ".claude"
resources = { agents = { path = "agents" } }

[agents]
"#;
        fs::write(temp_dir.path().join("agpm.toml"), manifest)?;

        replace_tools_section(temp_dir.path())?;

        let new_content = fs::read_to_string(temp_dir.path().join("agpm.toml"))?;

        // Should preserve project section
        assert!(new_content.contains("[project]"));
        assert!(new_content.contains("language = \"rust\""));

        // Should have commented tools section
        assert!(new_content.contains("# [tools.claude-code]"));

        Ok(())
    }

    #[tokio::test]
    async fn test_format_migration_includes_tools() -> Result<()> {
        let temp_dir = TempDir::new()?;

        // Create manifest with old-style tools config
        let manifest = r#"[sources]

[tools.claude-code]
path = ".claude"
resources = { agents = { path = "agents", flatten = true } }

[agents]
"#;
        fs::write(temp_dir.path().join("agpm.toml"), manifest)?;

        // Run full format migration
        run_format_migration(temp_dir.path()).await?;

        let new_content = fs::read_to_string(temp_dir.path().join("agpm.toml"))?;

        // Should have commented-out tools section
        assert!(new_content.contains("# [tools.claude-code]"));
        assert!(new_content.contains("# Built-in defaults are applied automatically"));

        Ok(())
    }
}
