//! Metadata extraction from resource files.
//!
//! This module provides functionality to extract dependency metadata from
//! resource files. It supports YAML frontmatter in Markdown files and
//! JSON fields in JSON configuration files.

pub mod extractor;

pub use extractor::MetadataExtractor;
