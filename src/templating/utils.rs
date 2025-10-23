//! Utility functions for the templating system.

use serde_json::Value;

/// Perform a deep merge of two JSON values.
///
/// Recursively merges `overrides` into `base`. For objects, fields from `overrides`
/// are added or replace fields in `base`. For arrays and primitives, `overrides`
/// completely replaces `base`.
///
/// # Arguments
///
/// * `base` - The base JSON value
/// * `overrides` - The override values to merge into base
///
/// # Returns
///
/// Returns the merged JSON value.
///
/// # Examples
///
/// ```rust,no_run
/// use serde_json::json;
/// use agpm_cli::templating::deep_merge_json;
///
/// let base = json!({ "project": { "name": "agpm", "language": "rust" } });
/// let overrides = json!({ "project": { "language": "python", "framework": "fastapi" } });
///
/// let result = deep_merge_json(base, &overrides);
/// // result: { "project": { "name": "agpm", "language": "python", "framework": "fastapi" } }
/// ```
pub fn deep_merge_json(mut base: Value, overrides: &Value) -> Value {
    match (base.as_object_mut(), overrides.as_object()) {
        (Some(base_obj), Some(override_obj)) => {
            // Both are objects - recursively merge
            for (key, override_value) in override_obj {
                match base_obj.get_mut(key) {
                    Some(base_value) if base_value.is_object() && override_value.is_object() => {
                        // Recursively merge nested objects
                        let merged = deep_merge_json(base_value.clone(), override_value);
                        base_obj.insert(key.clone(), merged);
                    }
                    _ => {
                        // For non-objects or missing keys, override completely
                        base_obj.insert(key.clone(), override_value.clone());
                    }
                }
            }
            base
        }
        (_, _) => {
            // If override is not an object, or base is not an object, override replaces base
            overrides.clone()
        }
    }
}

/// Convert Unix-style path (from lockfile) to platform-native format for display in templates.
///
/// Lockfiles always use Unix-style forward slashes for cross-platform compatibility,
/// but when rendering templates, we want to show paths in the platform's native format
/// so users see `.claude\agents\helper.md` on Windows and `.claude/agents/helper.md` on Unix.
///
/// # Arguments
///
/// * `unix_path` - Path string with forward slashes (from lockfile)
///
/// # Returns
///
/// Platform-native path string (backslashes on Windows, forward slashes on Unix)
///
/// # Examples
///
/// ```
/// # use agpm_cli::templating::to_native_path_display;
/// #[cfg(windows)]
/// assert_eq!(to_native_path_display(".claude/agents/test.md"), ".claude\\agents\\test.md");
///
/// #[cfg(not(windows))]
/// assert_eq!(to_native_path_display(".claude/agents/test.md"), ".claude/agents/test.md");
/// ```
pub fn to_native_path_display(unix_path: &str) -> String {
    #[cfg(windows)]
    {
        unix_path.replace('/', "\\")
    }
    #[cfg(not(windows))]
    {
        unix_path.to_string()
    }
}
