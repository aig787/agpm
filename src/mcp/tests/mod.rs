use std::fs;

/// Test helper: Creates agpm.toml in temp directory so find_project_root works
pub(crate) fn setup_project_root(temp_path: &std::path::Path) {
    fs::write(temp_path.join("agpm.toml"), "[dependencies]\n").unwrap();
}

mod config_tests;
mod operations_tests;
mod serialization_tests;
mod settings_tests;
