#[cfg(test)]
#[allow(clippy::module_inception)]
mod tests {
    use super::super::*;
    use std::process::Command;
    use tempfile::TempDir;

    // Progress bar mock for testing
    mod mock {
        use std::sync::{Arc, Mutex};

        /// Mock progress bar that tracks all method calls for testing
        #[derive(Clone)]
        #[allow(dead_code)]
        pub struct MockProgressBar {
            #[allow(dead_code)]
            pub messages: Arc<Mutex<Vec<String>>>,
            #[allow(dead_code)]
            pub finished: Arc<Mutex<bool>>,
            #[allow(dead_code)]
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

            #[allow(dead_code)]
            pub fn set_message(&self, msg: impl Into<String>) {
                self.messages.lock().unwrap().push(msg.into());
            }

            #[allow(dead_code)]
            pub fn finish_with_message(&self, msg: impl Into<String>) {
                *self.finished.lock().unwrap() = true;
                *self.finished_message.lock().unwrap() = Some(msg.into());
            }

            #[allow(dead_code)]
            pub fn get_messages(&self) -> Vec<String> {
                self.messages.lock().unwrap().clone()
            }

            #[allow(dead_code)]
            pub fn is_finished(&self) -> bool {
                *self.finished.lock().unwrap()
            }

            #[allow(dead_code)]
            pub fn get_finished_message(&self) -> Option<String> {
                self.finished_message.lock().unwrap().clone()
            }
        }

        /// Wrapper to make MockProgressBar compatible with the real ProgressBar interface
        #[allow(dead_code)]
        pub struct ProgressBarWrapper {
            inner: MockProgressBar,
        }

        impl ProgressBarWrapper {
            #[allow(dead_code)]
            pub fn from_mock(mock: MockProgressBar) -> Self {
                Self { inner: mock }
            }

            #[allow(dead_code)]
            pub fn set_message(&self, msg: impl Into<String>) {
                self.inner.set_message(msg);
            }

            #[allow(dead_code)]
            pub fn finish_with_message(&self, msg: impl Into<String>) {
                self.inner.finish_with_message(msg);
            }
        }
    }

    use mock::MockProgressBar;

    #[test]
    fn test_is_git_installed() {
        assert!(is_git_installed());
    }

    #[test]
    fn test_parse_git_url() {
        let cases = vec![
            ("https://github.com/user/repo.git", ("user", "repo")),
            ("git@github.com:user/repo.git", ("user", "repo")),
            ("https://gitlab.com/user/repo", ("user", "repo")),
            ("https://bitbucket.org/user/repo.git", ("user", "repo")),
        ];

        for (url, expected) in cases {
            let result = parse_git_url(url).unwrap();
            assert_eq!(result.0, expected.0);
            assert_eq!(result.1, expected.1);
        }
    }

    #[test]
    fn test_parse_git_url_invalid() {
        let result = parse_git_url("not-a-url");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_git_url_ssh_format() {
        let result = parse_git_url("ssh://git@github.com/user/repo.git");
        assert!(result.is_ok());
        let (owner, name) = result.unwrap();
        assert_eq!(owner, "user");
        assert_eq!(name, "repo");
    }

    #[test]
    fn test_parse_git_url_more_formats() {
        let test_cases = vec![
            (
                "https://github.com/rust-lang/cargo.git",
                ("rust-lang", "cargo"),
            ),
            ("git@gitlab.com:group/project.git", ("group", "project")),
            ("ssh://git@bitbucket.org/team/repo", ("team", "repo")),
            (
                "https://github.com/user-name/repo-name",
                ("user-name", "repo-name"),
            ),
        ];

        for (url, (expected_owner, expected_repo)) in test_cases {
            let result = parse_git_url(url);
            assert!(result.is_ok(), "Failed to parse URL: {}", url);
            let (owner, repo) = result.unwrap();
            assert_eq!(owner, expected_owner, "Owner mismatch for URL: {}", url);
            assert_eq!(repo, expected_repo, "Repo mismatch for URL: {}", url);
        }
    }

    #[test]
    fn test_parse_git_url_edge_cases() {
        let invalid_urls = vec![
            "not-a-url",
            "https://example.com/something",
            "",
            // Note: file:// URLs and local paths are now valid
        ];

        for url in invalid_urls {
            let result = parse_git_url(url);
            assert!(result.is_err(), "Expected error for invalid URL: {}", url);
        }

        // Test that local paths are now valid
        let valid_local_paths = vec!["/local/path/to/repo", "./relative/path", "../parent/path"];

        for path in valid_local_paths {
            let result = parse_git_url(path);
            assert!(result.is_ok(), "Expected local path to be valid: {}", path);
        }
    }

    #[test]
    fn test_parse_git_url_file_urls() {
        // Test file:// URLs
        let test_cases = vec![
            ("file:///home/user/repos/myrepo", ("local", "myrepo")),
            ("file:///home/user/repos/myrepo.git", ("local", "myrepo")),
            ("file:///tmp/test", ("local", "test")),
            (
                "file:///var/folders/sources/official",
                ("local", "official"),
            ),
        ];

        for (url, (expected_owner, expected_repo)) in test_cases {
            let result = parse_git_url(url).unwrap();
            assert_eq!(result.0, expected_owner, "Owner mismatch for {}", url);
            assert_eq!(result.1, expected_repo, "Repo mismatch for {}", url);
        }
    }

    #[test]
    fn test_parse_git_url_special_cases() {
        // Test URLs with ports
        let url_with_port = "ssh://git@github.com:22/user/repo.git";
        let result = parse_git_url(url_with_port);
        assert!(result.is_ok());

        // Test URLs with subgroups (GitLab)
        let gitlab_subgroup = "https://gitlab.com/group/subgroup/project.git";
        let result = parse_git_url(gitlab_subgroup);
        assert!(result.is_ok());
        let (owner, name) = result.unwrap();
        assert_eq!(owner, "subgroup");
        assert_eq!(name, "project");

        // Test URL without .git extension
        let no_git_ext = "https://github.com/user/repo";
        let result = parse_git_url(no_git_ext);
        assert!(result.is_ok());
        let (owner, name) = result.unwrap();
        assert_eq!(owner, "user");
        assert_eq!(name, "repo");
    }

    #[test]
    fn test_is_git_repo() {
        let temp_dir = TempDir::new().unwrap();
        let repo = GitRepo::new(temp_dir.path());

        assert!(!repo.is_git_repo());

        Command::new("git")
            .args(["init"])
            .current_dir(temp_dir.path())
            .output()
            .unwrap();

        assert!(repo.is_git_repo());
    }

    #[test]
    fn test_git_repo_path() {
        let temp_dir = TempDir::new().unwrap();
        let repo = GitRepo::new(temp_dir.path());
        assert_eq!(repo.path(), temp_dir.path());
    }

    #[tokio::test]
    async fn test_clone_local_repo() {
        let temp_dir = TempDir::new().unwrap();
        let source_path = temp_dir.path().join("source");
        let target_path = temp_dir.path().join("target");

        // Create source repo
        std::fs::create_dir(&source_path).unwrap();
        Command::new("git")
            .args(["init", "--bare"])
            .current_dir(&source_path)
            .output()
            .unwrap();

        // Clone it
        let result = GitRepo::clone(source_path.to_str().unwrap(), &target_path, None).await;

        assert!(result.is_ok());
        let cloned_repo = result.unwrap();
        assert!(cloned_repo.is_git_repo());
    }

    #[tokio::test]
    async fn test_clone_with_progress() {
        let temp_dir = TempDir::new().unwrap();
        let bare_path = temp_dir.path().join("bare");
        let clone_path = temp_dir.path().join("clone");

        // Create bare repo
        std::fs::create_dir(&bare_path).unwrap();
        let output = Command::new("git")
            .args(["init", "--bare"])
            .current_dir(&bare_path)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "Failed to init bare repo: {:?}",
            output
        );

        // Create a mock progress bar
        let mock = MockProgressBar::new();
        let _mock_clone = mock.clone();

        // We need to use the real ProgressBar type for the API
        // This test verifies the clone succeeds with progress
        let pb = crate::utils::progress::ProgressBar::new_spinner();
        pb.set_message("Test clone");

        let result = GitRepo::clone(bare_path.to_str().unwrap(), &clone_path, Some(&pb)).await;

        assert!(result.is_ok());
        let repo = result.unwrap();
        assert!(repo.is_git_repo());
        assert!(clone_path.exists());

        // The progress bar should have been used (finish_with_message called)
        pb.finish_with_message("Clone complete");
    }

    #[tokio::test]
    async fn test_clone_invalid_url() {
        let target_dir = TempDir::new().unwrap();
        let target_path = target_dir.path().join("cloned");

        let result = GitRepo::clone("/non/existent/path", &target_path, None).await;

        assert!(result.is_err());
        assert!(!target_path.exists());
    }

    #[tokio::test]
    async fn test_clone_invalid_url_detailed() {
        let target_dir = TempDir::new().unwrap();
        let target_path = target_dir.path().join("cloned");

        // Test various invalid URLs
        let invalid_urls = vec![
            "/non/existent/path",
            "http://invalid-git-url.test",
            "not-a-url",
            "",
        ];

        for url in invalid_urls {
            let result = GitRepo::clone(url, &target_path, None).await;
            assert!(result.is_err(), "Expected error for URL: {}", url);
            if let Err(error) = result {
                assert!(
                    error.to_string().contains("Failed to clone")
                        || error.to_string().contains("Failed to execute")
                );
            }
        }
    }

    #[tokio::test]
    async fn test_clone_stderr_error_message() {
        let target_dir = TempDir::new().unwrap();
        let target_path = target_dir.path().join("cloned");

        // Try to clone with an invalid URL that will produce stderr
        let result = GitRepo::clone(
            "https://invalid.host.that.does.not.exist.9999/repo.git",
            &target_path,
            None,
        )
        .await;

        assert!(result.is_err());
        if let Err(error) = result {
            let error_msg = error.to_string();
            assert!(error_msg.contains("Failed to clone"));
        }
    }

    #[tokio::test]
    async fn test_fetch_and_pull() {
        // Ensure we run from a stable working directory
        // (some other test may have left us in a deleted temp directory)
        let _ = std::env::set_current_dir(std::env::temp_dir());

        let temp_dir = TempDir::new().unwrap();
        let bare_path = temp_dir.path().join("bare");
        let repo1_path = temp_dir.path().join("repo1");
        let repo2_path = temp_dir.path().join("repo2");

        // Create bare repo
        std::fs::create_dir(&bare_path).unwrap();
        let output = Command::new("git")
            .args(["init", "--bare"])
            .current_dir(&bare_path)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "Failed to init bare repo: {:?}",
            output
        );

        // Create a temporary clone to add initial commit
        let init_repo = temp_dir.path().join("init_repo");
        Command::new("git")
            .args(["init"])
            .arg(&init_repo)
            .current_dir(temp_dir.path()) // Ensure stable working directory
            .output()
            .unwrap();

        // Configure the init repo
        Command::new("git")
            .args(["config", "user.email", "test@example.com"])
            .current_dir(&init_repo)
            .output()
            .unwrap();
        Command::new("git")
            .args(["config", "user.name", "Test User"])
            .current_dir(&init_repo)
            .output()
            .unwrap();

        // Create initial commit
        std::fs::write(init_repo.join("README.md"), "Initial").unwrap();
        Command::new("git")
            .args(["add", "."])
            .current_dir(&init_repo)
            .output()
            .unwrap();
        Command::new("git")
            .args(["commit", "-m", "Initial commit"])
            .current_dir(&init_repo)
            .output()
            .unwrap();

        // Push to bare repo
        Command::new("git")
            .args(["remote", "add", "origin", bare_path.to_str().unwrap()])
            .current_dir(&init_repo)
            .output()
            .unwrap();
        // Get the current branch name (could be master or main)
        let branch_output = Command::new("git")
            .args(["branch", "--show-current"])
            .current_dir(&init_repo)
            .output()
            .unwrap();
        let branch = String::from_utf8_lossy(&branch_output.stdout)
            .trim()
            .to_string();
        let branch = if branch.is_empty() {
            "master"
        } else {
            branch.as_str()
        };

        let push_output = Command::new("git")
            .args(["push", "-u", "origin", branch])
            .current_dir(&init_repo)
            .output()
            .unwrap();
        assert!(
            push_output.status.success(),
            "Failed to push to bare repo: {:?}",
            String::from_utf8_lossy(&push_output.stderr)
        );

        // Now clone to repo1
        let clone_output = Command::new("git")
            .args([
                "clone",
                bare_path.to_str().unwrap(),
                repo1_path.to_str().unwrap(),
            ])
            .current_dir(temp_dir.path()) // Ensure we run from a stable directory
            .output()
            .unwrap();
        assert!(
            clone_output.status.success(),
            "Failed to clone to repo1: {:?}",
            String::from_utf8_lossy(&clone_output.stderr)
        );

        // Configure repo1
        Command::new("git")
            .args(["config", "user.email", "test@example.com"])
            .current_dir(&repo1_path)
            .output()
            .unwrap();
        Command::new("git")
            .args(["config", "user.name", "Test User"])
            .current_dir(&repo1_path)
            .output()
            .unwrap();

        // Create commit in repo1
        std::fs::write(repo1_path.join("file1.txt"), "content1").unwrap();
        Command::new("git")
            .args(["add", "."])
            .current_dir(&repo1_path)
            .output()
            .unwrap();
        Command::new("git")
            .args(["commit", "-m", "First commit"])
            .current_dir(&repo1_path)
            .output()
            .unwrap();

        // Get the current branch name
        let branch_output = Command::new("git")
            .args(["branch", "--show-current"])
            .current_dir(&repo1_path)
            .output()
            .unwrap();
        let branch = String::from_utf8_lossy(&branch_output.stdout)
            .trim()
            .to_string();
        let branch = if branch.is_empty() {
            "master".to_string()
        } else {
            branch
        };

        let push_output = Command::new("git")
            .args(["push", "origin", &branch])
            .current_dir(&repo1_path)
            .output()
            .unwrap();
        assert!(
            push_output.status.success(),
            "Failed to push from repo1: {:?}",
            String::from_utf8_lossy(&push_output.stderr)
        );

        // Clone to repo2
        let repo2 = GitRepo::clone(bare_path.to_str().unwrap(), &repo2_path, None)
            .await
            .unwrap();

        // Make another commit in repo1
        std::fs::write(repo1_path.join("file2.txt"), "content2").unwrap();
        Command::new("git")
            .args(["add", "."])
            .current_dir(&repo1_path)
            .output()
            .unwrap();
        Command::new("git")
            .args(["commit", "-m", "Second commit"])
            .current_dir(&repo1_path)
            .output()
            .unwrap();

        // Get the current branch name again
        let branch_output = Command::new("git")
            .args(["branch", "--show-current"])
            .current_dir(&repo1_path)
            .output()
            .unwrap();
        let branch = String::from_utf8_lossy(&branch_output.stdout)
            .trim()
            .to_string();
        let branch = if branch.is_empty() {
            "master".to_string()
        } else {
            branch
        };

        Command::new("git")
            .args(["push", "origin", &branch])
            .current_dir(&repo1_path)
            .output()
            .unwrap();

        // Fetch in repo2
        let fetch_result = repo2.fetch(None, None).await;
        assert!(fetch_result.is_ok());

        // Pull in repo2
        let pull_result = repo2.pull(None).await;
        assert!(pull_result.is_ok());

        // Verify file2.txt now exists in repo2
        assert!(repo2_path.join("file2.txt").exists());
    }

    #[tokio::test]
    async fn test_fetch_with_progress() {
        let temp_dir = TempDir::new().unwrap();
        let bare_path = temp_dir.path().join("bare");
        let repo_path = temp_dir.path().join("repo");

        // Setup bare repo
        std::fs::create_dir(&bare_path).unwrap();
        Command::new("git")
            .args(["init", "--bare"])
            .current_dir(&bare_path)
            .output()
            .unwrap();

        // Clone it
        let repo = GitRepo::clone(bare_path.to_str().unwrap(), &repo_path, None)
            .await
            .unwrap();

        // Fetch with progress
        let pb = crate::utils::progress::ProgressBar::new_spinner();
        pb.set_message("Test fetch");

        let result = repo.fetch(None, Some(&pb)).await;
        assert!(result.is_ok());

        pb.finish_with_message("Fetch complete");
    }

    #[tokio::test]
    async fn test_fetch_with_no_network() {
        let temp_dir = TempDir::new().unwrap();
        let repo_path = temp_dir.path();

        Command::new("git")
            .args(["init"])
            .current_dir(repo_path)
            .output()
            .unwrap();

        // Add a fake remote
        Command::new("git")
            .args([
                "remote",
                "add",
                "origin",
                "https://non-existent-host-9999.test/repo.git",
            ])
            .current_dir(repo_path)
            .output()
            .unwrap();

        let repo = GitRepo::new(repo_path);
        let result = repo.fetch(None, None).await;

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Git operation failed: fetch"));
    }

    #[tokio::test]
    async fn test_pull_with_progress() {
        let temp_dir = TempDir::new().unwrap();
        let bare_path = temp_dir.path().join("bare");
        let repo_path = temp_dir.path().join("repo");

        // Setup
        std::fs::create_dir(&bare_path).unwrap();
        Command::new("git")
            .args(["init", "--bare"])
            .current_dir(&bare_path)
            .output()
            .unwrap();

        let repo = GitRepo::clone(bare_path.to_str().unwrap(), &repo_path, None)
            .await
            .unwrap();

        // Pull with progress
        let pb = crate::utils::progress::ProgressBar::new_spinner();
        pb.set_message("Test pull");

        // This will fail (no upstream) but we're testing progress handling
        let _ = repo.pull(Some(&pb)).await;

        pb.finish_with_message("Pull attempt complete");
    }

    #[tokio::test]
    async fn test_pull_with_conflicts() {
        let temp_dir = TempDir::new().unwrap();
        let repo_path = temp_dir.path();

        Command::new("git")
            .args(["init"])
            .current_dir(repo_path)
            .output()
            .unwrap();

        Command::new("git")
            .args(["config", "user.email", "test@ccpm.test"])
            .current_dir(repo_path)
            .output()
            .unwrap();

        Command::new("git")
            .args(["config", "user.name", "Test User"])
            .current_dir(repo_path)
            .output()
            .unwrap();

        // Create initial commit
        std::fs::write(repo_path.join("test.txt"), "content").unwrap();
        Command::new("git")
            .args(["add", "."])
            .current_dir(repo_path)
            .output()
            .unwrap();
        Command::new("git")
            .args(["commit", "-m", "Initial"])
            .current_dir(repo_path)
            .output()
            .unwrap();

        let repo = GitRepo::new(repo_path);
        let result = repo.pull(None).await;

        // Should fail because there's no upstream branch
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_checkout() {
        let temp_dir = TempDir::new().unwrap();
        let repo_path = temp_dir.path();

        // Initialize a git repo
        Command::new("git")
            .args(["init"])
            .current_dir(repo_path)
            .output()
            .unwrap();

        Command::new("git")
            .args(["config", "user.email", "test@example.com"])
            .current_dir(repo_path)
            .output()
            .unwrap();

        Command::new("git")
            .args(["config", "user.name", "Test User"])
            .current_dir(repo_path)
            .output()
            .unwrap();

        // Create initial commit
        std::fs::write(repo_path.join("README.md"), "Test").unwrap();
        Command::new("git")
            .args(["add", "."])
            .current_dir(repo_path)
            .output()
            .unwrap();
        Command::new("git")
            .args(["commit", "-m", "Initial commit"])
            .current_dir(repo_path)
            .output()
            .unwrap();

        // Create a tag
        Command::new("git")
            .args(["tag", "v1.0.0"])
            .current_dir(repo_path)
            .output()
            .unwrap();

        // Create another commit
        std::fs::write(repo_path.join("file2.txt"), "Test2").unwrap();
        Command::new("git")
            .args(["add", "."])
            .current_dir(repo_path)
            .output()
            .unwrap();
        Command::new("git")
            .args(["commit", "-m", "Second commit"])
            .current_dir(repo_path)
            .output()
            .unwrap();

        let repo = GitRepo::new(repo_path);

        // Checkout the tag
        let result = repo.checkout("v1.0.0").await;
        assert!(result.is_ok());

        // Verify we're in detached HEAD state at v1.0.0
        assert!(!repo_path.join("file2.txt").exists());
    }

    #[tokio::test]
    async fn test_checkout_branch() {
        let temp_dir = TempDir::new().unwrap();
        let repo_path = temp_dir.path();

        Command::new("git")
            .args(["init"])
            .current_dir(repo_path)
            .output()
            .unwrap();

        Command::new("git")
            .args(["config", "user.email", "test@ccpm.test"])
            .current_dir(repo_path)
            .output()
            .unwrap();

        Command::new("git")
            .args(["config", "user.name", "CCPM Test"])
            .current_dir(repo_path)
            .output()
            .unwrap();

        // Create initial commit
        std::fs::write(repo_path.join("main.txt"), "Main branch").unwrap();
        Command::new("git")
            .args(["add", "."])
            .current_dir(repo_path)
            .output()
            .unwrap();
        Command::new("git")
            .args(["commit", "-m", "Main commit"])
            .current_dir(repo_path)
            .output()
            .unwrap();

        // Create feature branch
        Command::new("git")
            .args(["checkout", "-b", "feature"])
            .current_dir(repo_path)
            .output()
            .unwrap();

        std::fs::write(repo_path.join("feature.txt"), "Feature branch").unwrap();
        Command::new("git")
            .args(["add", "."])
            .current_dir(repo_path)
            .output()
            .unwrap();
        Command::new("git")
            .args(["commit", "-m", "Feature commit"])
            .current_dir(repo_path)
            .output()
            .unwrap();

        let repo = GitRepo::new(repo_path);

        // Verify we're on feature branch
        assert_eq!(repo.get_current_branch().await.unwrap(), "feature");
        assert!(repo_path.join("feature.txt").exists());

        // Checkout main branch
        let main_branch = if Command::new("git")
            .args(["rev-parse", "--verify", "main"])
            .current_dir(repo_path)
            .output()
            .unwrap()
            .status
            .success()
        {
            "main"
        } else {
            "master"
        };

        repo.checkout(main_branch).await.unwrap();
        assert!(!repo_path.join("feature.txt").exists());
        assert!(repo_path.join("main.txt").exists());

        // Checkout back to feature
        repo.checkout("feature").await.unwrap();
        assert!(repo_path.join("feature.txt").exists());
    }

    #[tokio::test]
    async fn test_checkout_commit_hash() {
        let temp_dir = TempDir::new().unwrap();
        let repo_path = temp_dir.path();

        Command::new("git")
            .args(["init"])
            .current_dir(repo_path)
            .output()
            .unwrap();

        Command::new("git")
            .args(["config", "user.email", "test@ccpm.test"])
            .current_dir(repo_path)
            .output()
            .unwrap();

        Command::new("git")
            .args(["config", "user.name", "Test User"])
            .current_dir(repo_path)
            .output()
            .unwrap();

        // Create first commit
        std::fs::write(repo_path.join("file1.txt"), "content1").unwrap();
        Command::new("git")
            .args(["add", "."])
            .current_dir(repo_path)
            .output()
            .unwrap();
        Command::new("git")
            .args(["commit", "-m", "First commit"])
            .current_dir(repo_path)
            .output()
            .unwrap();

        // Get first commit hash
        let output = Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(repo_path)
            .output()
            .unwrap();
        let first_commit = String::from_utf8_lossy(&output.stdout).trim().to_string();

        // Create second commit
        std::fs::write(repo_path.join("file2.txt"), "content2").unwrap();
        Command::new("git")
            .args(["add", "."])
            .current_dir(repo_path)
            .output()
            .unwrap();
        Command::new("git")
            .args(["commit", "-m", "Second commit"])
            .current_dir(repo_path)
            .output()
            .unwrap();

        let repo = GitRepo::new(repo_path);

        // Checkout first commit by hash
        repo.checkout(&first_commit).await.unwrap();

        // Verify we're at first commit (file2 shouldn't exist)
        assert!(repo_path.join("file1.txt").exists());
        assert!(!repo_path.join("file2.txt").exists());
    }

    #[tokio::test]
    async fn test_checkout_invalid_ref() {
        let temp_dir = TempDir::new().unwrap();
        let repo_path = temp_dir.path();

        Command::new("git")
            .args(["init"])
            .current_dir(repo_path)
            .output()
            .unwrap();

        Command::new("git")
            .args(["config", "user.email", "test@ccpm.test"])
            .current_dir(repo_path)
            .output()
            .unwrap();

        Command::new("git")
            .args(["config", "user.name", "CCPM Test"])
            .current_dir(repo_path)
            .output()
            .unwrap();

        std::fs::write(repo_path.join("README.md"), "# Test").unwrap();
        Command::new("git")
            .args(["add", "."])
            .current_dir(repo_path)
            .output()
            .unwrap();
        Command::new("git")
            .args(["commit", "-m", "Initial commit"])
            .current_dir(repo_path)
            .output()
            .unwrap();

        let repo = GitRepo::new(repo_path);
        let result = repo.checkout("non-existent-branch").await;

        assert!(result.is_err());
        let error_message = format!("{:?}", result.unwrap_err());
        assert!(error_message.contains("Failed to checkout"));
    }

    #[tokio::test]
    async fn test_list_tags() {
        let temp_dir = TempDir::new().unwrap();
        let repo_path = temp_dir.path();

        Command::new("git")
            .args(["init"])
            .current_dir(repo_path)
            .output()
            .unwrap();

        Command::new("git")
            .args(["config", "user.email", "test@ccpm.test"])
            .current_dir(repo_path)
            .output()
            .unwrap();

        Command::new("git")
            .args(["config", "user.name", "CCPM Test"])
            .current_dir(repo_path)
            .output()
            .unwrap();

        std::fs::write(repo_path.join("README.md"), "# Test").unwrap();
        Command::new("git")
            .args(["add", "."])
            .current_dir(repo_path)
            .output()
            .unwrap();
        Command::new("git")
            .args(["commit", "-m", "Initial commit"])
            .current_dir(repo_path)
            .output()
            .unwrap();

        // Add multiple tags
        let tags_to_add = vec!["v1.0.0", "v1.1.0", "v2.0.0-beta", "release-1.2.3"];
        for tag in &tags_to_add {
            Command::new("git")
                .args(["tag", tag])
                .current_dir(repo_path)
                .output()
                .unwrap();
        }

        let repo = GitRepo::new(repo_path);
        let mut tags = repo.list_tags().await.unwrap();
        tags.sort();

        assert_eq!(tags.len(), 4);
        assert!(tags.contains(&"v1.0.0".to_string()));
        assert!(tags.contains(&"v2.0.0-beta".to_string()));
    }

    #[tokio::test]
    async fn test_list_tags_sorted() {
        let temp_dir = TempDir::new().unwrap();
        let repo_path = temp_dir.path();

        Command::new("git")
            .args(["init"])
            .current_dir(repo_path)
            .output()
            .unwrap();

        Command::new("git")
            .args(["config", "user.email", "test@ccpm.test"])
            .current_dir(repo_path)
            .output()
            .unwrap();

        Command::new("git")
            .args(["config", "user.name", "Test User"])
            .current_dir(repo_path)
            .output()
            .unwrap();

        std::fs::write(repo_path.join("README.md"), "# Test").unwrap();
        Command::new("git")
            .args(["add", "."])
            .current_dir(repo_path)
            .output()
            .unwrap();
        Command::new("git")
            .args(["commit", "-m", "Initial"])
            .current_dir(repo_path)
            .output()
            .unwrap();

        // Add tags in non-sorted order
        let tags = vec!["v2.0.0", "v1.0.0", "v1.2.0", "v1.1.0", "v3.0.0-alpha"];
        for tag in &tags {
            Command::new("git")
                .args(["tag", tag])
                .current_dir(repo_path)
                .output()
                .unwrap();
        }

        let repo = GitRepo::new(repo_path);
        let listed_tags = repo.list_tags().await.unwrap();

        // Git tag -l returns tags in alphabetical order
        assert_eq!(listed_tags.len(), 5);
        // Verify they exist (order may vary by git version)
        for tag in tags {
            assert!(listed_tags.contains(&tag.to_string()));
        }
    }

    #[tokio::test]
    async fn test_get_remote_url() {
        let temp_dir = TempDir::new().unwrap();
        let repo_path = temp_dir.path();

        // Initialize a git repo
        Command::new("git")
            .args(["init"])
            .current_dir(repo_path)
            .output()
            .unwrap();

        // Add a remote
        Command::new("git")
            .args([
                "remote",
                "add",
                "origin",
                "https://github.com/test/repo.git",
            ])
            .current_dir(repo_path)
            .output()
            .unwrap();

        let repo = GitRepo::new(repo_path);
        let url = repo.get_remote_url().await.unwrap();
        // Accept both HTTPS and SSH formats (git config may rewrite URLs)
        assert!(
            url == "https://github.com/test/repo.git"
                || url == "ssh://git@github.com/test/repo.git"
                || url == "git@github.com:test/repo.git"
        );
    }

    #[tokio::test]
    async fn test_get_remote_url_no_remote() {
        let temp_dir = TempDir::new().unwrap();
        let repo_path = temp_dir.path();

        Command::new("git")
            .args(["init"])
            .current_dir(repo_path)
            .output()
            .unwrap();

        let repo = GitRepo::new(repo_path);
        let result = repo.get_remote_url().await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_get_current_branch() {
        let temp_dir = TempDir::new().unwrap();
        let repo_path = temp_dir.path();

        Command::new("git")
            .args(["init"])
            .current_dir(repo_path)
            .output()
            .unwrap();

        Command::new("git")
            .args(["config", "user.email", "test@example.com"])
            .current_dir(repo_path)
            .output()
            .unwrap();

        Command::new("git")
            .args(["config", "user.name", "Test User"])
            .current_dir(repo_path)
            .output()
            .unwrap();

        // Create initial commit
        std::fs::write(repo_path.join("README.md"), "Test").unwrap();
        Command::new("git")
            .args(["add", "."])
            .current_dir(repo_path)
            .output()
            .unwrap();
        Command::new("git")
            .args(["commit", "-m", "Initial commit"])
            .current_dir(repo_path)
            .output()
            .unwrap();

        let repo = GitRepo::new(repo_path);

        // Test get_current_branch
        let branch = repo.get_current_branch().await.unwrap();
        assert!(branch == "main" || branch == "master");
    }

    #[tokio::test]
    async fn test_push() {
        // Ensure we run from a stable working directory
        // (some other test may have left us in a deleted temp directory)
        let _ = std::env::set_current_dir(std::env::temp_dir());

        let temp_dir = TempDir::new().unwrap();
        let bare_path = temp_dir.path().join("bare");
        let local_path = temp_dir.path().join("local");

        // Create bare repo
        std::fs::create_dir(&bare_path).unwrap();
        let output = Command::new("git")
            .args(["init", "--bare"])
            .current_dir(&bare_path)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "Failed to init bare repo: {:?}",
            output
        );

        // Initialize the bare repo with a commit
        let init_repo = temp_dir.path().join("init_push");
        Command::new("git")
            .args(["init"])
            .arg(&init_repo)
            .current_dir(temp_dir.path()) // Ensure stable working directory
            .output()
            .unwrap();
        Command::new("git")
            .args(["config", "user.email", "test@ccpm.test"])
            .current_dir(&init_repo)
            .output()
            .unwrap();
        Command::new("git")
            .args(["config", "user.name", "CCPM Test"])
            .current_dir(&init_repo)
            .output()
            .unwrap();
        std::fs::write(init_repo.join("README.md"), "Initial").unwrap();
        Command::new("git")
            .args(["add", "."])
            .current_dir(&init_repo)
            .output()
            .unwrap();
        Command::new("git")
            .args(["commit", "-m", "Initial commit"])
            .current_dir(&init_repo)
            .output()
            .unwrap();
        Command::new("git")
            .args(["remote", "add", "origin", bare_path.to_str().unwrap()])
            .current_dir(&init_repo)
            .output()
            .unwrap();
        // Get the current branch name (could be master or main)
        let branch_output = Command::new("git")
            .args(["branch", "--show-current"])
            .current_dir(&init_repo)
            .output()
            .unwrap();
        let branch = String::from_utf8_lossy(&branch_output.stdout)
            .trim()
            .to_string();
        let branch = if branch.is_empty() {
            "master"
        } else {
            branch.as_str()
        };

        let push_output = Command::new("git")
            .args(["push", "-u", "origin", branch])
            .current_dir(&init_repo)
            .output()
            .unwrap();
        assert!(
            push_output.status.success(),
            "Failed to push to bare repo: {:?}",
            String::from_utf8_lossy(&push_output.stderr)
        );

        // Clone it
        let repo = GitRepo::clone(bare_path.to_str().unwrap(), &local_path, None)
            .await
            .unwrap();

        // Configure git
        Command::new("git")
            .args(["config", "user.email", "test@ccpm.test"])
            .current_dir(&local_path)
            .output()
            .unwrap();
        Command::new("git")
            .args(["config", "user.name", "CCPM Test"])
            .current_dir(&local_path)
            .output()
            .unwrap();

        // Add file and commit
        repo.add_file("new_file.txt", "New content").await.unwrap();
        repo.commit("Add new file").await.unwrap();

        // Push to remote
        let branch = repo.get_current_branch().await.unwrap();
        let result = repo.push(&branch, None).await;
        assert!(result.is_ok());

        // Verify by cloning again
        let other_path = temp_dir.path().join("other");
        let _other_repo = GitRepo::clone(bare_path.to_str().unwrap(), &other_path, None)
            .await
            .unwrap();

        assert!(other_path.join("new_file.txt").exists());
    }

    #[tokio::test]
    async fn test_error_handling_non_git_repo() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().to_path_buf();

        // GitRepo::open doesn't exist, using new instead
        // Try git operations on non-git directory will fail

        // Try git operations on non-git directory
        let fake_repo = GitRepo { path };

        let result = fake_repo.fetch(None, None).await;
        assert!(result.is_err());

        let result = fake_repo.get_current_branch().await;
        assert!(result.is_err());

        let result = fake_repo.list_tags().await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_push_with_progress() {
        // Ensure we run from a stable working directory
        // (some other test may have left us in a deleted temp directory)
        let _ = std::env::set_current_dir(std::env::temp_dir());

        let temp_dir = TempDir::new().unwrap();
        let bare_path = temp_dir.path().join("bare");
        let local_path = temp_dir.path().join("local");

        // Create bare repo
        std::fs::create_dir(&bare_path).unwrap();
        let output = Command::new("git")
            .args(["init", "--bare"])
            .current_dir(&bare_path)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "Failed to init bare repo: {:?}",
            output
        );

        // Initialize the bare repo with a commit
        let init_repo = temp_dir.path().join("init_push_progress");
        Command::new("git")
            .args(["init"])
            .arg(&init_repo)
            .current_dir(temp_dir.path()) // Ensure stable working directory
            .output()
            .unwrap();
        Command::new("git")
            .args(["config", "user.email", "test@ccpm.test"])
            .current_dir(&init_repo)
            .output()
            .unwrap();
        Command::new("git")
            .args(["config", "user.name", "CCPM Test"])
            .current_dir(&init_repo)
            .output()
            .unwrap();
        std::fs::write(init_repo.join("README.md"), "Initial").unwrap();
        Command::new("git")
            .args(["add", "."])
            .current_dir(&init_repo)
            .output()
            .unwrap();
        Command::new("git")
            .args(["commit", "-m", "Initial commit"])
            .current_dir(&init_repo)
            .output()
            .unwrap();
        Command::new("git")
            .args(["remote", "add", "origin", bare_path.to_str().unwrap()])
            .current_dir(&init_repo)
            .output()
            .unwrap();
        // Get the current branch name (could be master or main)
        let branch_output = Command::new("git")
            .args(["branch", "--show-current"])
            .current_dir(&init_repo)
            .output()
            .unwrap();
        let branch = String::from_utf8_lossy(&branch_output.stdout)
            .trim()
            .to_string();
        let branch = if branch.is_empty() {
            "master"
        } else {
            branch.as_str()
        };

        let push_output = Command::new("git")
            .args(["push", "-u", "origin", branch])
            .current_dir(&init_repo)
            .output()
            .unwrap();
        assert!(
            push_output.status.success(),
            "Failed to push to bare repo: {:?}",
            String::from_utf8_lossy(&push_output.stderr)
        );

        // Clone it
        let repo = GitRepo::clone(bare_path.to_str().unwrap(), &local_path, None)
            .await
            .unwrap();

        // Configure git
        Command::new("git")
            .args(["config", "user.email", "test@ccpm.test"])
            .current_dir(&local_path)
            .output()
            .unwrap();
        Command::new("git")
            .args(["config", "user.name", "Test User"])
            .current_dir(&local_path)
            .output()
            .unwrap();

        // Add file and commit
        repo.add_file("test.txt", "test content").await.unwrap();
        repo.commit("Test commit").await.unwrap();

        // Push with progress
        let pb = crate::utils::progress::ProgressBar::new_spinner();
        pb.set_message("Test push");

        let branch = repo.get_current_branch().await.unwrap();
        let result = repo.push(&branch, Some(&pb)).await;
        assert!(result.is_ok());

        pb.finish_with_message("Push complete");
    }

    #[tokio::test]
    async fn test_push_no_remote() {
        let temp_dir = TempDir::new().unwrap();
        let repo_path = temp_dir.path();

        Command::new("git")
            .args(["init"])
            .current_dir(repo_path)
            .output()
            .unwrap();

        Command::new("git")
            .args(["config", "user.email", "test@ccpm.test"])
            .current_dir(repo_path)
            .output()
            .unwrap();

        Command::new("git")
            .args(["config", "user.name", "CCPM Test"])
            .current_dir(repo_path)
            .output()
            .unwrap();

        std::fs::write(repo_path.join("test.txt"), "content").unwrap();
        Command::new("git")
            .args(["add", "."])
            .current_dir(repo_path)
            .output()
            .unwrap();
        Command::new("git")
            .args(["commit", "-m", "Initial commit"])
            .current_dir(repo_path)
            .output()
            .unwrap();

        let repo = GitRepo::new(repo_path);
        let result = repo.push("main", None).await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_add_file_and_commit() {
        let temp_dir = TempDir::new().unwrap();
        let repo_path = temp_dir.path();

        Command::new("git")
            .args(["init"])
            .current_dir(repo_path)
            .output()
            .unwrap();

        Command::new("git")
            .args(["config", "user.email", "test@ccpm.test"])
            .current_dir(repo_path)
            .output()
            .unwrap();

        Command::new("git")
            .args(["config", "user.name", "CCPM Test"])
            .current_dir(repo_path)
            .output()
            .unwrap();

        let repo = GitRepo::new(repo_path);

        // Add a file
        repo.add_file("test.txt", "Test content").await.unwrap();
        assert!(repo_path.join("test.txt").exists());

        // Commit it
        let commit_hash = repo.commit("Test commit").await.unwrap();
        assert!(!commit_hash.is_empty());
        assert_eq!(commit_hash.len(), 40); // SHA-1 hash length

        // Add a nested file
        repo.add_file("nested/dir/file.txt", "Nested content")
            .await
            .unwrap();
        assert!(repo_path.join("nested/dir/file.txt").exists());

        let commit_hash2 = repo.commit("Add nested file").await.unwrap();
        assert_ne!(commit_hash, commit_hash2);
    }

    #[tokio::test]
    async fn test_add_file_nested_directory_creation() {
        let temp_dir = TempDir::new().unwrap();
        let repo_path = temp_dir.path();

        Command::new("git")
            .args(["init"])
            .current_dir(repo_path)
            .output()
            .unwrap();

        let repo = GitRepo::new(repo_path);

        // Test creating deeply nested directories
        let result = repo
            .add_file("deep/nested/path/to/file.txt", "content")
            .await;
        assert!(result.is_ok());
        assert!(repo_path.join("deep/nested/path/to/file.txt").exists());
    }

    #[tokio::test]
    async fn test_add_file_creates_parent_dirs() {
        let temp_dir = TempDir::new().unwrap();
        let repo_path = temp_dir.path();

        Command::new("git")
            .args(["init"])
            .current_dir(repo_path)
            .output()
            .unwrap();

        let repo = GitRepo::new(repo_path);

        // Ensure parent directory doesn't exist
        assert!(!repo_path.join("a").exists());

        // Add file with nested path - should create parents
        let result = repo.add_file("a/b/c/d/file.txt", "nested content").await;
        assert!(result.is_ok());

        // Verify all parent directories were created
        assert!(repo_path.join("a").exists());
        assert!(repo_path.join("a/b").exists());
        assert!(repo_path.join("a/b/c").exists());
        assert!(repo_path.join("a/b/c/d").exists());
        assert!(repo_path.join("a/b/c/d/file.txt").exists());

        // Verify content
        let content = std::fs::read_to_string(repo_path.join("a/b/c/d/file.txt")).unwrap();
        assert_eq!(content, "nested content");
    }

    #[tokio::test]
    async fn test_commit_nothing_staged() {
        let temp_dir = TempDir::new().unwrap();
        let repo_path = temp_dir.path();

        Command::new("git")
            .args(["init"])
            .current_dir(repo_path)
            .output()
            .unwrap();

        Command::new("git")
            .args(["config", "user.email", "test@ccpm.test"])
            .current_dir(repo_path)
            .output()
            .unwrap();

        Command::new("git")
            .args(["config", "user.name", "Test User"])
            .current_dir(repo_path)
            .output()
            .unwrap();

        let repo = GitRepo::new(repo_path);

        // Try to commit with nothing staged
        let result = repo.commit("Empty commit").await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Git operation failed: commit"));
    }

    #[tokio::test]
    async fn test_unicode_paths_and_messages() {
        let temp_dir = TempDir::new().unwrap();
        let repo_path = temp_dir.path();

        Command::new("git")
            .args(["init"])
            .current_dir(repo_path)
            .output()
            .unwrap();

        Command::new("git")
            .args(["config", "user.email", "test@ccpm.test"])
            .current_dir(repo_path)
            .output()
            .unwrap();

        Command::new("git")
            .args(["config", "user.name", "Test User"])
            .current_dir(repo_path)
            .output()
            .unwrap();

        let repo = GitRepo::new(repo_path);

        // Test unicode in file paths
        repo.add_file("中文文件.txt", "Content with emoji 🚀")
            .await
            .unwrap();
        assert!(repo_path.join("中文文件.txt").exists());

        // Test unicode in commit messages
        let commit_hash = repo.commit("提交信息 with émoji 🎉").await.unwrap();
        assert!(!commit_hash.is_empty());

        // Verify the commit was created
        let output = Command::new("git")
            .args(["log", "--oneline", "-1"])
            .current_dir(repo_path)
            .output()
            .unwrap();
        let log = String::from_utf8_lossy(&output.stdout);
        assert!(log.contains("提交信息 with émoji 🎉"));
    }

    #[tokio::test]
    async fn test_concurrent_operations() {
        let temp_dir = TempDir::new().unwrap();
        let repo_path = temp_dir.path().to_path_buf();

        Command::new("git")
            .args(["init"])
            .current_dir(&repo_path)
            .output()
            .unwrap();

        Command::new("git")
            .args(["config", "user.email", "test@ccpm.test"])
            .current_dir(&repo_path)
            .output()
            .unwrap();

        Command::new("git")
            .args(["config", "user.name", "Test User"])
            .current_dir(&repo_path)
            .output()
            .unwrap();

        // Create initial commit
        std::fs::write(repo_path.join("initial.txt"), "initial").unwrap();
        Command::new("git")
            .args(["add", "."])
            .current_dir(&repo_path)
            .output()
            .unwrap();
        Command::new("git")
            .args(["commit", "-m", "Initial"])
            .current_dir(&repo_path)
            .output()
            .unwrap();

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
        let result1 = handle1.await.unwrap();
        let result2 = handle2.await.unwrap();

        assert!(result1.is_ok());
        assert!(result2.is_ok());
    }

    #[tokio::test]
    async fn test_trait_implementation() {
        let temp_dir = TempDir::new().unwrap();
        let repo_path = temp_dir.path();

        Command::new("git")
            .args(["init"])
            .current_dir(repo_path)
            .output()
            .unwrap();

        Command::new("git")
            .args(["config", "user.email", "test@ccpm.test"])
            .current_dir(repo_path)
            .output()
            .unwrap();

        Command::new("git")
            .args(["config", "user.name", "CCPM Test"])
            .current_dir(repo_path)
            .output()
            .unwrap();

        std::fs::write(repo_path.join("README.md"), "# Test").unwrap();
        Command::new("git")
            .args(["add", "."])
            .current_dir(repo_path)
            .output()
            .unwrap();
        Command::new("git")
            .args(["commit", "-m", "Initial commit"])
            .current_dir(repo_path)
            .output()
            .unwrap();

        let repo = GitRepo::new(repo_path);

        // Test methods directly
        assert!(repo.is_git_repo());
        assert!(repo.path().exists());
        let tags = repo.list_tags().await.unwrap();
        assert_eq!(tags.len(), 0);
    }

    #[tokio::test]
    #[cfg(unix)]
    async fn test_unix_specific_paths() {
        let temp_dir = TempDir::new().unwrap();
        let repo_path = temp_dir.path();

        Command::new("git")
            .args(["init"])
            .current_dir(repo_path)
            .output()
            .unwrap();

        let path_with_spaces = repo_path.join("path with spaces");
        std::fs::create_dir(&path_with_spaces).unwrap();

        let repo = GitRepo::new(repo_path);
        repo.add_file("path with spaces/file.txt", "content")
            .await
            .unwrap();

        assert!(path_with_spaces.join("file.txt").exists());
    }

    #[tokio::test]
    #[cfg(windows)]
    async fn test_windows_specific_paths() {
        let temp_dir = TempDir::new().unwrap();
        let repo_path = temp_dir.path();

        Command::new("git")
            .args(["init"])
            .current_dir(repo_path)
            .output()
            .unwrap();

        let path_with_spaces = repo_path.join("path with spaces");
        std::fs::create_dir(&path_with_spaces).unwrap();

        let repo = GitRepo::new(repo_path);
        repo.add_file("path with spaces\\file.txt", "content")
            .await
            .unwrap();

        assert!(path_with_spaces.join("file.txt").exists());
    }

    #[tokio::test]
    #[cfg(windows)]
    async fn test_long_path_windows() {
        let temp_dir = TempDir::new().unwrap();
        let repo_path = temp_dir.path();

        Command::new("git")
            .args(["init"])
            .current_dir(repo_path)
            .output()
            .unwrap();

        let repo = GitRepo::new(repo_path);

        // Create a path with 260+ characters
        let long_name = "a".repeat(50);
        let long_path = format!(
            "{}/{}/{}/{}/file.txt",
            long_name, long_name, long_name, long_name
        );

        // This might fail on older Windows without long path support
        let result = repo.add_file(&long_path, "content").await;
        // We don't assert success/failure as it depends on Windows config
        // but we test that it doesn't panic
        let _ = result;
    }

    // Additional error path tests

    #[tokio::test]
    async fn test_clone_permission_denied() {
        let temp_dir = TempDir::new().unwrap();
        let source_path = temp_dir.path().join("source");
        let target_path = temp_dir.path().join("target");

        // Create a local git repository to clone from
        std::fs::create_dir(&source_path).unwrap();
        Command::new("git")
            .args(["init", "--bare"])
            .current_dir(&source_path)
            .output()
            .unwrap();

        // Create the target directory and make it read-only
        std::fs::create_dir(&target_path).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&target_path).unwrap().permissions();
            perms.set_mode(0o444); // Read-only
            std::fs::set_permissions(&target_path, perms).unwrap();
        }

        let source_url = format!(
            "file://{}",
            source_path.display().to_string().replace('\\', "/")
        );
        let result = GitRepo::clone(&source_url, &target_path, None).await;

        // Clean up permissions before assertion
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&target_path).unwrap().permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&target_path, perms).unwrap();
        }

        // On Windows, the test might not fail due to different permission handling
        // On Unix, it should fail due to permission denied
        #[cfg(unix)]
        assert!(result.is_err());
        #[cfg(windows)]
        let _ = result; // Windows handles permissions differently
    }

    #[tokio::test]
    async fn test_clone_empty_url() {
        let temp_dir = TempDir::new().unwrap();
        let target_path = temp_dir.path().join("target");

        let result = GitRepo::clone("", &target_path, None).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Failed to clone"));
    }

    #[tokio::test]
    async fn test_fetch_local_repository() {
        let temp_dir = TempDir::new().unwrap();
        let repo_path = temp_dir.path().join("repo");
        let origin_path = temp_dir.path().join("origin");

        // Create origin repository
        std::fs::create_dir_all(&origin_path).unwrap();
        Command::new("git")
            .args(["init", "--bare"])
            .current_dir(&origin_path)
            .output()
            .unwrap();

        // Create repo and add the local origin
        std::fs::create_dir_all(&repo_path).unwrap();
        Command::new("git")
            .args(["init"])
            .current_dir(&repo_path)
            .output()
            .unwrap();

        // Add a file:// remote
        let origin_url = format!("file://{}", origin_path.display());
        Command::new("git")
            .args(["remote", "add", "origin", &origin_url])
            .current_dir(&repo_path)
            .output()
            .unwrap();

        let repo = GitRepo::new(&repo_path);
        let result = repo.fetch(None, None).await;

        // Should fetch successfully from local repositories
        assert!(result.is_ok(), "Fetch failed: {:?}", result.err());
    }

    #[tokio::test]
    async fn test_fetch_git_protocol() {
        let temp_dir = TempDir::new().unwrap();
        let repo_path = temp_dir.path();

        Command::new("git")
            .args(["init"])
            .current_dir(repo_path)
            .output()
            .unwrap();

        // Add a file:// remote (local repository)
        let bare_repo = temp_dir.path().join("bare");
        std::fs::create_dir(&bare_repo).unwrap();
        Command::new("git")
            .args(["init", "--bare"])
            .current_dir(&bare_repo)
            .output()
            .unwrap();

        Command::new("git")
            .args([
                "remote",
                "add",
                "origin",
                &format!("file://{}", bare_repo.display()),
            ])
            .current_dir(repo_path)
            .output()
            .unwrap();

        let repo = GitRepo::new(repo_path);
        let pb = crate::utils::progress::ProgressBar::new_spinner();
        let result = repo.fetch(None, Some(&pb)).await;

        // Should fetch for file:// repositories
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_fetch_with_auth_url() {
        let temp_dir = TempDir::new().unwrap();
        let bare_path = temp_dir.path().join("bare");
        let repo_path = temp_dir.path().join("repo");

        // Create bare repo
        std::fs::create_dir(&bare_path).unwrap();
        Command::new("git")
            .args(["init", "--bare"])
            .current_dir(&bare_path)
            .output()
            .unwrap();

        // Clone it
        let repo = GitRepo::clone(bare_path.to_str().unwrap(), &repo_path, None)
            .await
            .unwrap();

        // Fetch with specific auth URL
        let auth_url = format!("file://{}", bare_path.display());
        let result = repo.fetch(Some(&auth_url), None).await;
        assert!(result.is_ok(), "Fetch failed: {:?}", result.err());
    }

    #[tokio::test]
    async fn test_checkout_remote_branch() {
        let temp_dir = TempDir::new().unwrap();
        let bare_path = temp_dir.path().join("bare");
        let init_path = temp_dir.path().join("init");
        let clone_path = temp_dir.path().join("clone");

        // Create bare repo
        std::fs::create_dir(&bare_path).unwrap();
        Command::new("git")
            .args(["init", "--bare"])
            .current_dir(&bare_path)
            .output()
            .unwrap();

        // Create initial repo directory first
        std::fs::create_dir(&init_path).unwrap();

        // Create initial repo with branches
        Command::new("git")
            .args(["init"])
            .current_dir(&init_path)
            .output()
            .unwrap();

        Command::new("git")
            .args(["config", "user.email", "test@example.com"])
            .current_dir(&init_path)
            .output()
            .unwrap();

        Command::new("git")
            .args(["config", "user.name", "Test User"])
            .current_dir(&init_path)
            .output()
            .unwrap();

        std::fs::write(init_path.join("main.txt"), "main").unwrap();
        Command::new("git")
            .args(["add", "."])
            .current_dir(&init_path)
            .output()
            .unwrap();
        Command::new("git")
            .args(["commit", "-m", "Main commit"])
            .current_dir(&init_path)
            .output()
            .unwrap();

        // Create feature branch
        Command::new("git")
            .args(["checkout", "-b", "feature"])
            .current_dir(&init_path)
            .output()
            .unwrap();

        std::fs::write(init_path.join("feature.txt"), "feature").unwrap();
        Command::new("git")
            .args(["add", "."])
            .current_dir(&init_path)
            .output()
            .unwrap();
        Command::new("git")
            .args(["commit", "-m", "Feature commit"])
            .current_dir(&init_path)
            .output()
            .unwrap();

        // Push both branches
        Command::new("git")
            .args(["remote", "add", "origin", bare_path.to_str().unwrap()])
            .current_dir(&init_path)
            .output()
            .unwrap();

        Command::new("git")
            .args(["push", "origin", "--all"])
            .current_dir(&init_path)
            .output()
            .unwrap();

        // Clone and try to checkout remote branch
        let repo = GitRepo::clone(bare_path.to_str().unwrap(), &clone_path, None)
            .await
            .unwrap();

        // First checkout attempt should fail, second (with origin/) should succeed
        let result = repo.checkout("feature").await;
        assert!(result.is_ok());
        assert!(clone_path.join("feature.txt").exists());
    }

    #[tokio::test]
    async fn test_list_tags_non_git_directory() {
        let temp_dir = TempDir::new().unwrap();
        let non_git_path = temp_dir.path().join("not_git");
        std::fs::create_dir(&non_git_path).unwrap();

        let repo = GitRepo::new(&non_git_path);
        let result = repo.list_tags().await;

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Not a git repository"));
    }

    #[tokio::test]
    async fn test_list_tags_non_existent_directory() {
        let temp_dir = TempDir::new().unwrap();
        let non_existent = temp_dir.path().join("does_not_exist");

        let repo = GitRepo::new(&non_existent);
        let result = repo.list_tags().await;

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Repository path does not exist"));
    }

    #[tokio::test]
    async fn test_verify_url_file_protocol() {
        let temp_dir = TempDir::new().unwrap();
        let repo_path = temp_dir.path().join("repo");
        std::fs::create_dir(&repo_path).unwrap();

        // Test existing file:// URL
        let file_url = format!("file://{}", repo_path.display());
        let result = GitRepo::verify_url(&file_url).await;
        assert!(result.is_ok());

        // Test non-existent file:// URL
        let bad_file_url = "file:///non/existent/path";
        let result = GitRepo::verify_url(bad_file_url).await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Local path does not exist"));
    }

    #[tokio::test]
    async fn test_verify_url_remote() {
        // Test with invalid remote URL
        let result = GitRepo::verify_url("https://invalid-host-9999.test/repo.git").await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Failed to verify repository"));
    }

    #[test]
    fn test_strip_auth_from_url() {
        // Test HTTPS with authentication
        let url = "https://user:pass@github.com/owner/repo.git";
        let result = strip_auth_from_url(url).unwrap();
        assert_eq!(result, "https://github.com/owner/repo.git");

        // Test HTTPS with OAuth token
        let url = "https://oauth2:ghp_xxxx@github.com/owner/repo.git";
        let result = strip_auth_from_url(url).unwrap();
        assert_eq!(result, "https://github.com/owner/repo.git");

        // Test HTTP with authentication
        let url = "http://user:pass@example.com/repo.git";
        let result = strip_auth_from_url(url).unwrap();
        assert_eq!(result, "http://example.com/repo.git");

        // Test URL without authentication
        let url = "https://github.com/owner/repo.git";
        let result = strip_auth_from_url(url).unwrap();
        assert_eq!(result, "https://github.com/owner/repo.git");

        // Test SSH URL (should remain unchanged)
        let url = "git@github.com:owner/repo.git";
        let result = strip_auth_from_url(url).unwrap();
        assert_eq!(result, "git@github.com:owner/repo.git");

        // Test URL with @ in the path (not auth)
        let url = "https://example.com/user@domain/repo.git";
        let result = strip_auth_from_url(url).unwrap();
        assert_eq!(result, "https://example.com/user@domain/repo.git");
    }

    #[test]
    fn test_parse_git_url_local_paths() {
        let result = parse_git_url("/absolute/path/to/repo").unwrap();
        assert_eq!(result.0, "local");
        assert_eq!(result.1, "repo");

        let result = parse_git_url("./relative/path/repo.git").unwrap();
        assert_eq!(result.0, "local");
        assert_eq!(result.1, "repo");

        let result = parse_git_url("../parent/repo").unwrap();
        assert_eq!(result.0, "local");
        assert_eq!(result.1, "repo");

        // Test path without slashes - this is not a valid URL format
        // The parse_git_url function expects URLs or paths with at least one slash
        let result = parse_git_url("repo.git");
        assert!(result.is_err());
    }

    #[test]
    fn test_ensure_git_available() {
        // This should work on any system with git installed
        let result = ensure_git_available();
        assert!(result.is_ok());
    }

    #[test]
    fn test_ensure_valid_git_repo() {
        let temp_dir = TempDir::new().unwrap();
        let repo_path = temp_dir.path();

        // Test with non-git directory
        let result = ensure_valid_git_repo(repo_path);
        assert!(result.is_err());
        // The error message format changed - check for any git repo related error
        let err_str = result.unwrap_err().to_string();
        assert!(err_str.contains("git repository") || err_str.contains("Git repository"));

        // Initialize git repo
        Command::new("git")
            .args(["init"])
            .current_dir(repo_path)
            .output()
            .unwrap();

        // Test with valid git directory
        let result = ensure_valid_git_repo(repo_path);
        assert!(result.is_ok());
    }

    #[test]
    fn test_is_valid_git_repo() {
        let temp_dir = TempDir::new().unwrap();
        let repo_path = temp_dir.path();

        assert!(!is_valid_git_repo(repo_path));

        Command::new("git")
            .args(["init"])
            .current_dir(repo_path)
            .output()
            .unwrap();

        assert!(is_valid_git_repo(repo_path));
    }

    #[tokio::test]
    async fn test_checkout_reset_error_handling() {
        let temp_dir = TempDir::new().unwrap();
        let repo_path = temp_dir.path();

        // Initialize repo
        Command::new("git")
            .args(["init"])
            .current_dir(repo_path)
            .output()
            .unwrap();

        Command::new("git")
            .args(["config", "user.email", "test@example.com"])
            .current_dir(repo_path)
            .output()
            .unwrap();

        Command::new("git")
            .args(["config", "user.name", "Test User"])
            .current_dir(repo_path)
            .output()
            .unwrap();

        // Create initial commit
        std::fs::write(repo_path.join("file.txt"), "content").unwrap();
        Command::new("git")
            .args(["add", "."])
            .current_dir(repo_path)
            .output()
            .unwrap();
        Command::new("git")
            .args(["commit", "-m", "Initial"])
            .current_dir(repo_path)
            .output()
            .unwrap();

        // Create a tag
        Command::new("git")
            .args(["tag", "v1.0.0"])
            .current_dir(repo_path)
            .output()
            .unwrap();

        let repo = GitRepo::new(repo_path);

        // Checkout tag (will do reset first)
        let result = repo.checkout("v1.0.0").await;
        assert!(result.is_ok());

        // Try to checkout non-existent ref
        let result = repo.checkout("non-existent").await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("Failed to checkout"));
    }

    #[tokio::test]
    async fn test_get_remote_url_stderr() {
        let temp_dir = TempDir::new().unwrap();
        let non_git_path = temp_dir.path().join("not_git");
        std::fs::create_dir(&non_git_path).unwrap();

        let repo = GitRepo::new(&non_git_path);
        let result = repo.get_remote_url().await;

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Git operation failed"));
    }

    #[tokio::test]
    async fn test_concurrent_git_operations_same_repo() {
        let temp_dir = TempDir::new().unwrap();
        let repo_path = temp_dir.path();

        // Initialize repo
        Command::new("git")
            .args(["init"])
            .current_dir(repo_path)
            .output()
            .unwrap();

        Command::new("git")
            .args(["config", "user.email", "test@example.com"])
            .current_dir(repo_path)
            .output()
            .unwrap();

        Command::new("git")
            .args(["config", "user.name", "Test User"])
            .current_dir(repo_path)
            .output()
            .unwrap();

        // Create some commits and tags
        for i in 0..3 {
            let file_name = format!("file{}.txt", i);
            std::fs::write(repo_path.join(&file_name), format!("content{}", i)).unwrap();
            Command::new("git")
                .args(["add", "."])
                .current_dir(repo_path)
                .output()
                .unwrap();
            Command::new("git")
                .args(["commit", "-m", &format!("Commit {}", i)])
                .current_dir(repo_path)
                .output()
                .unwrap();
            Command::new("git")
                .args(["tag", &format!("v{}.0.0", i)])
                .current_dir(repo_path)
                .output()
                .unwrap();
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
        assert!(results.0.unwrap().is_ok());
        assert!(results.1.unwrap().is_ok());
        assert!(results.2.unwrap().is_ok());
    }
}
