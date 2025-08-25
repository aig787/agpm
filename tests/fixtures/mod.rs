use anyhow::{Context, Result};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

/// Convert a path to a file:// URL string, properly handling Windows paths
pub fn path_to_file_url(path: &Path) -> String {
    // Convert backslashes to forward slashes for Windows paths in URLs
    let path_str = path.display().to_string().replace('\\', "/");
    format!("file://{}", path_str)
}

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
incomplete-agent = { source = "official", path = "agents/test.md" }  # Missing version
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

    /// Write the manifest to a directory
    pub fn write_to(&self, dir: &Path) -> Result<PathBuf> {
        let manifest_path = dir.join("ccpm.toml");
        fs::write(&manifest_path, &self.content)?;
        Ok(manifest_path)
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

    /// Write the lockfile to a directory
    #[allow(dead_code)]
    pub fn write_to(&self, dir: &Path) -> Result<PathBuf> {
        let lockfile_path = dir.join("ccpm.lock");
        fs::write(&lockfile_path, &self.content)?;
        Ok(lockfile_path)
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

## Features

- Feature 1: Does something useful
- Feature 2: Provides helpful functionality
- Feature 3: Works with other agents

## Example

```
Example usage of the {} agent.
```
"#,
                name, name, name, name
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
            path: format!("snippets/{}.md", name),
            content: format!(
                r#"---
type: snippet
name: {}
version: 1.0.0
language: python
---

# {} Snippet

This is a test code snippet.

```python
def {}():
    """Test function for {} snippet."""
    return "Hello from {} snippet!"

if __name__ == "__main__":
    print({}())
```
"#,
                name, name, name, name, name, name
            ),
            frontmatter: Some(frontmatter),
        }
    }

    /// Markdown file without frontmatter
    #[allow(dead_code)]
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

/// Test environment helper that sets up a complete test project
pub struct TestEnvironment {
    #[allow(dead_code)]
    pub temp_dir: TempDir,
    pub project_dir: PathBuf,
    pub sources_dir: PathBuf,
    #[allow(dead_code)]
    pub cache_dir: PathBuf,
}

impl TestEnvironment {
    /// Create a new test environment
    pub fn new() -> Result<Self> {
        let temp_dir = TempDir::new()?;
        let project_dir = temp_dir.path().join("project");
        let sources_dir = temp_dir.path().join("sources");
        let cache_dir = temp_dir.path().join("cache");

        fs::create_dir_all(&project_dir)?;
        fs::create_dir_all(&sources_dir)?;
        fs::create_dir_all(&cache_dir)?;

        Ok(Self {
            temp_dir,
            project_dir,
            sources_dir,
            cache_dir,
        })
    }

    /// Create a test environment with a basic manifest
    pub fn with_basic_manifest() -> Result<Self> {
        let env = Self::new()?;
        ManifestFixture::basic().write_to(&env.project_dir)?;
        Ok(env)
    }

    /// Create a test environment with a basic manifest that uses file:// URLs for testing
    #[allow(dead_code)]
    pub fn with_basic_manifest_file_urls() -> Result<Self> {
        let env = Self::new()?;

        // Create a modified manifest that uses file:// URLs
        let official_url = path_to_file_url(&env.sources_dir.join("official"));
        let community_url = path_to_file_url(&env.sources_dir.join("community"));

        let manifest_content = format!(
            r#"
[sources]
official = "{}"
community = "{}"

[agents]
my-agent = {{ source = "official", path = "agents/my-agent.md", version = "v1.0.0" }}
helper = {{ source = "community", path = "agents/helper.md", version = "v1.0.0" }}

[snippets]
utils = {{ source = "official", path = "snippets/utils.md", version = "v1.0.0" }}
"#,
            official_url, community_url
        );

        fs::write(env.project_dir.join("ccpm.toml"), manifest_content.trim())?;
        Ok(env)
    }

    /// Create a test environment with manifest and lockfile
    #[allow(dead_code)]
    pub fn with_manifest_and_lockfile() -> Result<Self> {
        let env = Self::new()?;
        ManifestFixture::basic().write_to(&env.project_dir)?;
        LockfileFixture::basic().write_to(&env.project_dir)?;
        Ok(env)
    }

    /// Create a test environment with manifest and lockfile using file:// URLs
    #[allow(dead_code)]
    pub fn with_manifest_and_lockfile_file_urls() -> Result<Self> {
        let env = Self::new()?;

        // Create a modified manifest that uses file:// URLs
        let official_url = path_to_file_url(&env.sources_dir.join("official"));
        let community_url = path_to_file_url(&env.sources_dir.join("community"));

        let manifest_content = format!(
            r#"
[sources]
official = "{}"
community = "{}"

[agents]
my-agent = {{ source = "official", path = "agents/my-agent.md", version = "v1.0.0" }}
helper = {{ source = "community", path = "agents/helper.md", version = "v1.0.0" }}

[snippets]
utils = {{ source = "official", path = "snippets/utils.md", version = "v1.0.0" }}
"#,
            official_url, community_url
        );

        fs::write(env.project_dir.join("ccpm.toml"), manifest_content.trim())?;

        // Create a matching lockfile that uses file:// URLs
        let lockfile_content = format!(
            r#"
# Auto-generated lockfile - DO NOT EDIT
version = 1

[[sources]]
name = "official"
url = "{}"
commit = "abc123456789abcdef123456789abcdef12345678"
fetched_at = "2024-01-01T00:00:00Z"

[[sources]]
name = "community"
url = "{}"
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
"#,
            official_url, community_url
        );

        fs::write(env.project_dir.join("ccpm.lock"), lockfile_content.trim())?;
        Ok(env)
    }

    /// Add a mock git repository to the sources directory
    pub fn add_mock_source(
        &self,
        name: &str,
        _url: &str,
        files: Vec<MarkdownFixture>,
    ) -> Result<PathBuf> {
        let source_dir = self.sources_dir.join(name);
        fs::create_dir_all(&source_dir)?;

        // Initialize as a real git repository
        std::process::Command::new("git")
            .arg("init")
            .current_dir(&source_dir)
            .output()
            .context("Failed to initialize git repository")?;

        // Configure git user for commits (required for git)
        std::process::Command::new("git")
            .args(["config", "user.email", "test@example.com"])
            .current_dir(&source_dir)
            .output()?;

        std::process::Command::new("git")
            .args(["config", "user.name", "Test User"])
            .current_dir(&source_dir)
            .output()?;

        // Write all markdown files
        for file in files {
            file.write_to(&source_dir)?;
        }

        // Add and commit all files
        std::process::Command::new("git")
            .args(["add", "."])
            .current_dir(&source_dir)
            .output()?;

        std::process::Command::new("git")
            .args(["commit", "-m", "Initial commit"])
            .current_dir(&source_dir)
            .output()?;

        // Create a tag for v1.0.0 (commonly used in tests)
        std::process::Command::new("git")
            .args(["tag", "v1.0.0"])
            .current_dir(&source_dir)
            .output()?;

        Ok(source_dir)
    }

    /// Get the file:// URL for a mock source
    #[allow(dead_code)]
    pub fn get_mock_source_url(&self, name: &str) -> String {
        format!("file://{}/{}", self.sources_dir.display(), name)
    }

    /// Create a manifest with only local dependencies (no external sources)
    /// This is useful for tests that don't need network access or git operations
    #[allow(dead_code)]
    pub fn create_local_only_manifest(&self) -> Result<()> {
        let manifest_content = r#"
[agents]
local-agent = { path = "./agents/local.md" }

[snippets]
local-snippet = { path = "./snippets/local.md" }
"#;
        fs::write(self.project_dir.join("ccpm.toml"), manifest_content)?;

        // Create the local files
        let agents_dir = self.project_dir.join("agents");
        let snippets_dir = self.project_dir.join("snippets");
        fs::create_dir_all(&agents_dir)?;
        fs::create_dir_all(&snippets_dir)?;
        fs::write(agents_dir.join("local.md"), "# Local Agent")?;
        fs::write(snippets_dir.join("local.md"), "# Local Snippet")?;

        Ok(())
    }

    /// Get the project directory path
    pub fn project_path(&self) -> &Path {
        &self.project_dir
    }

    /// Get the sources directory path  
    pub fn sources_path(&self) -> &Path {
        &self.sources_dir
    }

    #[allow(dead_code)]
    pub fn cache_path(&self) -> &Path {
        &self.cache_dir
    }

    /// Create a Command for ccpm with the test's isolated cache directory
    #[allow(dead_code)]
    pub fn ccpm_command(&self) -> assert_cmd::Command {
        let mut cmd = assert_cmd::Command::cargo_bin("ccpm").unwrap();
        cmd.current_dir(&self.project_dir)
            .env("CCPM_CACHE_DIR", &self.cache_dir);
        cmd
    }

    /// Get the temp directory path
    #[allow(dead_code)]
    pub fn temp_path(&self) -> &Path {
        self.temp_dir.path()
    }

    /// Create a manifest using HTTP URLs from a git server
    /// The urls parameter is a map of repo name to URL
    #[allow(dead_code)]
    pub fn create_manifest_for_server(&self, urls: HashMap<String, String>) -> Result<()> {
        // Create a basic manifest with the server URLs
        let mut content = String::from("[sources]\n");
        for (name, url) in &urls {
            content.push_str(&format!("{} = \"{}\"\n", name, url));
        }

        // Add some basic dependencies
        content.push_str("\n[agents]\n");
        if urls.contains_key("official") {
            content.push_str("my-agent = { source = \"official\", path = \"agents/my-agent.md\", version = \"v1.0.0\" }\n");
        }
        if urls.contains_key("community") {
            content.push_str("helper = { source = \"community\", path = \"agents/helper.md\", version = \"v1.0.0\" }\n");
        }

        content.push_str("\n[snippets]\n");
        if urls.contains_key("official") {
            content.push_str("utils = { source = \"official\", path = \"snippets/utils.md\", version = \"v1.0.0\" }\n");
        }

        fs::write(self.project_path().join("ccpm.toml"), content)?;
        Ok(())
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

    #[test]
    fn test_environment_setup() -> Result<()> {
        let env = TestEnvironment::new()?;
        assert!(env.project_path().exists());
        assert!(env.sources_path().exists());

        let env_with_manifest = TestEnvironment::with_basic_manifest()?;
        assert!(env_with_manifest.project_path().join("ccpm.toml").exists());

        Ok(())
    }

    #[test]
    fn test_mock_source_creation() -> Result<()> {
        let env = TestEnvironment::new()?;

        let files = vec![
            MarkdownFixture::agent("test-agent"),
            MarkdownFixture::snippet("test-snippet"),
        ];

        let source_dir = env.add_mock_source(
            "official",
            "https://github.com/example-org/ccpm-official.git",
            files,
        )?;

        assert!(source_dir.join(".git").exists());
        assert!(source_dir.join("agents/test-agent.md").exists());
        assert!(source_dir.join("snippets/test-snippet.md").exists());

        Ok(())
    }
}
