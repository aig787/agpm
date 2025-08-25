use std::sync::Once;

static INIT: Once = Once::new();

/// Initialize test environment once for all tests
/// This ensures tests can run in parallel without interference
///
/// NOTE: This function uses std::env::set_var in a Once::call_once block,
/// which is safe because it only runs once at the start of the test suite.
/// These environment variables configure the test environment globally.
pub fn init_test_env() {
    INIT.call_once(|| {
        // Set up logging for tests
        if std::env::var("RUST_LOG").is_err() {
            std::env::set_var("RUST_LOG", "ccpm=debug");
        }

        // Initialize tracing subscriber for test output
        let _ = tracing_subscriber::fmt()
            .with_test_writer()
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .try_init();

        // Set test-specific environment variables
        std::env::set_var("CCPM_TEST_MODE", "1");
        std::env::set_var("CCPM_NO_PROGRESS", "1"); // Disable progress bars in tests
        std::env::set_var("CCPM_PARALLEL_TESTS", "1");

        // Ensure consistent behavior across platforms
        std::env::set_var("CCPM_FORCE_COLOR", "0"); // Disable colors in test output

        // Set reasonable timeouts for tests
        std::env::set_var("CCPM_NETWORK_TIMEOUT", "30");
        std::env::set_var("CCPM_GIT_TIMEOUT", "30");
    });
}

/// Test utilities for parallel execution
pub mod parallel {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

    /// Generate a unique test identifier for parallel test isolation
    pub fn unique_test_id() -> String {
        let counter = TEST_COUNTER.fetch_add(1, Ordering::SeqCst);
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        format!("test_{}_{}", counter, timestamp)
    }

    /// Create isolated test environment variables
    pub fn isolated_env_vars() -> Vec<(String, String)> {
        let test_id = unique_test_id();
        vec![
            ("CCPM_TEST_ID".to_string(), test_id.clone()),
            (
                "CCPM_CACHE_DIR".to_string(),
                format!("/tmp/ccpm_test_{}", test_id),
            ),
            (
                "CCPM_CONFIG_DIR".to_string(),
                format!("/tmp/ccpm_config_{}", test_id),
            ),
        ]
    }
}

/// Test assertion helpers
pub mod assertions {
    use predicates::prelude::*;

    /// Predicate for successful command execution
    pub fn success_with_output(expected: &str) -> impl Predicate<str> {
        predicate::str::contains(expected)
    }

    /// Predicate for failure with specific error
    pub fn failure_with_error(expected: &str) -> impl Predicate<str> {
        predicate::str::contains(expected)
    }

    /// Predicate for checking lockfile validity
    pub fn valid_lockfile() -> impl Predicate<str> {
        predicate::str::contains("version = 1")
            .and(predicate::str::contains("[[sources]]").or(predicate::str::contains("[[agents]]")))
    }

    /// Predicate for checking manifest validity
    pub fn valid_manifest() -> impl Predicate<str> {
        predicate::str::contains("[sources]")
            .and(predicate::str::contains("[agents]").or(predicate::str::contains("[snippets]")))
    }
}

/// Test data generators
pub mod generators {
    use std::collections::HashMap;

    /// Generate test manifest content with specified number of dependencies
    pub fn manifest_with_deps(num_agents: usize, num_snippets: usize) -> String {
        let mut content = String::from(
            r#"
[sources]
official = "https://github.com/example-org/ccpm-official.git"
community = "https://github.com/example-org/ccpm-community.git"

[agents]
"#,
        );

        for i in 0..num_agents {
            content.push_str(&format!(
                "agent_{} = {{ source = \"official\", path = \"agents/agent_{}.md\", version = \"v1.0.0\" }}\n",
                i, i
            ));
        }

        if num_snippets > 0 {
            content.push_str("\n[snippets]\n");
            for i in 0..num_snippets {
                content.push_str(&format!(
                    "snippet_{} = {{ source = \"community\", path = \"snippets/snippet_{}.md\", version = \"v1.0.0\" }}\n",
                    i, i
                ));
            }
        }

        content
    }

    /// Generate test lockfile content matching manifest
    pub fn lockfile_for_manifest(num_agents: usize, num_snippets: usize) -> String {
        let mut content = String::from(
            r#"
# Auto-generated lockfile - DO NOT EDIT
version = 1

[[sources]]
name = "official"
url = "https://github.com/example-org/ccpm-official.git"
commit = "abc123456789abcdef123456789abcdef12345678"
"#,
        );

        if num_snippets > 0 {
            content.push_str(
                r#"
[[sources]]
name = "community"
url = "https://github.com/example-org/ccpm-community.git"
commit = "def456789abcdef123456789abcdef123456789ab"
"#,
            );
        }

        for i in 0..num_agents {
            content.push_str(&format!(
                r#"
[[agents]]
name = "agent_{}"
source = "official"
path = "agents/agent_{}.md"
version = "v1.0.0"
resolved_commit = "abc123456789abcdef123456789abcdef12345678"
checksum = "sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
installed_at = "agents/agent_{}.md"
"#,
                i, i, i
            ));
        }

        for i in 0..num_snippets {
            content.push_str(&format!(
                r#"
[[snippets]]
name = "snippet_{}"
source = "community"
path = "snippets/snippet_{}.md"
version = "v1.0.0"
resolved_commit = "def456789abcdef123456789abcdef123456789ab"
checksum = "sha256:38b060a751ac96384cd9327eb1b1e36a21fdb71114be07434c0cc7bf63f6e1da"
installed_at = "snippets/snippet_{}.md"
"#,
                i, i, i
            ));
        }

        content
    }

    /// Generate version constraints for testing
    pub fn version_constraints() -> HashMap<&'static str, &'static str> {
        let mut constraints = HashMap::new();
        constraints.insert("exact", "=1.0.0");
        constraints.insert("caret", "^1.0.0");
        constraints.insert("tilde", "~1.0.0");
        constraints.insert("range", ">=1.0.0, <2.0.0");
        constraints.insert("latest", "latest");
        constraints.insert("wildcard", "*");
        constraints
    }
}

/// Platform-specific test utilities
pub mod platform {
    /// Check if running on Windows
    pub fn is_windows() -> bool {
        cfg!(windows)
    }

    /// Check if running on Unix (Linux/macOS)
    pub fn is_unix() -> bool {
        cfg!(unix)
    }

    /// Get platform-appropriate path separator
    pub fn path_separator() -> &'static str {
        if is_windows() {
            "\\"
        } else {
            "/"
        }
    }

    /// Get platform-appropriate executable extension
    pub fn exe_extension() -> &'static str {
        if is_windows() {
            ".exe"
        } else {
            ""
        }
    }

    /// Skip test if not on specified platform
    #[allow(unused_macros)]
    macro_rules! skip_unless_platform {
        (windows) => {
            if !platform::is_windows() {
                eprintln!("Skipping Windows-specific test");
                return;
            }
        };
        (unix) => {
            if !platform::is_unix() {
                eprintln!("Skipping Unix-specific test");
                return;
            }
        };
    }

    #[allow(unused_imports)]
    pub(crate) use skip_unless_platform;
}

/// Performance testing utilities
pub mod performance {
    use std::time::{Duration, Instant};

    /// Measure execution time of a function
    pub fn measure_time<F, R>(f: F) -> (R, Duration)
    where
        F: FnOnce() -> R,
    {
        let start = Instant::now();
        let result = f();
        let duration = start.elapsed();
        (result, duration)
    }

    /// Assert that operation completes within specified time
    pub fn assert_within_time<F>(f: F, max_duration: Duration)
    where
        F: FnOnce(),
    {
        let (_, duration) = measure_time(f);
        assert!(
            duration <= max_duration,
            "Operation took {:?}, expected <= {:?}",
            duration,
            max_duration
        );
    }

    /// Performance benchmarks for common operations
    pub struct Benchmarks;

    impl Benchmarks {
        pub const INSTALL_TIMEOUT: Duration = Duration::from_secs(30);
        pub const UPDATE_TIMEOUT: Duration = Duration::from_secs(20);
        pub const VALIDATE_TIMEOUT: Duration = Duration::from_secs(10);
        pub const LIST_TIMEOUT: Duration = Duration::from_secs(5);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_unique_test_ids() {
        let id1 = parallel::unique_test_id();
        let id2 = parallel::unique_test_id();
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_manifest_generation() {
        let manifest = generators::manifest_with_deps(2, 1);
        assert!(manifest.contains("agent_0"));
        assert!(manifest.contains("agent_1"));
        assert!(manifest.contains("snippet_0"));
    }

    #[test]
    fn test_platform_detection() {
        // These should work on any platform
        assert!(platform::is_windows() || platform::is_unix());
        assert!(!platform::path_separator().is_empty());
    }
}
