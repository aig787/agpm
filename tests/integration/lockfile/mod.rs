//! Lockfile management and determinism tests
//!
//! Tests for lockfile functionality:
//! - Staleness detection
//! - Stability across operations
//! - Deterministic generation
//! - Checksum computation and validation
//! - Migration from older lockfile formats

mod checksums;
mod determinism;
mod migration;
mod stability;
mod staleness;
