use anyhow::Result;
use std::fs;

/// Test helper: Creates agpm.toml in temp directory so find_project_root works
pub(crate) fn setup_project_root(temp_path: &std::path::Path) -> Result<()> {
    fs::write(temp_path.join("agpm.toml"), "[dependencies]\n")?;
    Ok(())
}

mod config_tests;
mod operations_tests;
mod serialization_tests;
mod settings_tests;
