use std::sync::Once;

static INIT: Once = Once::new();

/// Initialize test environment once for all tests
/// This ensures tests can run in parallel without interference
///
/// # Safety
/// This function uses `std::env::set_var` in a `Once::call_once` block,
/// which is safe because:
/// - It only runs once at the start of the test suite
/// - These are test configuration variables, not being tested themselves
/// - The Once guard ensures no race conditions during initialization
/// - These environment variables configure the test environment globally
pub fn init_test_env() {
    INIT.call_once(|| {
        // Use the shared logging initialization
        agpm_cli::test_utils::init_test_logging(None);

        // Set test-specific environment variables
        unsafe {
            std::env::set_var("AGPM_TEST_MODE", "1");
            std::env::set_var("AGPM_NO_PROGRESS", "1"); // Disable progress bars in tests
            std::env::set_var("AGPM_PARALLEL_TESTS", "1");

            // Ensure consistent behavior across platforms
            std::env::set_var("AGPM_FORCE_COLOR", "0"); // Disable colors in test output

            // Set reasonable timeouts for tests
            std::env::set_var("AGPM_NETWORK_TIMEOUT", "30");
            std::env::set_var("AGPM_GIT_TIMEOUT", "30");
        }
    });
}
