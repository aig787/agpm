# Clippy Configuration

## Enforce These Lints

```rust
#![warn(
    clippy::all,
    clippy::pedantic,
    clippy::nursery,
    clippy::cargo,
    missing_docs,
    missing_debug_implementations,
    missing_copy_implementations,
    trivial_casts,
    trivial_numeric_casts,
    unsafe_code,
    unstable_features,
    unused_import_braces,
    unused_qualifications,
)]
```

## Allow These When Appropriate

```rust
#![allow(
    clippy::module_name_repetitions,  // Common in Rust APIs
    clippy::must_use_candidate,       // Can be too noisy
)]
```
