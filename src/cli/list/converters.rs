use super::formatters::ListItem;

/// Convert a lockfile entry to a `ListItem`
pub fn lockentry_to_listitem(
    entry: &crate::lockfile::LockedResource,
    resource_type: &str,
) -> ListItem {
    ListItem {
        name: entry.name.clone(),
        source: entry.source.clone(),
        version: entry.version.clone(),
        path: Some(entry.path.clone()),
        resource_type: resource_type.to_string(),
        installed_at: Some(entry.installed_at.clone()),
        checksum: Some(entry.checksum.clone()),
        resolved_commit: entry.resolved_commit.clone(),
        tool: Some(entry.tool.clone().unwrap_or_else(|| "claude-code".to_string())),
        applied_patches: entry.applied_patches.clone(),
        approximate_token_count: entry.approximate_token_count,
    }
}
