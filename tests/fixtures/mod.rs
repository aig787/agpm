use std::collections::HashMap;


/// Test fixture for creating sample ccpm.toml files
pub struct ManifestFixture {
    pub content: String,
    #[allow(dead_code)]
    pub name: String,
}

impl ManifestFixture {
    /// Basic manifest with simple dependencies
    pub fn basic() -> Self {
        Self {
            name: "basic".to_string(),
            content: r#"
[sources]
official = "https://github.com/example-org/ccpm-official.git"
community = "https://github.com/example-org/ccpm-community.git"

[agents]
my-agent = { source = "official", path = "agents/my-agent.md", version = "v1.0.0" }
helper = { source = "community", path = "agents/helper.md", version = "^1.2.0" }

[snippets]
utils = { source = "official", path = "snippets/utils.md", version = "v1.0.0" }
"#
            .trim()
            .to_string(),
        }
    }

    /// Manifest with local dependencies
    #[allow(dead_code)]
    pub fn with_local() -> Self {
        Self {
            name: "with_local".to_string(),
            content: r#"
[sources]
official = "https://github.com/example-org/ccpm-official.git"

[agents]
my-agent = { source = "official", path = "agents/my-agent.md", version = "v1.0.0" }
local-agent = { path = "../local-agents/helper.md" }

[snippets]
local-utils = { path = "./snippets/local-utils.md" }
"#
            .trim()
            .to_string(),
        }
    }

    /// Manifest with invalid syntax
    pub fn invalid_syntax() -> Self {
        Self {
            name: "invalid_syntax".to_string(),
            content: r#"
[sources
official = "https://github.com/example-org/ccpm-official.git"

[agents]
my-agent = { source = "official", path = "agents/my-agent.md", version = "v1.0.0"
"#
            .trim()
            .to_string(),
        }
    }

    /// Manifest with missing required fields
    #[allow(dead_code)]
    pub fn missing_fields() -> Self {
        Self {
            name: "missing_fields".to_string(),
            content: r#"
[sources]
official = "https://github.com/example-org/ccpm-official.git"

[agents]
incomplete-agent = { source = "official", path = "" }  # Missing path
"#
            .trim()
            .to_string(),
        }
    }

    /// Manifest with version conflicts
    #[allow(dead_code)]
    pub fn version_conflicts() -> Self {
        Self {
            name: "version_conflicts".to_string(),
            content: r#"
[sources]
source1 = "https://github.com/example-org/ccpm-repo1.git"
source2 = "https://github.com/example-org/ccpm-repo2.git"

[agents]
# Agents that would conflict if installed together
agent-from-source1 = { source = "source1", path = "shared.md", version = "v1.0.0" }
agent-from-source2 = { source = "source2", path = "shared.md", version = "v2.0.0" }
"#
            .trim()
            .to_string(),
        }
    }

}

/// Test fixture for creating sample lockfiles
pub struct LockfileFixture {
    pub content: String,
    #[allow(dead_code)]
    pub name: String,
}

impl LockfileFixture {
    /// Basic lockfile with resolved dependencies
    pub fn basic() -> Self {
        Self {
            name: "basic".to_string(),
            content: r#"
# Auto-generated lockfile - DO NOT EDIT
version = 1

[[sources]]
name = "official"
url = "https://github.com/example-org/ccpm-official.git"
commit = "abc123456789abcdef123456789abcdef12345678"
fetched_at = "2024-01-01T00:00:00Z"

[[sources]]
name = "community"
url = "https://github.com/example-org/ccpm-community.git"
commit = "def456789abcdef123456789abcdef123456789ab"
fetched_at = "2024-01-01T00:00:00Z"

[[agents]]
name = "my-agent"
source = "official"
path = "agents/my-agent.md"
version = "v1.0.0"
resolved_commit = "abc123456789abcdef123456789abcdef12345678"
checksum = "sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
installed_at = "agents/my-agent.md"

[[agents]]
name = "helper"
source = "community"
path = "agents/helper.md"
version = "v1.0.0"
resolved_commit = "def456789abcdef123456789abcdef123456789ab"
checksum = "sha256:38b060a751ac96384cd9327eb1b1e36a21fdb71114be07434c0cc7bf63f6e1da"
installed_at = "agents/helper.md"

[[snippets]]
name = "utils"
source = "official"
path = "snippets/utils.md"
version = "v1.0.0"
resolved_commit = "abc123456789abcdef123456789abcdef12345678"
checksum = "sha256:74e6f7298a9c2d168935f58c6b6c5b5ea4c3df6a0b6b8d2e7b2a2b8c3d4e5f6a"
installed_at = "snippets/utils.md"
"#
            .trim()
            .to_string(),
        }
    }

    /// Lockfile with out-of-date dependencies
    #[allow(dead_code)]
    pub fn outdated() -> Self {
        Self {
            name: "outdated".to_string(),
            content: r#"
# Auto-generated lockfile - DO NOT EDIT
version = 1

[[sources]]
name = "official"
url = "https://github.com/example-org/ccpm-official.git"
commit = "old123456789abcdef123456789abcdef123456789"
fetched_at = "2023-12-01T00:00:00Z"

[[agents]]
name = "my-agent"
source = "official"
path = "agents/my-agent.md"
version = "v0.9.0"
resolved_commit = "old123456789abcdef123456789abcdef123456789"
checksum = "sha256:old3b060a751ac96384cd9327eb1b1e36a21fdb71114be07434c0cc7bf63f6e1da"
installed_at = "agents/my-agent.md"
"#
            .trim()
            .to_string(),
        }
    }

}

/// Test fixture for creating sample markdown files
pub struct MarkdownFixture {
    pub path: String,
    pub content: String,
    #[allow(dead_code)]
    pub frontmatter: Option<HashMap<String, String>>,
}

impl MarkdownFixture {
    /// Agent markdown file with frontmatter
    pub fn agent(name: &str) -> Self {
        let mut frontmatter = HashMap::new();
        frontmatter.insert("type".to_string(), "agent".to_string());
        frontmatter.insert("name".to_string(), name.to_string());
        frontmatter.insert("version".to_string(), "1.0.0".to_string());
        frontmatter.insert("description".to_string(), format!("Test agent: {name}"));

        Self {
            path: format!("agents/{name}.md"),
            content: format!(
                r#"---
type: agent
name: {name}
version: 1.0.0
description: "Test agent: {name}"
---

# {name} Agent

This is a test agent that demonstrates the basic structure of a Claude Code agent.

## Usage

You can use this agent by importing it into your Claude Code project.

## Features

- Feature 1: Does something useful
- Feature 2: Provides helpful functionality
- Feature 3: Works with other agents

## Example

```
Example usage of the {name} agent.
```
"#
            ),
            frontmatter: Some(frontmatter),
        }
    }

    /// Snippet markdown file with frontmatter
    pub fn snippet(name: &str) -> Self {
        let mut frontmatter = HashMap::new();
        frontmatter.insert("type".to_string(), "snippet".to_string());
        frontmatter.insert("name".to_string(), name.to_string());
        frontmatter.insert("version".to_string(), "1.0.0".to_string());
        frontmatter.insert("language".to_string(), "python".to_string());

        Self {
            path: format!("snippets/{name}.md"),
            content: format!(
                r#"---
type: snippet
name: {name}
version: 1.0.0
language: python
---

# {name} Snippet

This is a test code snippet.

```python
def {name}():
    """Test function for {name} snippet."""
    return "Hello from {name} snippet!"

if __name__ == "__main__":
    print({name}())
```
"#
            ),
            frontmatter: Some(frontmatter),
        }
    }

    /// Markdown file without frontmatter
    #[allow(dead_code)]
    pub fn simple(name: &str, content: &str) -> Self {
        Self {
            path: format!("{name}.md"),
            content: content.to_string(),
            frontmatter: None,
        }
    }

}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_manifest_fixtures() {
        let basic = ManifestFixture::basic();
        assert!(basic.content.contains("[sources]"));
        assert!(basic.content.contains("[agents]"));

        let invalid = ManifestFixture::invalid_syntax();
        assert!(invalid.content.contains("[sources")); // Missing closing bracket
    }

    #[test]
    fn test_lockfile_fixtures() {
        let basic = LockfileFixture::basic();
        assert!(basic.content.contains("version = 1"));
        assert!(basic.content.contains("[[sources]]"));
        assert!(basic.content.contains("[[agents]]"));
    }

    #[test]
    fn test_markdown_fixtures() {
        let agent = MarkdownFixture::agent("test-agent");
        assert!(agent.content.contains("type: agent"));
        assert!(agent.content.contains("# test-agent Agent"));
        assert_eq!(agent.path, "agents/test-agent.md");

        let snippet = MarkdownFixture::snippet("test-snippet");
        assert!(snippet.content.contains("type: snippet"));
        assert!(snippet.content.contains("language: python"));
        assert_eq!(snippet.path, "snippets/test-snippet.md");
    }
}
