//! Migration command for renaming legacy CCPM files to AGPM.
//!
//! This module provides functionality to migrate from the legacy CCPM (Claude Code Package Manager)
//! naming to the new AGPM naming. It detects and renames ccpm.toml and ccpm.lock files to their
//! agpm equivalents.

use anyhow::{Context, Result, bail};
use clap::Parser;
use colored::Colorize;
use std::path::{Path, PathBuf};

/// Migrate from legacy CCPM naming to AGPM.
///
/// This command detects ccpm.toml and ccpm.lock files in the current directory
/// and renames them to agpm.toml and agpm.lock respectively.
///
/// # Examples
///
/// ```bash
/// # Migrate in current directory
/// agpm migrate
///
/// # Migrate with custom path
/// agpm migrate --path /path/to/project
///
/// # Dry run to see what would be renamed
/// agpm migrate --dry-run
/// ```
#[derive(Parser, Debug)]
#[command(name = "migrate")]
pub struct MigrateCommand {
    /// Path to the directory containing ccpm.toml/ccpm.lock files.
    ///
    /// Defaults to the current directory if not specified.
    #[arg(short, long)]
    path: Option<PathBuf>,

    /// Show what would be renamed without actually renaming files.
    ///
    /// This is useful for previewing the migration before committing to it.
    #[arg(long)]
    dry_run: bool,
}

impl MigrateCommand {
    /// Execute the migrate command.
    ///
    /// # Returns
    ///
    /// - `Ok(())` if migration succeeded or no migration was needed
    /// - `Err(anyhow::Error)` if migration failed
    pub async fn execute(self) -> Result<()> {
        let dir = self.path.as_deref().unwrap_or_else(|| Path::new("."));
        let dir = dir
            .canonicalize()
            .context("Failed to resolve directory path")?;

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
            return Ok(());
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
        println!("\n📦 Files to migrate:");
        if ccpm_toml_exists {
            println!("  • ccpm.toml → agpm.toml");
        }
        if ccpm_lock_exists {
            println!("  • ccpm.lock → agpm.lock");
        }

        if self.dry_run {
            println!(
                "\n{} (use without --dry-run to perform migration)",
                "Dry run complete".yellow()
            );
            return Ok(());
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

        println!(
            "\n🎉 {}",
            "Migration completed successfully!".green().bold()
        );
        println!(
            "\n💡 Next steps:\n  • Review the renamed files\n  • Run {} to verify\n  • Commit the changes to version control",
            "agpm validate".cyan()
        );

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_migrate_no_files() {
        let temp_dir = TempDir::new().unwrap();
        let cmd = MigrateCommand {
            path: Some(temp_dir.path().to_path_buf()),
            dry_run: false,
        };

        let result = cmd.execute().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_migrate_both_files() {
        let temp_dir = TempDir::new().unwrap();
        let ccpm_toml = temp_dir.path().join("ccpm.toml");
        let ccpm_lock = temp_dir.path().join("ccpm.lock");

        fs::write(&ccpm_toml, "[sources]\n").unwrap();
        fs::write(&ccpm_lock, "# lockfile\n").unwrap();

        let cmd = MigrateCommand {
            path: Some(temp_dir.path().to_path_buf()),
            dry_run: false,
        };

        let result = cmd.execute().await;
        assert!(result.is_ok());

        assert!(!ccpm_toml.exists());
        assert!(!ccpm_lock.exists());
        assert!(temp_dir.path().join("agpm.toml").exists());
        assert!(temp_dir.path().join("agpm.lock").exists());
    }

    #[tokio::test]
    async fn test_migrate_dry_run() {
        let temp_dir = TempDir::new().unwrap();
        let ccpm_toml = temp_dir.path().join("ccpm.toml");

        fs::write(&ccpm_toml, "[sources]\n").unwrap();

        let cmd = MigrateCommand {
            path: Some(temp_dir.path().to_path_buf()),
            dry_run: true,
        };

        let result = cmd.execute().await;
        assert!(result.is_ok());

        // Files should not be renamed in dry run
        assert!(ccpm_toml.exists());
        assert!(!temp_dir.path().join("agpm.toml").exists());
    }

    #[tokio::test]
    async fn test_migrate_conflict() {
        let temp_dir = TempDir::new().unwrap();
        let ccpm_toml = temp_dir.path().join("ccpm.toml");
        let agpm_toml = temp_dir.path().join("agpm.toml");

        fs::write(&ccpm_toml, "[sources]\n").unwrap();
        fs::write(&agpm_toml, "[sources]\n").unwrap();

        let cmd = MigrateCommand {
            path: Some(temp_dir.path().to_path_buf()),
            dry_run: false,
        };

        let result = cmd.execute().await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("conflict"));
    }

    #[tokio::test]
    async fn test_migrate_only_toml() {
        let temp_dir = TempDir::new().unwrap();
        let ccpm_toml = temp_dir.path().join("ccpm.toml");

        fs::write(&ccpm_toml, "[sources]\n").unwrap();

        let cmd = MigrateCommand {
            path: Some(temp_dir.path().to_path_buf()),
            dry_run: false,
        };

        let result = cmd.execute().await;
        assert!(result.is_ok());

        assert!(!ccpm_toml.exists());
        assert!(temp_dir.path().join("agpm.toml").exists());
    }

    #[tokio::test]
    async fn test_migrate_only_lock() {
        let temp_dir = TempDir::new().unwrap();
        let ccpm_lock = temp_dir.path().join("ccpm.lock");

        fs::write(&ccpm_lock, "# lockfile\n").unwrap();

        let cmd = MigrateCommand {
            path: Some(temp_dir.path().to_path_buf()),
            dry_run: false,
        };

        let result = cmd.execute().await;
        assert!(result.is_ok());

        assert!(!ccpm_lock.exists());
        assert!(temp_dir.path().join("agpm.lock").exists());
    }
}
