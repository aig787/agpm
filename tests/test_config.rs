use std::sync::Once;

static INIT: Once = Once::new();

/// Initialize test environment once for all tests
/// This ensures tests can run in parallel without interference
///
/// NOTE: This function uses `std::env::set_var` in a `Once::call_once` block,
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
