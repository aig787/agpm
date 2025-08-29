use ccpm::utils::progress::{create_progress_bar, spinner_with_message, ProgressBar};
use std::env;

// NOTE: These tests intentionally modify environment variables to test
// the CCPM_NO_PROGRESS functionality. This is safe because each test
// cleans up after itself by removing the environment variable.

#[test]
fn test_no_progress_flag_disables_progress() {
    ccpm::test_utils::init_test_logging();
    // Clear the environment variable first
    env::remove_var("CCPM_NO_PROGRESS");

    // Create progress bars without the flag
    let pb1 = ProgressBar::new(100);
    pb1.set_message("Test without flag");
    pb1.finish_and_clear();

    let spinner1 = spinner_with_message("Spinner without flag");
    spinner1.finish_and_clear();

    // Set the no_progress flag
    env::set_var("CCPM_NO_PROGRESS", "1");

    // Create progress bars with the flag
    let pb2 = ProgressBar::new(100);
    pb2.set_message("Test with flag");
    pb2.inc(50);
    pb2.finish_and_clear();

    let spinner2 = spinner_with_message("Spinner with flag");
    spinner2.finish_and_clear();

    // Test create_progress_bar helper
    let pb3 = create_progress_bar(Some(200));
    pb3.set_message("Helper with flag");
    pb3.finish_and_clear();

    // Clean up
    env::remove_var("CCPM_NO_PROGRESS");
}

#[test]
fn test_no_progress_with_thread_safe_progress() {
    ccpm::test_utils::init_test_logging();
    use ccpm::utils::progress::{create_thread_safe_progress, ThreadSafeProgressBar};

    // Clear the environment variable first
    env::remove_var("CCPM_NO_PROGRESS");

    // Create without flag
    let ts_pb1 = ThreadSafeProgressBar::new(50);
    ts_pb1.set_message("Thread-safe without flag");
    ts_pb1.finish_and_clear();

    // Set the flag
    env::set_var("CCPM_NO_PROGRESS", "1");

    // Create with flag
    let ts_pb2 = ThreadSafeProgressBar::new_spinner();
    ts_pb2.set_message("Thread-safe with flag");
    ts_pb2.inc(10);
    ts_pb2.finish_and_clear();

    // Test helper
    let ts_pb3 = create_thread_safe_progress(None);
    ts_pb3.set_message("Thread-safe helper with flag");
    ts_pb3.finish_and_clear();

    // Clean up
    env::remove_var("CCPM_NO_PROGRESS");
}

#[test]
fn test_no_progress_with_parallel_counter() {
    ccpm::test_utils::init_test_logging();
    use ccpm::utils::progress::ParallelProgressCounter;

    // Clear the environment variable first
    env::remove_var("CCPM_NO_PROGRESS");

    // Create with progress bar, no flag
    let counter1 = ParallelProgressCounter::new(5, true);
    let _ = counter1.increment();
    counter1.finish();

    // Set the flag
    env::set_var("CCPM_NO_PROGRESS", "1");

    // Create with progress bar, but flag is set
    let counter2 = ParallelProgressCounter::new(5, true);
    let _ = counter2.increment();
    let _ = counter2.increment();
    counter2.finish();

    // Clean up
    env::remove_var("CCPM_NO_PROGRESS");
}
