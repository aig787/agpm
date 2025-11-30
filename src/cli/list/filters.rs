use crate::manifest::ResourceDependency;

use super::formatters::ListItem;

/// Determine if a resource type should be shown based on filters
pub fn should_show_resource_type(
    resource_type: crate::core::ResourceType,
    agents: bool,
    snippets: bool,
    commands: bool,
    skills: bool,
    type_filter: Option<&String>,
) -> bool {
    use crate::core::ResourceType;

    // Check if there's a type filter
    if let Some(t) = type_filter {
        let type_str = resource_type.to_string();
        return t == &type_str || t == &format!("{type_str}s");
    }

    // Check individual flags - if any specific flag is set, only show that type
    let any_specific_filter = agents || snippets || commands || skills;
    match resource_type {
        ResourceType::Agent => !any_specific_filter || agents,
        ResourceType::Snippet => !any_specific_filter || snippets,
        ResourceType::Command => !any_specific_filter || commands,
        ResourceType::Skill => !any_specific_filter || skills,
        ResourceType::Script => !any_specific_filter,
        ResourceType::Hook => !any_specific_filter,
        ResourceType::McpServer => !any_specific_filter,
    }
}

/// Check if an item matches all filters
pub fn matches_filters(
    name: &str,
    dep: Option<&ResourceDependency>,
    _resource_type: &str,
    source_filter: Option<&String>,
    search_filter: Option<&String>,
) -> bool {
    // Source filter
    if let Some(source_filter) = source_filter
        && let Some(dep) = dep
    {
        if let Some(source) = dep.get_source() {
            if source != source_filter {
                return false;
            }
        } else {
            return false; // No source but filter specified
        }
    }

    // Search filter
    if let Some(search) = search_filter
        && !name.contains(search)
    {
        return false;
    }

    true
}

/// Check if a lockfile entry matches all filters
pub fn matches_lockfile_filters(
    name: &str,
    entry: &crate::lockfile::LockedResource,
    _resource_type: &str,
    source_filter: Option<&String>,
    search_filter: Option<&String>,
) -> bool {
    // Source filter
    if let Some(source_filter) = source_filter {
        if let Some(ref source) = entry.source {
            if source != source_filter {
                return false;
            }
        } else {
            return false; // No source but filter specified
        }
    }

    // Search filter
    if let Some(search) = search_filter
        && !name.contains(search)
    {
        return false;
    }

    true
}

/// Sort items based on sort criteria
pub fn sort_items(items: &mut [ListItem], sort_field: Option<&String>) {
    if let Some(sort_field) = sort_field {
        match sort_field.as_str() {
            "name" => items.sort_by(|a, b| a.name.cmp(&b.name)),
            "version" => items.sort_by(|a, b| {
                a.version.as_deref().unwrap_or("").cmp(b.version.as_deref().unwrap_or(""))
            }),
            "source" => items.sort_by(|a, b| {
                a.source.as_deref().unwrap_or("local").cmp(b.source.as_deref().unwrap_or("local"))
            }),
            "type" => items.sort_by(|a, b| a.resource_type.cmp(&b.resource_type)),
            _ => {} // Already validated
        }
    }
}
