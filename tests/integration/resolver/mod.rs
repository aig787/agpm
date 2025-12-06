//! Integration tests for resolver module
//!
//! Tests resolver functionality including:
//! - Version resolution with caching
//! - Tag caching optimization
//! - Performance characteristics
//! - Error handling
//! - Branch and revision reference handling
//! - Transitive dependency version inheritance

pub mod branch_main_test;
pub mod glob_transitive_deps;
pub mod resource_service;
pub mod service_lifecycle;
pub mod tag_caching_tests;
pub mod transitive_main_conflict;
