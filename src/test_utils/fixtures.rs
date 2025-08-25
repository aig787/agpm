//! Test fixtures for creating sample data structures
//!
//! This module provides builders for creating test data like manifests,
//! lockfiles, and markdown files.

use anyhow::{Context, Result};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

/// Test fixture for creating sample ccpm.toml files
#[derive(Clone, Debug)]
pub struct ManifestFixture {
    pub content: String,
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
    pub fn missing_fields() -> Self {
        Self {
            name: "missing_fields".to_string(),
            content: r#"
[sources]
official = "https://github.com/example-org/ccpm-official.git"

[agents]
incomplete-agent = { source = "official", path = "agents/test.md" }  # Missing version
"#
            .trim()
            .to_string(),
        }
    }

    /// Manifest with version conflicts
    pub fn version_conflicts() -> Self {
        Self {
            name: "version_conflicts".to_string(),
            content: r#"
[sources]
source1 = "https://github.com/example-org/ccpm-repo1.git"
source2 = "https://github.com/example-org/ccpm-repo2.git"

[agents]
agent1 = { source = "source1", path = "shared/lib.md", version = "v1.0.0" }
agent2 = { source = "source2", path = "shared/lib.md", version = "v2.0.0" }
"#
            .trim()
            .to_string(),
        }
    }

    /// Empty manifest (only comments)
    pub fn empty() -> Self {
        Self {
            name: "empty".to_string(),
            content: r#"
# Empty ccpm.toml file
# No sources or dependencies defined
"#
            .trim()
            .to_string(),
        }
    }

    /// Write the manifest to a directory
    pub fn write_to(&self, dir: &Path) -> Result<PathBuf> {
        let manifest_path = dir.join("ccpm.toml");
        fs::write(&manifest_path, &self.content)?;
        Ok(manifest_path)
    }
}

/// Test fixture for creating sample ccpm.lock files
#[derive(Clone, Debug)]
pub struct LockfileFixture {
    pub content: String,
    pub name: String,
}

impl LockfileFixture {
    /// Basic lockfile matching the basic manifest
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
"#
            .trim()
            .to_string(),
        }
    }

    /// Write the lockfile to a directory
    pub fn write_to(&self, dir: &Path) -> Result<PathBuf> {
        let lockfile_path = dir.join("ccpm.lock");
        fs::write(&lockfile_path, &self.content)?;
        Ok(lockfile_path)
    }
}

/// Test fixture for creating sample markdown files
#[derive(Clone, Debug)]
pub struct MarkdownFixture {
    pub path: String,
    pub content: String,
    pub frontmatter: Option<HashMap<String, String>>,
}

impl MarkdownFixture {
    /// Agent markdown file with frontmatter
    pub fn agent(name: &str) -> Self {
        let mut frontmatter = HashMap::new();
        frontmatter.insert("type".to_string(), "agent".to_string());
        frontmatter.insert("name".to_string(), name.to_string());
        frontmatter.insert("version".to_string(), "1.0.0".to_string());
        frontmatter.insert("description".to_string(), format!("Test agent: {}", name));

        Self {
            path: format!("agents/{}.md", name),
            content: format!(
                r#"---
type: agent
name: {}
version: 1.0.0
description: "Test agent: {}"
---

# {} Agent

This is a test agent that demonstrates the basic structure of a Claude Code agent.

## Usage

You can use this agent by importing it into your Claude Code project.

## Examples

```javascript
// Example usage of {} agent
const {} = require('{}');
```
"#,
                name, name, name, name, name, name
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

        Self {
            path: format!("snippets/{}.md", name),
            content: format!(
                r#"---
type: snippet
name: {}
version: 1.0.0
---

# {} Snippet

This is a test snippet.

## Code

```javascript
// {} snippet code
function {}() {{
    return "test";
}}
```
"#,
                name, name, name, name
            ),
            frontmatter: Some(frontmatter),
        }
    }

    /// Markdown file without frontmatter
    pub fn simple(name: &str, content: &str) -> Self {
        Self {
            path: format!("{}.md", name),
            content: content.to_string(),
            frontmatter: None,
        }
    }

    /// Write the markdown file to a directory
    pub fn write_to(&self, dir: &Path) -> Result<PathBuf> {
        let file_path = dir.join(&self.path);
        if let Some(parent) = file_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&file_path, &self.content)?;
        Ok(file_path)
    }
}

/// Git repository fixture for testing
#[derive(Clone, Debug)]
pub struct GitRepoFixture {
    pub path: PathBuf,
    pub files: Vec<MarkdownFixture>,
}

impl GitRepoFixture {
    /// Create a new git repository fixture
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            files: Vec::new(),
        }
    }

    /// Add a file to the repository
    pub fn with_file(mut self, file: MarkdownFixture) -> Self {
        self.files.push(file);
        self
    }

    /// Initialize the repository and add files
    pub fn init(&self) -> Result<()> {
        fs::create_dir_all(&self.path)?;

        // Initialize git repository
        std::process::Command::new("git")
            .arg("init")
            .current_dir(&self.path)
            .output()
            .context("Failed to initialize git repository")?;

        // Configure git user (required for commits)
        std::process::Command::new("git")
            .args(["config", "user.email", "test@example.com"])
            .current_dir(&self.path)
            .output()?;

        std::process::Command::new("git")
            .args(["config", "user.name", "Test User"])
            .current_dir(&self.path)
            .output()?;

        // Add files
        for file in &self.files {
            file.write_to(&self.path)?;
        }

        // Add and commit all files
        std::process::Command::new("git")
            .args(["add", "."])
            .current_dir(&self.path)
            .output()?;

        std::process::Command::new("git")
            .args(["commit", "-m", "Initial commit"])
            .current_dir(&self.path)
            .output()?;

        // Create a v1.0.0 tag
        std::process::Command::new("git")
            .args(["tag", "v1.0.0"])
            .current_dir(&self.path)
            .output()?;

        Ok(())
    }
}
