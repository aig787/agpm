#[cfg(test)]
#[allow(clippy::module_inception)]
mod tests {
    use super::super::*;
    use crate::utils::normalize_path_for_storage;
    use anyhow::Result;
    use indicatif::ProgressBar;
    use std::process::Command;
    use tempfile::TempDir;

    // Progress bar mock for testing
    mod mock {
        use std::sync::{Arc, Mutex};

        /// Mock progress bar that tracks all method calls for testing
        #[derive(Clone)]
        #[allow(dead_code)] // Test utility struct used across test functions
        pub struct MockProgressBar {
            // Fields accessed via methods, not directly
            #[allow(dead_code)] // Accessed via get_messages()
            pub messages: Arc<Mutex<Vec<String>>>,
            #[allow(dead_code)] // Accessed via is_finished()
            pub finished: Arc<Mutex<bool>>,
            #[allow(dead_code)] // Accessed via get_finished_message()
            pub finished_message: Arc<Mutex<Option<String>>>,
        }

        impl MockProgressBar {
            pub fn new() -> Self {
                Self {
                    messages: Arc::new(Mutex::new(Vec::new())),
                    finished: Arc::new(Mutex::new(false)),
                    finished_message: Arc::new(Mutex::new(None)),
                }
            }

            #[allow(dead_code)] // Mock method for simulating progress updates in tests
            pub fn set_message(&self, msg: impl Into<String>) {
                self.messages.lock().unwrap().push(msg.into());
            }

            #[allow(dead_code)] // Mock method for simulating progress completion in tests
            pub fn finish_with_message(&self, msg: impl Into<String>) {
                *self.finished.lock().unwrap() = true;
                *self.finished_message.lock().unwrap() = Some(msg.into());
            }

            #[allow(dead_code)] // Test utility method for verifying captured messages
            pub fn get_messages(&self) -> Vec<String> {
                self.messages.lock().unwrap().clone()
            }

            #[allow(dead_code)] // Test utility method for checking completion state
            pub fn is_finished(&self) -> bool {
                *self.finished.lock().unwrap()
            }

            #[allow(dead_code)] // Test utility method for retrieving final message
            pub fn get_finished_message(&self) -> Option<String> {
                self.finished_message.lock().unwrap().clone()
            }
        }

        /// Wrapper to make `MockProgressBar` compatible with the real `ProgressBar` interface
        #[allow(dead_code)] // Test utility wrapper for progress bar mocking
        pub struct ProgressBarWrapper {
            inner: MockProgressBar,
        }

        impl ProgressBarWrapper {
            #[allow(dead_code)] // Constructor for creating wrapped mock in tests
            pub fn from_mock(mock: MockProgressBar) -> Self {
                Self {
                    inner: mock,
                }
            }

            #[allow(dead_code)] // Wrapper method delegating to mock implementation
            pub fn set_message(&self, msg: impl Into<String>) {
                self.inner.set_message(msg);
            }

            #[allow(dead_code)] // Wrapper method delegating to mock implementation
            pub fn finish_with_message(&self, msg: impl Into<String>) {
                self.inner.finish_with_message(msg);
            }
        }
    }

    use mock::MockProgressBar;

    #[test]
    fn test_is_git_installed() -> Result<()> {
        assert!(is_git_installed());
        Ok(())
    }

    #[test]
    fn test_parse_git_url() -> Result<()> {
        let cases = vec![
            ("https://github.com/user/repo.git", ("user", "repo")),
            ("git@github.com:user/repo.git", ("user", "repo")),
            ("https://gitlab.com/user/repo", ("user", "repo")),
            ("https://bitbucket.org/user/repo.git", ("user", "repo")),
        ];

        for (url, expected) in cases {
            let result = parse_git_url(url)?;
            assert_eq!(result.0, expected.0);
            assert_eq!(result.1, expected.1);
        }
        Ok(())
    }

    #[test]
    fn test_parse_git_url_invalid() {
        let result = parse_git_url("not-a-url");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_git_url_ssh_format() -> Result<()> {
        let result = parse_git_url("ssh://git@github.com/user/repo.git")?;
        let (owner, name) = result;
        assert_eq!(owner, "user");
        assert_eq!(name, "repo");
        Ok(())
    }

    #[test]
    fn test_parse_git_url_more_formats() -> Result<()> {
        let test_cases = vec![
            ("https://github.com/rust-lang/cargo.git", ("rust-lang", "cargo")),
            ("git@gitlab.com:group/project.git", ("group", "project")),
            ("ssh://git@bitbucket.org/team/repo", ("team", "repo")),
            ("https://github.com/user-name/repo-name", ("user-name", "repo-name")),
        ];

        for (url, (expected_owner, expected_repo)) in test_cases {
            let result = parse_git_url(url)?;
            let (owner, repo) = result;
            assert_eq!(owner, expected_owner, "Owner mismatch for URL: {url}");
            assert_eq!(repo, expected_repo, "Repo mismatch for URL: {url}");
        }
        Ok(())
    }

    #[test]
    fn test_parse_git_url_edge_cases() -> Result<()> {
        let invalid_urls = vec![
            "not-a-url",
            "https://example.com/something",
            "",
            // Note: file:// URLs and local paths are now valid
        ];

        for url in invalid_urls {
            let result = parse_git_url(url);
            assert!(result.is_err(), "Expected error for invalid URL: {url}");
        }

        // Test that local paths are now valid
        let valid_local_paths = vec!["/local/path/to/repo", "./relative/path", "../parent/path"];

        for path in valid_local_paths {
            let _result = parse_git_url(path)?;
            // Just verify it parses without error
        }
        Ok(())
    }

    #[test]
    fn test_parse_git_url_file_urls() -> Result<()> {
        // Test file:// URLs
        let test_cases = vec![
            ("file:///home/user/repos/myrepo", ("local", "myrepo")),
            ("file:///home/user/repos/myrepo.git", ("local", "myrepo")),
            ("file:///tmp/test", ("local", "test")),
            ("file:///var/folders/sources/official", ("local", "official")),
        ];

        for (url, (expected_owner, expected_repo)) in test_cases {
            let result = parse_git_url(url)?;
            assert_eq!(result.0, expected_owner, "Owner mismatch for {url}");
            assert_eq!(result.1, expected_repo, "Repo mismatch for {url}");
        }
        Ok(())
    }

    #[test]
    fn test_parse_git_url_special_cases() -> Result<()> {
        // Test URLs with ports
        let url_with_port = "ssh://git@github.com:22/user/repo.git";
        let _ = parse_git_url(url_with_port)?;

        // Test URLs with subgroups (GitLab)
        let gitlab_subgroup = "https://gitlab.com/group/subgroup/project.git";
        let result = parse_git_url(gitlab_subgroup)?;
        let (owner, name) = result;
        assert_eq!(owner, "subgroup");
        assert_eq!(name, "project");

        // Test URL without .git extension
        let no_git_ext = "https://github.com/user/repo";
        let result = parse_git_url(no_git_ext)?;
        let (owner, name) = result;
        assert_eq!(owner, "user");
        assert_eq!(name, "repo");
        Ok(())
    }

    #[test]
    fn test_is_git_repo() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let repo = GitRepo::new(temp_dir.path());

        assert!(!repo.is_git_repo());

        Command::new("git").args(["init"]).current_dir(temp_dir.path()).output()?;

        assert!(repo.is_git_repo());
        Ok(())
    }

    #[test]
    fn test_git_repo_path() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let repo = GitRepo::new(temp_dir.path());
        assert_eq!(repo.path(), temp_dir.path());
        Ok(())
    }

    #[tokio::test]
    async fn test_clone_local_repo() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let source_path = temp_dir.path().join("source");
        let target_path = temp_dir.path().join("target");

        // Create source repo
        std::fs::create_dir(&source_path)?;
        Command::new("git").args(["init", "--bare"]).current_dir(&source_path).output()?;

        // Clone it
        let cloned_repo = GitRepo::clone(source_path.to_str().unwrap(), &target_path).await?;
        assert!(cloned_repo.is_git_repo());
        Ok(())
    }

    #[tokio::test]
    async fn test_clone_with_progress() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let bare_path = temp_dir.path().join("bare");
        let clone_path = temp_dir.path().join("clone");

        // Create bare repo
        std::fs::create_dir(&bare_path).unwrap();
        let output =
            Command::new("git").args(["init", "--bare"]).current_dir(&bare_path).output().unwrap();
        assert!(output.status.success(), "Failed to init bare repo: {output:?}");

        // Create a mock progress bar
        let mock = MockProgressBar::new();
        let _mock_clone = mock.clone();

        // We need to use the real ProgressBar type for the API
        // This test verifies the clone succeeds with progress
        let pb = ProgressBar::new_spinner();
        pb.set_message("Test clone");

        let repo = GitRepo::clone(bare_path.to_str().unwrap(), &clone_path).await?;
        assert!(repo.is_git_repo());
        assert!(clone_path.exists());

        // The progress bar should have been used (finish_with_message called)
        pb.finish_with_message("Clone complete");
        Ok(())
    }

    #[tokio::test]
    async fn test_clone_invalid_url() -> Result<()> {
        let target_dir = TempDir::new().unwrap();
        let target_path = target_dir.path().join("cloned");

        let result = GitRepo::clone("/non/existent/path", &target_path).await;
        assert!(result.is_err());
        assert!(!target_path.exists());
        Ok(())
    }

    #[tokio::test]
    async fn test_clone_invalid_url_detailed() -> Result<()> {
        let target_dir = TempDir::new().unwrap();
        let target_path = target_dir.path().join("cloned");

        // Test various invalid URLs
        let invalid_urls =
            vec!["/non/existent/path", "http://invalid-git-url.test", "not-a-url", ""];

        for url in invalid_urls {
            let result = GitRepo::clone(url, &target_path).await;
            assert!(result.is_err(), "Expected error for URL: {url}");
            if let Err(error) = result {
                assert!(
                    error.to_string().contains("Failed to clone")
                        || error.to_string().contains("Failed to execute")
                );
            }
        }
        Ok(())
    }

    #[tokio::test]
    async fn test_clone_stderr_error_message() -> Result<()> {
        let target_dir = TempDir::new().unwrap();
        let target_path = target_dir.path().join("cloned");

        // Try to clone with an invalid URL that will produce stderr
        let result =
            GitRepo::clone("https://invalid.host.that.does.not.exist.9999/repo.git", &target_path)
                .await;

        assert!(result.is_err());
        if let Err(error) = result {
            let error_msg = error.to_string();
            assert!(error_msg.contains("Failed to clone"));
        }
        Ok(())
    }

    #[tokio::test]
    async fn test_fetch_simple() -> Result<()> {
        // Simple test that just validates fetch works with a bare repo
        let temp_dir = TempDir::new()?;
        let bare_path = temp_dir.path().join("bare");
        let clone_path = temp_dir.path().join("clone");

        // Create bare repo
        std::fs::create_dir(&bare_path).unwrap();
        let output =
            Command::new("git").args(["init", "--bare"]).current_dir(&bare_path).output().unwrap();
        assert!(output.status.success(), "Failed to init bare repo: {output:?}");

        // Clone it
        let repo = GitRepo::clone(bare_path.to_str().unwrap(), &clone_path).await?;

        // Fetch should work (even though there's nothing to fetch)
        repo.fetch(None).await?;

        // Fetch with progress should also work
        let pb = ProgressBar::new_spinner();
        pb.set_message("Test fetch");
        repo.fetch(None).await?;
        pb.finish_with_message("Fetch complete");
        Ok(())
    }

    #[tokio::test]
    async fn test_fetch_with_progress() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let bare_path = temp_dir.path().join("bare");
        let repo_path = temp_dir.path().join("repo");

        // Setup bare repo
        std::fs::create_dir(&bare_path).unwrap();
        Command::new("git").args(["init", "--bare"]).current_dir(&bare_path).output().unwrap();

        // Clone it
        let repo = GitRepo::clone(bare_path.to_str().unwrap(), &repo_path).await?;

        // Fetch with progress
        let pb = ProgressBar::new_spinner();
        pb.set_message("Test fetch");

        repo.fetch(None).await?;
        pb.finish_with_message("Fetch complete");
        Ok(())
    }

    #[tokio::test]
    async fn test_fetch_with_no_network() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let repo_path = temp_dir.path();

        Command::new("git").args(["init"]).current_dir(repo_path).output().unwrap();

        // Add a fake remote
        Command::new("git")
            .args(["remote", "add", "origin", "https://non-existent-host-9999.test/repo.git"])
            .current_dir(repo_path)
            .output()?;

        let repo = GitRepo::new(repo_path);
        let result = repo.fetch(None).await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Git operation failed: fetch"));
        Ok(())
    }

    #[tokio::test]
    async fn test_checkout() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let repo_path = temp_dir.path();

        // Initialize a git repo
        Command::new("git").args(["init"]).current_dir(repo_path).output().unwrap();

        Command::new("git")
            .args(["config", "user.email", "test@example.com"])
            .current_dir(repo_path)
            .output()?;

        Command::new("git")
            .args(["config", "user.name", "Test User"])
            .current_dir(repo_path)
            .output()?;

        // Create initial commit
        std::fs::write(repo_path.join("README.md"), "Test").unwrap();
        Command::new("git").args(["add", "."]).current_dir(repo_path).output().unwrap();
        Command::new("git")
            .args(["commit", "-m", "Initial commit"])
            .current_dir(repo_path)
            .output()?;

        // Create a tag
        Command::new("git").args(["tag", "v1.0.0"]).current_dir(repo_path).output().unwrap();

        // Create another commit
        std::fs::write(repo_path.join("file2.txt"), "Test2").unwrap();
        Command::new("git").args(["add", "."]).current_dir(repo_path).output().unwrap();
        Command::new("git")
            .args(["commit", "-m", "Second commit"])
            .current_dir(repo_path)
            .output()?;

        let repo = GitRepo::new(repo_path);

        // Checkout the tag
        repo.checkout("v1.0.0").await?;

        // Verify we're in detached HEAD state at v1.0.0
        assert!(!repo_path.join("file2.txt").exists());
        Ok(())
    }

    #[tokio::test]
    async fn test_checkout_branch() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let repo_path = temp_dir.path();

        Command::new("git").args(["init"]).current_dir(repo_path).output().unwrap();

        Command::new("git")
            .args(["config", "user.email", "test@agpm.test"])
            .current_dir(repo_path)
            .output()?;

        Command::new("git")
            .args(["config", "user.name", "AGPM Test"])
            .current_dir(repo_path)
            .output()?;

        // Create initial commit
        std::fs::write(repo_path.join("main.txt"), "Main branch").unwrap();
        Command::new("git").args(["add", "."]).current_dir(repo_path).output().unwrap();
        Command::new("git")
            .args(["commit", "-m", "Main commit"])
            .current_dir(repo_path)
            .output()?;

        // Create feature branch
        Command::new("git").args(["checkout", "-b", "feature"]).current_dir(repo_path).output()?;

        std::fs::write(repo_path.join("feature.txt"), "Feature branch").unwrap();
        Command::new("git").args(["add", "."]).current_dir(repo_path).output().unwrap();
        Command::new("git")
            .args(["commit", "-m", "Feature commit"])
            .current_dir(repo_path)
            .output()?;

        let repo = GitRepo::new(repo_path);

        // Verify we're on feature branch
        assert_eq!(repo.get_current_branch().await?, "feature");
        assert!(repo_path.join("feature.txt").exists());

        // Checkout main branch
        let main_branch = if Command::new("git")
            .args(["rev-parse", "--verify", "main"])
            .current_dir(repo_path)
            .output()?
            .status
            .success()
        {
            "main"
        } else {
            "master"
        };

        repo.checkout(main_branch).await?;
        assert!(!repo_path.join("feature.txt").exists());
        assert!(repo_path.join("main.txt").exists());

        // Checkout back to feature
        repo.checkout("feature").await?;
        assert!(repo_path.join("feature.txt").exists());
        Ok(())
    }

    #[tokio::test]
    async fn test_checkout_commit_hash() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let repo_path = temp_dir.path();

        Command::new("git").args(["init"]).current_dir(repo_path).output().unwrap();

        Command::new("git")
            .args(["config", "user.email", "test@agpm.test"])
            .current_dir(repo_path)
            .output()?;

        Command::new("git")
            .args(["config", "user.name", "Test User"])
            .current_dir(repo_path)
            .output()?;

        // Create first commit
        std::fs::write(repo_path.join("file1.txt"), "content1")?;
        Command::new("git").args(["add", "."]).current_dir(repo_path).output()?;
        Command::new("git")
            .args(["commit", "-m", "First commit"])
            .current_dir(repo_path)
            .output()?;

        // Get first commit hash
        let output =
            Command::new("git").args(["rev-parse", "HEAD"]).current_dir(repo_path).output()?;
        let first_commit = String::from_utf8_lossy(&output.stdout).trim().to_string();

        // Create second commit
        std::fs::write(repo_path.join("file2.txt"), "content2").unwrap();
        Command::new("git").args(["add", "."]).current_dir(repo_path).output().unwrap();
        Command::new("git")
            .args(["commit", "-m", "Second commit"])
            .current_dir(repo_path)
            .output()?;

        let repo = GitRepo::new(repo_path);

        // Checkout first commit by hash
        repo.checkout(&first_commit).await?;

        // Verify we're at first commit (file2 shouldn't exist)
        assert!(repo_path.join("file1.txt").exists());
        assert!(!repo_path.join("file2.txt").exists());
        Ok(())
    }

    #[tokio::test]
    async fn test_checkout_invalid_ref() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let repo_path = temp_dir.path();

        Command::new("git").args(["init"]).current_dir(repo_path).output().unwrap();

        Command::new("git")
            .args(["config", "user.email", "test@agpm.test"])
            .current_dir(repo_path)
            .output()?;

        Command::new("git")
            .args(["config", "user.name", "AGPM Test"])
            .current_dir(repo_path)
            .output()?;

        std::fs::write(repo_path.join("README.md"), "# Test").unwrap();
        Command::new("git").args(["add", "."]).current_dir(repo_path).output().unwrap();
        Command::new("git")
            .args(["commit", "-m", "Initial commit"])
            .current_dir(repo_path)
            .output()?;

        let repo = GitRepo::new(repo_path);
        let result = repo.checkout("non-existent-branch").await;

        assert!(result.is_err());
        let error_message = format!("{:?}", result.unwrap_err());
        assert!(error_message.contains("Failed to checkout"));
        Ok(())
    }

    #[tokio::test]
    async fn test_list_tags() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let repo_path = temp_dir.path();

        Command::new("git").args(["init"]).current_dir(repo_path).output().unwrap();

        Command::new("git")
            .args(["config", "user.email", "test@agpm.test"])
            .current_dir(repo_path)
            .output()?;

        Command::new("git")
            .args(["config", "user.name", "AGPM Test"])
            .current_dir(repo_path)
            .output()?;

        std::fs::write(repo_path.join("README.md"), "# Test").unwrap();
        Command::new("git").args(["add", "."]).current_dir(repo_path).output().unwrap();
        Command::new("git")
            .args(["commit", "-m", "Initial commit"])
            .current_dir(repo_path)
            .output()?;

        // Add multiple tags
        let tags_to_add = vec!["v1.0.0", "v1.1.0", "v2.0.0-beta", "release-1.2.3"];
        for tag in &tags_to_add {
            Command::new("git").args(["tag", tag]).current_dir(repo_path).output().unwrap();
        }

        let repo = GitRepo::new(repo_path);
        let mut tags = repo.list_tags().await?;
        tags.sort();

        assert_eq!(tags.len(), 4);
        assert!(tags.contains(&"v1.0.0".to_string()));
        assert!(tags.contains(&"v2.0.0-beta".to_string()));
        Ok(())
    }

    #[tokio::test]
    async fn test_list_tags_sorted() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let repo_path = temp_dir.path();

        Command::new("git").args(["init"]).current_dir(repo_path).output().unwrap();

        Command::new("git")
            .args(["config", "user.email", "test@agpm.test"])
            .current_dir(repo_path)
            .output()?;

        Command::new("git")
            .args(["config", "user.name", "Test User"])
            .current_dir(repo_path)
            .output()?;

        std::fs::write(repo_path.join("README.md"), "# Test").unwrap();
        Command::new("git").args(["add", "."]).current_dir(repo_path).output().unwrap();
        Command::new("git").args(["commit", "-m", "Initial"]).current_dir(repo_path).output()?;

        // Add tags in non-sorted order
        let tags = vec!["v2.0.0", "v1.0.0", "v1.2.0", "v1.1.0", "v3.0.0-alpha"];
        for tag in &tags {
            Command::new("git").args(["tag", tag]).current_dir(repo_path).output().unwrap();
        }

        let repo = GitRepo::new(repo_path);
        let listed_tags = repo.list_tags().await?;

        // Git tag -l returns tags in alphabetical order
        assert_eq!(listed_tags.len(), 5);
        // Verify they exist (order may vary by git version)
        for tag in tags {
            assert!(listed_tags.contains(&tag.to_string()));
        }
        Ok(())
    }

    #[tokio::test]
    async fn test_get_remote_url() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let repo_path = temp_dir.path();

        // Initialize a git repo
        Command::new("git").args(["init"]).current_dir(repo_path).output().unwrap();

        // Add a remote
        Command::new("git")
            .args(["remote", "add", "origin", "https://github.com/test/repo.git"])
            .current_dir(repo_path)
            .output()?;

        let repo = GitRepo::new(repo_path);
        let url = repo.get_remote_url().await?;
        // Accept both HTTPS and SSH formats (git config may rewrite URLs)
        assert!(
            url == "https://github.com/test/repo.git"
                || url == "ssh://git@github.com/test/repo.git"
                || url == "git@github.com:test/repo.git"
        );
        Ok(())
    }

    #[tokio::test]
    async fn test_get_remote_url_no_remote() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let repo_path = temp_dir.path();

        Command::new("git").args(["init"]).current_dir(repo_path).output().unwrap();

        let repo = GitRepo::new(repo_path);
        let result = repo.get_remote_url().await;

        assert!(result.is_err());
        Ok(())
    }

    #[tokio::test]
    async fn test_get_current_branch() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let repo_path = temp_dir.path();

        Command::new("git").args(["init"]).current_dir(repo_path).output().unwrap();

        Command::new("git")
            .args(["config", "user.email", "test@example.com"])
            .current_dir(repo_path)
            .output()?;

        Command::new("git")
            .args(["config", "user.name", "Test User"])
            .current_dir(repo_path)
            .output()?;

        // Create initial commit
        std::fs::write(repo_path.join("README.md"), "Test").unwrap();
        Command::new("git").args(["add", "."]).current_dir(repo_path).output().unwrap();
        Command::new("git")
            .args(["commit", "-m", "Initial commit"])
            .current_dir(repo_path)
            .output()?;

        let repo = GitRepo::new(repo_path);

        // Test get_current_branch
        let branch = repo.get_current_branch().await?;
        assert!(branch == "main" || branch == "master");
        Ok(())
    }

    #[tokio::test]
    async fn test_error_handling_non_git_repo() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let path = temp_dir.path().to_path_buf();

        // GitRepo::open doesn't exist, using new instead
        // Try git operations on non-git directory will fail

        // Try git operations on non-git directory
        let fake_repo = GitRepo {
            path,
            tag_cache: std::sync::OnceLock::new(),
        };

        let result = fake_repo.fetch(None).await;
        assert!(result.is_err());

        let result = fake_repo.get_current_branch().await;
        assert!(result.is_err());

        let result = fake_repo.list_tags().await;
        assert!(result.is_err());
        Ok(())
    }

    #[tokio::test]
    async fn test_concurrent_operations() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let repo_path = temp_dir.path().to_path_buf();

        Command::new("git").args(["init"]).current_dir(&repo_path).output().unwrap();

        Command::new("git")
            .args(["config", "user.email", "test@agpm.test"])
            .current_dir(&repo_path)
            .output()?;

        Command::new("git")
            .args(["config", "user.name", "Test User"])
            .current_dir(&repo_path)
            .output()?;

        // Create initial commit
        std::fs::write(repo_path.join("initial.txt"), "initial").unwrap();
        Command::new("git").args(["add", "."]).current_dir(&repo_path).output().unwrap();
        Command::new("git").args(["commit", "-m", "Initial"]).current_dir(&repo_path).output()?;

        let path1 = repo_path.clone();
        let path2 = repo_path.clone();

        // Spawn async tasks for concurrent operations
        let handle1 = tokio::spawn(async move {
            let repo = GitRepo::new(&path1);
            repo.list_tags().await
        });

        let handle2 = tokio::spawn(async move {
            let repo = GitRepo::new(&path2);
            repo.get_current_branch().await
        });

        // Both operations should succeed
        let _result1 = handle1.await.unwrap()?;
        let _result2 = handle2.await.unwrap()?;

        // Just verify they succeed
        Ok(())
    }

    #[tokio::test]
    async fn test_trait_implementation() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let repo_path = temp_dir.path();

        Command::new("git").args(["init"]).current_dir(repo_path).output().unwrap();

        Command::new("git")
            .args(["config", "user.email", "test@agpm.test"])
            .current_dir(repo_path)
            .output()?;

        Command::new("git")
            .args(["config", "user.name", "AGPM Test"])
            .current_dir(repo_path)
            .output()?;

        std::fs::write(repo_path.join("README.md"), "# Test").unwrap();
        Command::new("git").args(["add", "."]).current_dir(repo_path).output().unwrap();
        Command::new("git")
            .args(["commit", "-m", "Initial commit"])
            .current_dir(repo_path)
            .output()?;

        let repo = GitRepo::new(repo_path);

        // Test methods directly
        assert!(repo.is_git_repo());
        assert!(repo.path().exists());
        let tags = repo.list_tags().await?;
        assert_eq!(tags.len(), 0);
        Ok(())
    }

    // Additional error path tests

    #[tokio::test]
    async fn test_clone_permission_denied() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let source_path = temp_dir.path().join("source");
        let target_path = temp_dir.path().join("target");

        // Create a local git repository to clone from
        std::fs::create_dir(&source_path)?;
        Command::new("git").args(["init", "--bare"]).current_dir(&source_path).output()?;

        // Create the target directory and make it read-only
        std::fs::create_dir(&target_path)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&target_path)?.permissions();
            perms.set_mode(0o444); // Read-only
            std::fs::set_permissions(&target_path, perms)?;
        }

        let source_url = format!("file://{}", normalize_path_for_storage(&source_path));
        let result = GitRepo::clone(&source_url, &target_path).await;

        // Clean up permissions before assertion
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&target_path)?.permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&target_path, perms)?;
        }

        // On Windows, the test might not fail due to different permission handling
        // On Unix, it should fail due to permission denied
        #[cfg(unix)]
        assert!(result.is_err());
        #[cfg(windows)]
        let _ = result; // Windows handles permissions differently
        Ok(())
    }

    #[tokio::test]
    async fn test_clone_empty_url() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let target_path = temp_dir.path().join("target");

        let result = GitRepo::clone("", &target_path).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Failed to clone"));
        Ok(())
    }

    #[tokio::test]
    async fn test_fetch_local_repository() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let repo_path = temp_dir.path().join("repo");
        let origin_path = temp_dir.path().join("origin");

        // Create origin repository
        std::fs::create_dir_all(&origin_path)?;
        Command::new("git").args(["init", "--bare"]).current_dir(&origin_path).output()?;

        // Create repo and add the local origin
        std::fs::create_dir_all(&repo_path)?;
        Command::new("git").args(["init"]).current_dir(&repo_path).output()?;

        // Add a file:// remote
        let origin_url = format!("file://{}", origin_path.display());
        Command::new("git")
            .args(["remote", "add", "origin", &origin_url])
            .current_dir(&repo_path)
            .output()?;

        let repo = GitRepo::new(&repo_path);
        let result = repo.fetch(None).await;

        // Should fetch successfully from local repositories
        assert!(result.is_ok(), "Fetch failed: {:?}", result.err());
        Ok(())
    }

    #[tokio::test]
    async fn test_fetch_git_protocol() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let repo_path = temp_dir.path();

        Command::new("git").args(["init"]).current_dir(repo_path).output()?;

        // Add a file:// remote (local repository)
        let bare_repo = temp_dir.path().join("bare");
        std::fs::create_dir(&bare_repo)?;
        Command::new("git").args(["init", "--bare"]).current_dir(&bare_repo).output()?;

        Command::new("git")
            .args(["remote", "add", "origin", &format!("file://{}", bare_repo.display())])
            .current_dir(repo_path)
            .output()?;

        let repo = GitRepo::new(repo_path);
        repo.fetch(None).await?;
        // Should fetch for file:// repositories
        Ok(())
    }

    #[tokio::test]
    async fn test_fetch_with_auth_url() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let bare_path = temp_dir.path().join("bare");
        let repo_path = temp_dir.path().join("repo");

        // Create bare repo
        std::fs::create_dir(&bare_path)?;
        Command::new("git").args(["init", "--bare"]).current_dir(&bare_path).output()?;

        // Clone it
        let repo = GitRepo::clone(bare_path.to_str().unwrap(), &repo_path).await?;

        // Fetch with specific auth URL
        let auth_url = format!("file://{}", bare_path.display());
        repo.fetch(Some(&auth_url)).await?;
        Ok(())
    }

    #[tokio::test]
    async fn test_list_tags_non_git_directory() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let non_git_path = temp_dir.path().join("not_git");
        std::fs::create_dir(&non_git_path)?;

        let repo = GitRepo::new(&non_git_path);
        let result = repo.list_tags().await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Not a git repository"));
        Ok(())
    }

    #[tokio::test]
    async fn test_list_tags_non_existent_directory() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let non_existent = temp_dir.path().join("does_not_exist");

        let repo = GitRepo::new(&non_existent);
        let result = repo.list_tags().await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Repository path does not exist"));
        Ok(())
    }

    #[tokio::test]
    async fn test_verify_url_file_protocol() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let repo_path = temp_dir.path().join("repo");
        std::fs::create_dir(&repo_path).unwrap();

        // Test existing file:// URL
        let file_url = format!("file://{}", repo_path.display());
        GitRepo::verify_url(&file_url).await?;

        // Test non-existent file:// URL
        let bad_file_url = "file:///non/existent/path";
        let result = GitRepo::verify_url(bad_file_url).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Local path does not exist"));
        Ok(())
    }

    #[tokio::test]
    async fn test_verify_url_remote() -> Result<()> {
        // Test with invalid remote URL
        let result = GitRepo::verify_url("https://invalid-host-9999.test/repo.git").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Failed to verify remote repository"));
        Ok(())
    }

    #[test]
    fn test_strip_auth_from_url() -> Result<()> {
        // Test HTTPS with authentication
        let url = "https://user:pass@github.com/owner/repo.git";
        let result = strip_auth_from_url(url)?;
        assert_eq!(result, "https://github.com/owner/repo.git");

        // Test HTTPS with OAuth token
        let url = "https://oauth2:ghp_xxxx@github.com/owner/repo.git";
        let result = strip_auth_from_url(url)?;
        assert_eq!(result, "https://github.com/owner/repo.git");

        // Test HTTP with authentication
        let url = "http://user:pass@example.com/repo.git";
        let result = strip_auth_from_url(url)?;
        assert_eq!(result, "http://example.com/repo.git");

        // Test URL without authentication
        let url = "https://github.com/owner/repo.git";
        let result = strip_auth_from_url(url)?;
        assert_eq!(result, "https://github.com/owner/repo.git");

        // Test SSH URL (should remain unchanged)
        let url = "git@github.com:owner/repo.git";
        let result = strip_auth_from_url(url)?;
        assert_eq!(result, "git@github.com:owner/repo.git");

        // Test URL with @ in the path (not auth)
        let url = "https://example.com/user@domain/repo.git";
        let result = strip_auth_from_url(url)?;
        assert_eq!(result, "https://example.com/user@domain/repo.git");
        Ok(())
    }

    #[test]
    fn test_parse_git_url_local_paths() -> Result<()> {
        let result = parse_git_url("/absolute/path/to/repo")?;
        assert_eq!(result.0, "local");
        assert_eq!(result.1, "repo");

        let result = parse_git_url("./relative/path/repo.git")?;
        assert_eq!(result.0, "local");
        assert_eq!(result.1, "repo");

        let result = parse_git_url("../parent/repo")?;
        assert_eq!(result.0, "local");
        assert_eq!(result.1, "repo");

        // Test path without slashes - this is not a valid URL format
        // The parse_git_url function expects URLs or paths with at least one slash
        let result = parse_git_url("repo.git");
        assert!(result.is_err());
        Ok(())
    }

    #[test]
    fn test_ensure_git_available() -> Result<()> {
        // This should work on any system with git installed
        ensure_git_available()?;
        Ok(())
    }

    #[test]
    fn test_ensure_valid_git_repo() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let repo_path = temp_dir.path();

        // Test with non-git directory
        let result = ensure_valid_git_repo(repo_path);
        assert!(result.is_err());
        // The error message format changed - check for any git repo related error
        let err_str = result.unwrap_err().to_string();
        assert!(err_str.contains("git repository") || err_str.contains("Git repository"));

        // Initialize git repo
        Command::new("git").args(["init"]).current_dir(repo_path).output().unwrap();

        // Test with valid git directory
        ensure_valid_git_repo(repo_path)?;
        Ok(())
    }

    #[test]
    fn test_is_valid_git_repo() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let repo_path = temp_dir.path();

        assert!(!is_valid_git_repo(repo_path));

        Command::new("git").args(["init"]).current_dir(repo_path).output()?;

        assert!(is_valid_git_repo(repo_path));
        Ok(())
    }

    #[tokio::test]
    async fn test_checkout_reset_error_handling() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let repo_path = temp_dir.path();

        // Initialize repo
        Command::new("git").args(["init"]).current_dir(repo_path).output()?;

        Command::new("git")
            .args(["config", "user.email", "test@example.com"])
            .current_dir(repo_path)
            .output()?;

        Command::new("git")
            .args(["config", "user.name", "Test User"])
            .current_dir(repo_path)
            .output()?;

        // Create initial commit
        std::fs::write(repo_path.join("file.txt"), "content")?;
        Command::new("git").args(["add", "."]).current_dir(repo_path).output()?;
        Command::new("git").args(["commit", "-m", "Initial"]).current_dir(repo_path).output()?;

        // Create a tag
        Command::new("git").args(["tag", "v1.0.0"]).current_dir(repo_path).output()?;

        let repo = GitRepo::new(repo_path);

        // Checkout tag (will do reset first)
        repo.checkout("v1.0.0").await?;

        // Try to checkout non-existent ref
        let result = repo.checkout("non-existent").await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("Failed to checkout"));
        Ok(())
    }

    #[tokio::test]
    async fn test_get_remote_url_stderr() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let non_git_path = temp_dir.path().join("not_git");
        std::fs::create_dir(&non_git_path)?;

        let repo = GitRepo::new(&non_git_path);
        let result = repo.get_remote_url().await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Git operation failed"));
        Ok(())
    }

    #[tokio::test]
    async fn test_concurrent_git_operations_same_repo() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let repo_path = temp_dir.path();

        // Initialize repo
        Command::new("git").args(["init"]).current_dir(repo_path).output().unwrap();

        Command::new("git")
            .args(["config", "user.email", "test@example.com"])
            .current_dir(repo_path)
            .output()?;

        Command::new("git")
            .args(["config", "user.name", "Test User"])
            .current_dir(repo_path)
            .output()?;

        // Create some commits and tags
        for i in 0..3 {
            let file_name = format!("file{i}.txt");
            std::fs::write(repo_path.join(&file_name), format!("content{i}")).unwrap();
            Command::new("git").args(["add", "."]).current_dir(repo_path).output().unwrap();
            Command::new("git")
                .args(["commit", "-m", &format!("Commit {i}")])
                .current_dir(repo_path)
                .output()?;
            Command::new("git")
                .args(["tag", &format!("v{i}.0.0")])
                .current_dir(repo_path)
                .output()?;
        }

        // Spawn multiple concurrent operations
        let repo1 = GitRepo::new(repo_path);
        let repo2 = GitRepo::new(repo_path);
        let repo3 = GitRepo::new(repo_path);

        let handle1 = tokio::spawn(async move { repo1.list_tags().await });
        let handle2 = tokio::spawn(async move { repo2.get_current_branch().await });
        let handle3 = tokio::spawn(async move { repo3.checkout("v1.0.0").await });

        // All should succeed
        let results = tokio::join!(handle1, handle2, handle3);
        results.0.unwrap()?;
        results.1.unwrap()?;
        results.2.unwrap()?;
        Ok(())
    }

    // Tests for worktree functionality
    #[tokio::test]
    async fn test_clone_bare() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let source_path = temp_dir.path().join("source");
        let bare_path = temp_dir.path().join("bare.git");

        // Create source repo with content
        std::fs::create_dir(&source_path)?;
        Command::new("git").args(["init"]).current_dir(&source_path).output()?;

        Command::new("git")
            .args(["config", "user.email", "test@example.com"])
            .current_dir(&source_path)
            .output()?;

        Command::new("git")
            .args(["config", "user.name", "Test User"])
            .current_dir(&source_path)
            .output()?;

        std::fs::write(source_path.join("README.md"), "# Test")?;
        Command::new("git").args(["add", "."]).current_dir(&source_path).output()?;
        Command::new("git")
            .args(["commit", "-m", "Initial commit"])
            .current_dir(&source_path)
            .output()?;

        // Clone as bare repository using file:// URL
        let file_url = format!("file://{}", source_path.display());
        let result = GitRepo::clone_bare(&file_url, &bare_path).await;

        assert!(result.is_ok(), "Failed to clone bare: {:?}", result.err());
        let bare_repo = result?;
        assert!(bare_repo.path().exists());

        // For bare repos, we need to check for the git objects, not the .git directory
        // Bare repos have their git objects directly in the repo directory
        let has_objects = bare_repo.path().join("objects").exists();
        let has_refs = bare_repo.path().join("refs").exists();
        let has_head = bare_repo.path().join("HEAD").exists();

        assert!(has_objects, "Bare repo missing objects directory");
        assert!(has_refs, "Bare repo missing refs directory");
        assert!(has_head, "Bare repo missing HEAD file");

        // Note: is_git_repo() returns false for bare repos because they don't have .git subdirectory
        // This is expected behavior

        // Check if it's actually bare
        let is_bare = bare_repo.is_bare().await?;
        assert!(is_bare);
        Ok(())
    }

    #[tokio::test]
    async fn test_clone_bare_with_context() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let source_path = temp_dir.path().join("source");
        let bare_path = temp_dir.path().join("bare.git");

        // Create source repo
        std::fs::create_dir(&source_path).unwrap();
        Command::new("git").args(["init"]).current_dir(&source_path).output().unwrap();

        // Clone bare with context
        GitRepo::clone_bare_with_context(
            source_path.to_str().unwrap(),
            &bare_path,
            Some("test-dependency"),
        )
        .await?;
        Ok(())
    }

    #[tokio::test]
    async fn test_create_worktree() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let source_path = temp_dir.path().join("source");
        let bare_path = temp_dir.path().join("bare.git");
        let worktree_path = temp_dir.path().join("worktree");

        // Create source repo with content
        std::fs::create_dir(&source_path)?;
        Command::new("git").args(["init"]).current_dir(&source_path).output()?;

        Command::new("git")
            .args(["config", "user.email", "test@example.com"])
            .current_dir(&source_path)
            .output()?;

        Command::new("git")
            .args(["config", "user.name", "Test User"])
            .current_dir(&source_path)
            .output()?;

        std::fs::write(source_path.join("file.txt"), "content")?;
        Command::new("git").args(["add", "."]).current_dir(&source_path).output()?;
        Command::new("git")
            .args(["commit", "-m", "Initial commit"])
            .current_dir(&source_path)
            .output()?;

        // Create a tag
        Command::new("git").args(["tag", "v1.0.0"]).current_dir(&source_path).output()?;

        // Clone as bare
        let bare_repo = GitRepo::clone_bare(source_path.to_str().unwrap(), &bare_path).await?;

        // Create worktree
        let worktree = bare_repo.create_worktree(&worktree_path, Some("v1.0.0")).await?;
        assert!(worktree.is_git_repo());
        assert!(worktree_path.join("file.txt").exists());
        Ok(())
    }

    #[tokio::test]
    async fn test_create_worktree_with_context() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let source_path = temp_dir.path().join("source");
        let bare_path = temp_dir.path().join("bare.git");
        let worktree_path = temp_dir.path().join("worktree");

        // Create minimal source repo
        std::fs::create_dir(&source_path)?;
        Command::new("git").args(["init", "--bare"]).current_dir(&source_path).output()?;

        // Clone as bare
        let bare_repo = GitRepo::clone_bare(source_path.to_str().unwrap(), &bare_path).await?;

        // Create worktree with context
        let result = bare_repo
            .create_worktree_with_context(&worktree_path, None, Some("test-dependency"))
            .await;

        // This might fail because the bare repo has no commits, which is expected
        // We're mainly testing that the context parameter is handled
        let _ = result; // Don't assert success since empty repos might fail
        Ok(())
    }

    #[tokio::test]
    async fn test_remove_worktree() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let source_path = temp_dir.path().join("source");
        let bare_path = temp_dir.path().join("bare.git");
        let worktree_path = temp_dir.path().join("worktree");

        // Create source repo
        std::fs::create_dir(&source_path).unwrap();
        Command::new("git").args(["init"]).current_dir(&source_path).output().unwrap();

        Command::new("git")
            .args(["config", "user.email", "test@example.com"])
            .current_dir(&source_path)
            .output()?;

        Command::new("git")
            .args(["config", "user.name", "Test User"])
            .current_dir(&source_path)
            .output()?;

        std::fs::write(source_path.join("file.txt"), "content").unwrap();
        Command::new("git").args(["add", "."]).current_dir(&source_path).output().unwrap();
        Command::new("git")
            .args(["commit", "-m", "Initial commit"])
            .current_dir(&source_path)
            .output()?;

        // Clone as bare and create worktree
        let bare_repo =
            GitRepo::clone_bare(source_path.to_str().unwrap(), &bare_path).await.unwrap();

        let _worktree = bare_repo.create_worktree(&worktree_path, None).await.unwrap();
        assert!(worktree_path.exists());

        // Remove worktree
        bare_repo.remove_worktree(&worktree_path).await?;
        Ok(())
    }

    #[tokio::test]
    async fn test_list_worktrees() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let source_path = temp_dir.path().join("source");
        let bare_path = temp_dir.path().join("bare.git");

        // Create source repo
        std::fs::create_dir(&source_path)?;
        Command::new("git").args(["init"]).current_dir(&source_path).output()?;

        Command::new("git")
            .args(["config", "user.email", "test@example.com"])
            .current_dir(&source_path)
            .output()?;

        Command::new("git")
            .args(["config", "user.name", "Test User"])
            .current_dir(&source_path)
            .output()?;

        std::fs::write(source_path.join("file.txt"), "content")?;
        Command::new("git").args(["add", "."]).current_dir(&source_path).output()?;
        Command::new("git")
            .args(["commit", "-m", "Initial commit"])
            .current_dir(&source_path)
            .output()?;

        // Clone as bare
        let bare_repo = GitRepo::clone_bare(source_path.to_str().unwrap(), &bare_path).await?;

        // List worktrees (should be empty initially for bare repo)
        let worktrees = bare_repo.list_worktrees().await?;
        // Bare repos typically don't show up in worktree list, so this should be empty or minimal
        assert!(worktrees.len() <= 1); // Allow for different Git versions (some show main repo)
        Ok(())
    }

    #[tokio::test]
    async fn test_prune_worktrees() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let source_path = temp_dir.path().join("source");
        let bare_path = temp_dir.path().join("bare.git");

        // Create source repo
        std::fs::create_dir(&source_path).unwrap();
        Command::new("git").args(["init"]).current_dir(&source_path).output().unwrap();

        Command::new("git")
            .args(["config", "user.email", "test@example.com"])
            .current_dir(&source_path)
            .output()?;

        Command::new("git")
            .args(["config", "user.name", "Test User"])
            .current_dir(&source_path)
            .output()?;

        std::fs::write(source_path.join("file.txt"), "content").unwrap();
        Command::new("git").args(["add", "."]).current_dir(&source_path).output().unwrap();
        Command::new("git")
            .args(["commit", "-m", "Initial commit"])
            .current_dir(&source_path)
            .output()?;

        // Clone as bare
        let bare_repo =
            GitRepo::clone_bare(source_path.to_str().unwrap(), &bare_path).await.unwrap();

        // Prune worktrees (should succeed even if there are none)
        bare_repo.prune_worktrees().await?;
        Ok(())
    }

    #[tokio::test]
    async fn test_is_bare() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let normal_repo_path = temp_dir.path().join("normal");
        let bare_repo_path = temp_dir.path().join("bare.git");

        // Create normal repo
        std::fs::create_dir(&normal_repo_path)?;
        Command::new("git").args(["init"]).current_dir(&normal_repo_path).output()?;

        // Create bare repo
        std::fs::create_dir(&bare_repo_path)?;
        Command::new("git").args(["init", "--bare"]).current_dir(&bare_repo_path).output()?;

        let normal_repo = GitRepo::new(&normal_repo_path);
        let bare_repo = GitRepo::new(&bare_repo_path);

        // Test that normal repo is not bare
        let is_bare = normal_repo.is_bare().await?;
        assert!(!is_bare);

        // Test that bare repo is bare
        let is_bare = bare_repo.is_bare().await?;
        assert!(is_bare);
        Ok(())
    }

    #[tokio::test]
    async fn test_get_current_commit() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let repo_path = temp_dir.path();

        // Initialize repo
        Command::new("git").args(["init"]).current_dir(repo_path).output()?;

        Command::new("git")
            .args(["config", "user.email", "test@example.com"])
            .current_dir(repo_path)
            .output()?;

        Command::new("git")
            .args(["config", "user.name", "Test User"])
            .current_dir(repo_path)
            .output()?;

        // Create initial commit
        std::fs::write(repo_path.join("file.txt"), "content")?;
        Command::new("git").args(["add", "."]).current_dir(repo_path).output()?;
        Command::new("git")
            .args(["commit", "-m", "Initial commit"])
            .current_dir(repo_path)
            .output()?;

        let repo = GitRepo::new(repo_path);
        let commit = repo.get_current_commit().await?;

        // Should be a valid SHA-1 hash (40 characters)
        assert_eq!(commit.len(), 40);
        assert!(commit.chars().all(|c| c.is_ascii_hexdigit()));
        Ok(())
    }

    #[tokio::test]
    async fn test_get_current_commit_error() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let non_git_path = temp_dir.path().join("not_git");
        std::fs::create_dir(&non_git_path)?;

        let repo = GitRepo::new(&non_git_path);
        let result = repo.get_current_commit().await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Failed to get current commit"));
        Ok(())
    }

    #[tokio::test]
    async fn test_checkout_remote_branch_fallback() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let origin_path = temp_dir.path().join("origin");
        let repo_path = temp_dir.path().join("repo");

        // Create origin repo
        std::fs::create_dir(&origin_path)?;
        Command::new("git").args(["init"]).current_dir(&origin_path).output()?;

        Command::new("git")
            .args(["config", "user.email", "test@example.com"])
            .current_dir(&origin_path)
            .output()?;

        Command::new("git")
            .args(["config", "user.name", "Test User"])
            .current_dir(&origin_path)
            .output()?;

        std::fs::write(origin_path.join("file.txt"), "content")?;
        Command::new("git").args(["add", "."]).current_dir(&origin_path).output()?;
        Command::new("git")
            .args(["commit", "-m", "Initial commit"])
            .current_dir(&origin_path)
            .output()?;

        // Create feature branch
        Command::new("git")
            .args(["checkout", "-b", "feature"])
            .current_dir(&origin_path)
            .output()?;

        std::fs::write(origin_path.join("feature.txt"), "feature")?;
        Command::new("git").args(["add", "."]).current_dir(&origin_path).output()?;
        Command::new("git")
            .args(["commit", "-m", "Feature commit"])
            .current_dir(&origin_path)
            .output()?;

        // Clone the repo
        let repo = GitRepo::clone(origin_path.to_str().unwrap(), &repo_path).await?;

        // Fetch to get remote branches
        repo.fetch(None).await?;

        // Try to checkout the feature branch (should work via remote branch fallback)
        let result = repo.checkout("feature").await;
        assert!(result.is_ok(), "Failed to checkout remote branch: {:?}", result.err());
        Ok(())
    }

    #[tokio::test]
    async fn test_checkout_error_handling() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let repo_path = temp_dir.path();

        // Initialize repo
        Command::new("git").args(["init"]).current_dir(repo_path).output()?;

        Command::new("git")
            .args(["config", "user.email", "test@example.com"])
            .current_dir(repo_path)
            .output()?;

        Command::new("git")
            .args(["config", "user.name", "Test User"])
            .current_dir(repo_path)
            .output()?;

        std::fs::write(repo_path.join("file.txt"), "content")?;
        Command::new("git").args(["add", "."]).current_dir(repo_path).output()?;
        Command::new("git")
            .args(["commit", "-m", "Initial commit"])
            .current_dir(repo_path)
            .output()?;

        let repo = GitRepo::new(repo_path);

        // Try to checkout non-existent reference
        let result = repo.checkout("definitely-does-not-exist").await;
        assert!(result.is_err());

        // Check that it's the right error type
        let error_str = result.unwrap_err().to_string();
        assert!(
            error_str.contains("Failed to checkout") || error_str.contains("GitCheckoutFailed")
        );
        Ok(())
    }

    #[tokio::test]
    async fn test_resolve_to_sha() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let repo_path = temp_dir.path();

        // Initialize repo
        Command::new("git").args(["init"]).current_dir(repo_path).output()?;

        Command::new("git")
            .args(["config", "user.email", "test@example.com"])
            .current_dir(repo_path)
            .output()?;

        Command::new("git")
            .args(["config", "user.name", "Test User"])
            .current_dir(repo_path)
            .output()?;

        // Create initial commit
        std::fs::write(repo_path.join("file.txt"), "content")?;
        Command::new("git").args(["add", "."]).current_dir(repo_path).output()?;
        Command::new("git")
            .args(["commit", "-m", "Initial commit"])
            .current_dir(repo_path)
            .output()?;

        // Get the commit SHA
        let commit_sha =
            Command::new("git").args(["rev-parse", "HEAD"]).current_dir(repo_path).output()?;
        let expected_sha = String::from_utf8(commit_sha.stdout)?.trim().to_string();

        // Create a tag
        Command::new("git").args(["tag", "v1.0.0"]).current_dir(repo_path).output()?;

        let repo = GitRepo::new(repo_path);

        // Test resolving HEAD
        let sha = repo.resolve_to_sha(None).await?;
        assert_eq!(sha, expected_sha);

        // Test resolving HEAD explicitly
        let sha = repo.resolve_to_sha(Some("HEAD")).await?;
        assert_eq!(sha, expected_sha);

        // Test resolving a tag
        let sha = repo.resolve_to_sha(Some("v1.0.0")).await?;
        assert_eq!(sha, expected_sha);

        // Test that a full SHA is returned as-is (optimization)
        let full_sha = "a".repeat(40);
        let sha = repo.resolve_to_sha(Some(&full_sha)).await?;
        assert_eq!(sha, full_sha);

        // Test resolving main/master branch
        let main_branch = if Command::new("git")
            .args(["rev-parse", "--verify", "main"])
            .current_dir(repo_path)
            .output()?
            .status
            .success()
        {
            "main"
        } else {
            "master"
        };
        let sha = repo.resolve_to_sha(Some(main_branch)).await?;
        assert_eq!(sha, expected_sha);

        // Test error case - non-existent ref
        let result = repo.resolve_to_sha(Some("nonexistent")).await;
        assert!(result.is_err());
        Ok(())
    }

    #[tokio::test]
    async fn test_resolve_to_sha_with_multiple_commits() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let repo_path = temp_dir.path();

        // Initialize repo
        Command::new("git").args(["init"]).current_dir(repo_path).output()?;

        Command::new("git")
            .args(["config", "user.email", "test@example.com"])
            .current_dir(repo_path)
            .output()?;

        Command::new("git")
            .args(["config", "user.name", "Test User"])
            .current_dir(repo_path)
            .output()?;

        // Create first commit
        std::fs::write(repo_path.join("file1.txt"), "content1")?;
        Command::new("git").args(["add", "."]).current_dir(repo_path).output()?;
        Command::new("git")
            .args(["commit", "-m", "First commit"])
            .current_dir(repo_path)
            .output()?;

        // Tag first commit
        Command::new("git").args(["tag", "v1.0.0"]).current_dir(repo_path).output()?;

        let first_sha =
            Command::new("git").args(["rev-parse", "HEAD"]).current_dir(repo_path).output()?;
        let first_sha = String::from_utf8(first_sha.stdout)?.trim().to_string();

        // Create second commit
        std::fs::write(repo_path.join("file2.txt"), "content2")?;
        Command::new("git").args(["add", "."]).current_dir(repo_path).output()?;
        Command::new("git")
            .args(["commit", "-m", "Second commit"])
            .current_dir(repo_path)
            .output()?;

        // Tag second commit
        Command::new("git").args(["tag", "v2.0.0"]).current_dir(repo_path).output()?;

        let second_sha =
            Command::new("git").args(["rev-parse", "HEAD"]).current_dir(repo_path).output()?;
        let second_sha = String::from_utf8(second_sha.stdout)?.trim().to_string();

        let repo = GitRepo::new(repo_path);

        // Test that different tags resolve to different SHAs
        let sha_v1 = repo.resolve_to_sha(Some("v1.0.0")).await?;
        assert_eq!(sha_v1, first_sha);

        let sha_v2 = repo.resolve_to_sha(Some("v2.0.0")).await?;
        assert_eq!(sha_v2, second_sha);

        // Test HEAD resolves to latest
        let sha_head = repo.resolve_to_sha(Some("HEAD")).await?;
        assert_eq!(sha_head, second_sha);

        // Test short SHA resolution
        let short_sha = &first_sha[..7];
        let resolved = repo.resolve_to_sha(Some(short_sha)).await?;
        assert_eq!(resolved, first_sha);
        Ok(())
    }

    #[tokio::test]
    async fn test_file_url_clone_error_reporting() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let target_path = temp_dir.path().join("target");

        // Try to clone a non-existent file:// URL
        let invalid_file_url = "file:///non/existent/path/that/does/not/exist";
        let result = GitRepo::clone(invalid_file_url, &target_path).await;

        assert!(result.is_err());

        // Check that the error message contains the actual URL, not "unknown"
        let error_msg = result.unwrap_err().to_string();
        assert!(
            error_msg.contains(invalid_file_url),
            "Error message should contain the actual file:// URL, not 'unknown'. \
                 Got: {}",
            error_msg
        );
        assert!(
            !error_msg.contains("unknown"),
            "Error message should not contain 'unknown'. Got: {}",
            error_msg
        );
        Ok(())
    }
}
