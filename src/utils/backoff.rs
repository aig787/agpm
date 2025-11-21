//! Exponential backoff utilities for retry operations.

use crate::constants::{MAX_BACKOFF_DELAY_MS, STARTING_BACKOFF_DELAY_MS};
use std::time::Duration;

/// Performs exponential backoff with delay.
///
/// Implements exponential backoff: 10ms, 20ms, 40ms... capped at 500ms
/// and sleeps for the calculated delay.
///
/// # Arguments
/// * `attempt` - Current retry attempt number (0-based)
///
/// # Returns
/// * `u32` - The next attempt number (incremented)
pub async fn exponential_backoff_with_delay(attempt: u32) -> u32 {
    // Exponential backoff: 10ms, 20ms, 40ms... capped at 500ms
    let delay = std::cmp::min(STARTING_BACKOFF_DELAY_MS * (1 << attempt), MAX_BACKOFF_DELAY_MS);
    tokio::time::sleep(Duration::from_millis(delay)).await;
    attempt.saturating_add(1)
}
