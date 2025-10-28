//! Installation workflow tests
//!
//! Tests for resource installation and deployment:
//! - Basic installation workflows (formerly deploy.rs)
//! - Install field and content embedding
//! - Incremental dependency addition
//! - Multi-artifact installation
//! - Multi-resource management
//! - Artifact cleanup and removal

mod basic;
mod cleanup;
mod incremental_add;
mod install_field;
mod multi_artifact;
mod multi_resource;
