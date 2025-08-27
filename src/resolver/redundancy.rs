//! Redundancy detection and analysis for CCPM dependencies.
//!
//! This module provides sophisticated analysis of dependency redundancy patterns
//! to help users optimize their manifest files and understand resource usage.
//! Redundancy detection is designed to be advisory rather than blocking,
//! enabling legitimate use cases while highlighting optimization opportunities.
//!
//! # Types of Redundancy
//!
//! ## Version Redundancy
//! Multiple resources referencing the same source file with different versions:
//! ```toml
//! [agents]
//! app-helper = { source = "community", path = "agents/helper.md", version = "v1.0.0" }
//! tool-helper = { source = "community", path = "agents/helper.md", version = "v2.0.0" }
//! ```
//!
//! ## Mixed Constraint Redundancy  
//! Some dependencies use specific versions while others use latest:
//! ```toml
//! [agents]
//! main-agent = { source = "community", path = "agents/helper.md" } # latest
//! backup-agent = { source = "community", path = "agents/helper.md", version = "v1.0.0" }
//! ```
//!
//! ## Cross-Source Redundancy (Future)
//! Same resource available from multiple sources (not yet implemented):
//! ```toml
//! [sources]
//! official = "https://github.com/org/ccpm-official.git"
//! mirror = "https://github.com/org/ccpm-mirror.git"
//!
//! [agents]
//! helper1 = { source = "official", path = "agents/helper.md" }
//! helper2 = { source = "mirror", path = "agents/helper.md" }
//! ```
//!
//! # Algorithm Design
//!
//! The redundancy detection algorithm operates in O(n) time complexity:
//! 1. **Collection Phase**: Build usage map of source files → resources (O(n))
//! 2. **Analysis Phase**: Identify files with multiple version usages (O(n))
//! 3. **Classification Phase**: Categorize redundancy types (O(k) where k = redundancies)
//!
//! ## Data Structures
//!
//! The detector uses a hash map for efficient lookup:
//! ```text
//! usages: HashMap<String, Vec<ResourceUsage>>
//!         ↑                ↑
//!         source:path      list of resources using this file
//! ```
//!
//! # Design Principles
//!
//! ## Non-Blocking Detection
//! Redundancy analysis never prevents installation because:
//! - **A/B Testing**: Users may intentionally install multiple versions
//! - **Gradual Migration**: Transitioning between versions may require temporary redundancy
//! - **Testing Environments**: Different test scenarios may need different versions
//! - **Rollback Capability**: Keeping previous versions enables quick rollbacks
//!
//! ## Helpful Suggestions
//! Instead of blocking, the detector provides:
//! - **Version Alignment**: Suggest using consistent versions across resources
//! - **Consolidation Opportunities**: Identify resources that could share versions
//! - **Best Practices**: Guide users toward maintainable dependency patterns
//!
//! # Performance Considerations
//!
//! - **Lazy Evaluation**: Analysis only runs when explicitly requested
//! - **Memory Efficient**: Uses references where possible to avoid cloning
//! - **Early Termination**: Stops processing once redundancies are found (for boolean checks)
//! - **Batched Operations**: Groups related analysis operations together
//!
//! # Future Extensions
//!
//! Planned enhancements for redundancy detection:
//!
//! ## Transitive Analysis
//! When dependencies-of-dependencies are supported:
//! ```text
//! impl RedundancyDetector {
//!     pub fn check_transitive_redundancies(&self) -> Vec<Redundancy> {
//!         // Analyze entire dependency tree for redundant patterns
//!     }
//! }
//! ```
//!
//! ## Content-Based Detection
//! Hash-based redundancy detection for identical files:
//! ```rust,no_run
//! # use ccpm::resolver::redundancy::ResourceUsage;
//! pub struct ContentRedundancy {
//!     content_hash: String,
//!     identical_resources: Vec<ResourceUsage>,
//! }
//! ```
//!
//! ## Semantic Analysis
//! ML-based detection of functionally similar resources:
//! ```rust,no_run
//! # use ccpm::resolver::redundancy::ResourceUsage;
//! pub struct SemanticRedundancy {
//!     similarity_score: f64,
//!     similar_resources: Vec<ResourceUsage>,
//! }
//! ```

use crate::manifest::{Manifest, ResourceDependency};
use colored::Colorize;
use std::collections::{HashMap, HashSet};
use std::fmt;

/// Represents a specific usage of a source file by a resource dependency.
///
/// This struct captures how a particular resource (agent or snippet) uses
/// a source file, including version constraints and naming information.
/// It's the fundamental unit of redundancy analysis.
///
/// # Fields
///
/// - `resource_name`: The name given to this resource in the manifest
/// - `source_file`: Composite identifier in format "source:path"
/// - `version`: Version constraint (None means latest/default branch)
///
/// # Example
///
/// For this manifest entry:
/// ```toml
/// [agents]
/// my-helper = { source = "community", path = "agents/helper.md", version = "v1.2.3" }
/// ```
///
/// The corresponding `ResourceUsage` would be:
/// ```rust,no_run
/// # use ccpm::resolver::redundancy::ResourceUsage;
/// ResourceUsage {
///     resource_name: "my-helper".to_string(),
///     source_file: "community:agents/helper.md".to_string(),
///     version: Some("v1.2.3".to_string()),
/// }
/// # ;
/// ```
#[derive(Debug, Clone)]
pub struct ResourceUsage {
    /// The resource name that uses this source file
    pub resource_name: String,
    /// The source file being used (source:path)
    pub source_file: String,
    /// The version being used
    pub version: Option<String>,
}

impl fmt::Display for ResourceUsage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "'{}' uses version {}",
            self.resource_name,
            self.version.as_deref().unwrap_or("latest")
        )
    }
}

/// Represents a detected redundancy pattern where multiple resources use the same source file.
///
/// A [`Redundancy`] is created when the analysis detects that multiple resources
/// reference the same source file (identified by source:path) but with different
/// version constraints or names.
///
/// # Redundancy Criteria
///
/// A redundancy is detected when:
/// 1. **Multiple Usages**: More than one resource uses the same source file
/// 2. **Version Differences**: The usages specify different version constraints
///
/// Note: Multiple resources using the same source file with identical versions
/// are NOT considered redundant, as this is a valid use case.
///
/// # Use Cases for Legitimate Redundancy
///
/// - **A/B Testing**: Installing multiple versions for comparison
/// - **Migration Periods**: Gradually transitioning between versions
/// - **Rollback Preparation**: Keeping previous versions for quick rollback
/// - **Environment Differences**: Different versions for dev/staging/prod
///
/// # Display Format
///
/// When displayed, redundancies show:
/// ```text
/// ⚠ Multiple versions of 'community:agents/helper.md' will be installed:
///   - 'app-helper' uses version v1.0.0
///   - 'tool-helper' uses version v2.0.0
/// ```
#[derive(Debug)]
pub struct Redundancy {
    /// The source file that is used multiple times
    pub source_file: String,
    /// All usages of this source file
    pub usages: Vec<ResourceUsage>,
}

impl fmt::Display for Redundancy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            f,
            "{} Multiple versions of '{}' will be installed:",
            "⚠".yellow(),
            self.source_file
        )?;
        for usage in &self.usages {
            writeln!(f, "  - {usage}")?;
        }
        Ok(())
    }
}

/// Analyzes dependency patterns to detect and categorize redundancies.
///
/// The [`RedundancyDetector`] is the main analysis engine for identifying
/// optimization opportunities in dependency manifests. It builds a comprehensive
/// view of how resources use source files and identifies patterns that might
/// indicate redundancy.
///
/// # Analysis Process
///
/// 1. **Collection**: Gather all resource usages via [`add_usage()`] or [`analyze_manifest()`]
/// 2. **Detection**: Run [`detect_redundancies()`] to find redundant patterns
/// 3. **Reporting**: Generate warnings or suggestions using helper methods
///
/// # Thread Safety
///
/// The detector is not thread-safe due to mutable state during analysis.
/// Create separate instances for concurrent analysis operations.
///
/// # Memory Usage
///
/// The detector maintains an in-memory map of all resource usages. For large
/// manifests with hundreds of dependencies, memory usage scales linearly:
/// - Each resource usage: ~100 bytes (strings + metadata)
/// - `HashMap` overhead: ~25% of total usage data
///
/// [`add_usage()`]: RedundancyDetector::add_usage
/// [`analyze_manifest()`]: RedundancyDetector::analyze_manifest
/// [`detect_redundancies()`]: RedundancyDetector::detect_redundancies
pub struct RedundancyDetector {
    /// Map of source file identifiers to their usages
    usages: HashMap<String, Vec<ResourceUsage>>,
}

impl Default for RedundancyDetector {
    fn default() -> Self {
        Self::new()
    }
}

impl RedundancyDetector {
    /// Creates a new redundancy detector with empty state.
    ///
    /// The detector starts with no resource usage data. Use [`add_usage()`]
    /// for individual dependencies or [`analyze_manifest()`] for complete
    /// manifest analysis.
    ///
    /// # Example
    ///
    /// ```rust
    /// use ccpm::resolver::redundancy::RedundancyDetector;
    ///
    /// let mut detector = RedundancyDetector::new();
    /// // Add usages or analyze manifest...
    /// let redundancies = detector.detect_redundancies();
    /// ```
    ///
    /// [`add_usage()`]: RedundancyDetector::add_usage
    /// [`analyze_manifest()`]: RedundancyDetector::analyze_manifest
    #[must_use]
    pub fn new() -> Self {
        Self {
            usages: HashMap::new(),
        }
    }

    /// Records a resource usage for redundancy analysis.
    ///
    /// This method adds a single resource dependency to the analysis dataset.
    /// Local dependencies are automatically filtered out since they don't
    /// have redundancy concerns (each local path is unique).
    ///
    /// # Filtering Logic
    ///
    /// - **Remote Dependencies**: Added to analysis (have source + path)
    /// - **Local Dependencies**: Skipped (path-only, no redundancy issues)
    /// - **Invalid Dependencies**: Skipped (missing source information)
    ///
    /// # Source File Identification
    ///
    /// Remote dependencies are identified by their composite key:
    /// ```text
    /// source_file = "{source_name}:{resource_path}"
    /// ```
    ///
    /// This ensures that the same file from different sources is treated
    /// as separate resources (no cross-source redundancy detection yet).
    ///
    /// # Parameters
    ///
    /// - `resource_name`: Name assigned to this resource in the manifest
    /// - `dep`: Resource dependency specification from manifest
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use ccpm::resolver::redundancy::RedundancyDetector;
    /// use ccpm::manifest::{ResourceDependency, DetailedDependency};
    ///
    /// let mut detector = RedundancyDetector::new();
    ///
    /// // This will be recorded
    /// let remote_dep = ResourceDependency::Detailed(DetailedDependency {
    ///     source: Some("community".to_string()),
    ///     path: "agents/helper.md".to_string(),
    ///     version: Some("v1.0.0".to_string()),
    ///     branch: None,
    ///     rev: None,
    ///     command: None,
    ///     args: None,
    /// });
    /// detector.add_usage("my-helper".to_string(), &remote_dep);
    ///
    /// // This will be ignored (local dependency)
    /// let local_dep = ResourceDependency::Simple("../local/helper.md".to_string());
    /// detector.add_usage("local-helper".to_string(), &local_dep);
    /// ```
    pub fn add_usage(&mut self, resource_name: String, dep: &ResourceDependency) {
        // Only track remote dependencies (local ones don't have redundancy issues)
        if dep.is_local() {
            return;
        }

        if let Some(source) = dep.get_source() {
            let source_file = format!("{}:{}", source, dep.get_path());

            let usage = ResourceUsage {
                resource_name,
                source_file: source_file.clone(),
                version: dep.get_version().map(std::string::ToString::to_string),
            };

            self.usages.entry(source_file).or_default().push(usage);
        }
    }

    /// Analyzes all dependencies from a manifest for redundancy patterns.
    ///
    /// This is a convenience method that processes all dependencies from
    /// a manifest file in a single operation. It's equivalent to calling
    /// [`add_usage()`] for each dependency individually.
    ///
    /// # Processing Scope
    ///
    /// The method analyzes:
    /// - **Agent Dependencies**: From `[agents]` section
    /// - **Snippet Dependencies**: From `[snippets]` section
    /// - **Remote Dependencies**: Only those with source specifications
    ///
    /// Local dependencies are automatically filtered out during analysis.
    ///
    /// # Usage Pattern
    ///
    /// This method is typically used in the main resolution workflow:
    /// ```rust,no_run
    /// use ccpm::resolver::redundancy::RedundancyDetector;
    /// use ccpm::manifest::Manifest;
    /// use std::path::Path;
    ///
    /// # fn example() -> anyhow::Result<()> {
    /// let manifest = Manifest::load(Path::new("ccpm.toml"))?;
    /// let mut detector = RedundancyDetector::new();
    /// detector.analyze_manifest(&manifest);
    ///
    /// let redundancies = detector.detect_redundancies();
    /// let warning = detector.generate_redundancy_warning(&redundancies);
    /// if !warning.is_empty() {
    ///     eprintln!("{}", warning);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Performance
    ///
    /// - **Time Complexity**: O(n) where n = total dependencies
    /// - **Space Complexity**: O(r) where r = remote dependencies
    /// - **Memory Usage**: Linear with number of remote dependencies
    ///
    /// [`add_usage()`]: RedundancyDetector::add_usage
    pub fn analyze_manifest(&mut self, manifest: &Manifest) {
        for (name, dep) in manifest.all_dependencies() {
            self.add_usage(name.to_string(), dep);
        }
    }

    /// Detects redundancy patterns in the collected resource usages.
    ///
    /// This method analyzes all collected resource usages and identifies
    /// patterns where multiple resources use the same source file with
    /// different version constraints.
    ///
    /// # Detection Algorithm
    ///
    /// For each source file in the usage map:
    /// 1. **Skip Single Usage**: Files used by only one resource are not redundant
    /// 2. **Version Analysis**: Collect all unique version constraints for the file
    /// 3. **Redundancy Check**: If multiple different versions exist, mark as redundant
    ///
    /// # Redundancy Criteria
    ///
    /// A source file is considered redundant when:
    /// - **Multiple Resources**: More than one resource uses the file
    /// - **Different Versions**: Resources specify different version constraints
    ///
    /// # Non-Redundant Cases
    ///
    /// These cases are NOT considered redundant:
    /// - Single resource using a source file
    /// - Multiple resources using identical version constraints
    /// - Multiple resources all using "latest" (no version specified)
    ///
    /// # Algorithm Complexity
    ///
    /// - **Time**: O(n + k·m) where:
    ///   - n = total resource usages
    ///   - k = unique source files
    ///   - m = average usages per file
    /// - **Space**: O(r) where r = detected redundancies
    ///
    /// # Returns
    ///
    /// A vector of [`Redundancy`] objects, each representing a source file
    /// with redundant usage patterns. The vector is empty if no redundancies
    /// are detected.
    ///
    /// # Example Output
    ///
    /// For a manifest with redundant dependencies, this method might return:
    /// ```text
    /// [
    ///     Redundancy {
    ///         source_file: "community:agents/helper.md",
    ///         usages: [
    ///             ResourceUsage { resource_name: "app-helper", version: Some("v1.0.0") },
    ///             ResourceUsage { resource_name: "tool-helper", version: Some("v2.0.0") },
    ///         ]
    ///     }
    /// ]
    /// ```
    ///
    /// [`Redundancy`]: Redundancy
    #[must_use]
    pub fn detect_redundancies(&self) -> Vec<Redundancy> {
        let mut redundancies = Vec::new();

        for (source_file, uses) in &self.usages {
            // Skip if only one resource uses this source file
            if uses.len() <= 1 {
                continue;
            }

            // Collect unique versions
            let versions: HashSet<Option<String>> =
                uses.iter().map(|u| u.version.clone()).collect();

            // If there are different versions, it's a redundancy worth noting
            if versions.len() > 1 {
                redundancies.push(Redundancy {
                    source_file: source_file.clone(),
                    usages: uses.clone(),
                });
            }
        }

        redundancies
    }

    /// Determines if a redundancy could be consolidated to use a single version.
    ///
    /// This method analyzes a detected redundancy to determine if all resources
    /// using the source file could reasonably be updated to use the same version.
    /// This is a heuristic for suggesting consolidation opportunities.
    ///
    /// # Consolidation Logic
    ///
    /// A redundancy can be consolidated if:
    /// - All resources use the same version constraint (already consolidated)
    /// - All resources use "latest" (no specific versions)
    ///
    /// A redundancy cannot be easily consolidated if:
    /// - Resources use different specific versions (may have compatibility reasons)
    /// - Mixed latest and specific versions (may indicate intentional pinning)
    ///
    /// # Use Cases
    ///
    /// This method helps identify:
    /// - **Easy Wins**: Redundancies that could be quickly resolved
    /// - **Complex Cases**: Redundancies that may require careful consideration
    /// - **Intentional Patterns**: Cases where redundancy might be deliberate
    ///
    /// # Parameters
    ///
    /// - `redundancy`: The redundancy pattern to analyze
    ///
    /// # Returns
    ///
    /// - `true`: All usages could likely be consolidated to a single version
    /// - `false`: Consolidation would require careful analysis of compatibility
    ///
    /// # Example
    ///
    /// ```rust
    /// use ccpm::resolver::redundancy::{RedundancyDetector, Redundancy, ResourceUsage};
    ///
    /// let detector = RedundancyDetector::new();
    ///
    /// // Easy to consolidate (all use latest)
    /// let easy_redundancy = Redundancy {
    ///     source_file: "community:agents/helper.md".to_string(),
    ///     usages: vec![
    ///         ResourceUsage { resource_name: "helper1".to_string(), source_file: "community:agents/helper.md".to_string(), version: None },
    ///         ResourceUsage { resource_name: "helper2".to_string(), source_file: "community:agents/helper.md".to_string(), version: None },
    ///     ]
    /// };
    /// assert!(detector.can_consolidate(&easy_redundancy));
    ///
    /// // Hard to consolidate (different versions)
    /// let hard_redundancy = Redundancy {
    ///     source_file: "community:agents/helper.md".to_string(),
    ///     usages: vec![
    ///         ResourceUsage { resource_name: "helper1".to_string(), source_file: "community:agents/helper.md".to_string(), version: Some("v1.0.0".to_string()) },
    ///         ResourceUsage { resource_name: "helper2".to_string(), source_file: "community:agents/helper.md".to_string(), version: Some("v2.0.0".to_string()) },
    ///     ]
    /// };
    /// assert!(!detector.can_consolidate(&hard_redundancy));
    /// ```
    #[must_use]
    pub fn can_consolidate(&self, redundancy: &Redundancy) -> bool {
        // If all usages want the same version or latest, they could be consolidated
        let versions: HashSet<_> = redundancy.usages.iter().map(|u| &u.version).collect();
        versions.len() == 1
    }

    /// Generates a comprehensive warning message for detected redundancies.
    ///
    /// This method creates a user-friendly warning message that explains detected
    /// redundancies and provides actionable suggestions for optimization. The
    /// message is designed to be informative rather than alarming, emphasizing
    /// that redundancy is not an error.
    ///
    /// # Message Structure
    ///
    /// The generated warning includes:
    /// 1. **Header**: Clear indication this is a warning, not an error
    /// 2. **Redundancy List**: Each detected redundancy with details
    /// 3. **General Guidance**: Explanation of implications and options
    /// 4. **Specific Suggestions**: Targeted advice based on detected patterns
    ///
    /// # Message Tone
    ///
    /// The warning message maintains a helpful, non-blocking tone:
    /// - Emphasizes that installation will proceed normally
    /// - Explains that redundancy may be intentional
    /// - Provides optimization suggestions without mandating changes
    /// - Uses clear, jargon-free language
    ///
    /// # Color Coding
    ///
    /// The message uses terminal colors for better readability:
    /// - **Yellow**: Warning indicators and attention markers
    /// - **Blue**: Informational notes and suggestions
    /// - **Default**: Main content and resource names
    ///
    /// # Parameters
    ///
    /// - `redundancies`: List of detected redundancy patterns
    ///
    /// # Returns
    ///
    /// - **Non-empty**: Formatted warning message if redundancies exist
    /// - **Empty string**: If no redundancies provided
    ///
    /// # Example Output
    ///
    /// ```text
    /// Warning: Redundant dependencies detected
    ///
    /// ⚠ Multiple versions of 'community:agents/helper.md' will be installed:
    ///   - 'app-helper' uses version v1.0.0
    ///   - 'tool-helper' uses version latest
    ///
    /// Note: This is not an error, but you may want to consider:
    ///   • Using the same version for consistency
    ///   • These resources will be installed to different files
    ///   • Each will work independently
    ///   • Consider aligning versions for 'community:agents/helper.md' across all resources
    /// ```
    #[must_use]
    pub fn generate_redundancy_warning(&self, redundancies: &[Redundancy]) -> String {
        if redundancies.is_empty() {
            return String::new();
        }

        let mut message = format!(
            "\n{} Redundant dependencies detected\n\n",
            "Warning:".yellow().bold()
        );

        for redundancy in redundancies {
            message.push_str(&format!("{redundancy}\n"));
        }

        message.push_str(&format!(
            "\n{} This is not an error, but you may want to consider:\n",
            "Note:".blue()
        ));
        message.push_str("  • Using the same version for consistency\n");
        message.push_str("  • These resources will be installed to different files\n");
        message.push_str("  • Each will work independently\n");

        // Add specific suggestions based on redundancy patterns
        for redundancy in redundancies {
            let has_latest = redundancy.usages.iter().any(|u| u.version.is_none());
            let has_specific = redundancy.usages.iter().any(|u| u.version.is_some());

            if has_latest && has_specific {
                message.push_str(&format!(
                    "  • Consider aligning versions for '{}' across all resources\n",
                    redundancy.source_file
                ));
            }
        }

        message
    }

    /// Placeholder for future transitive redundancy detection.
    ///
    /// This method is reserved for future implementation when CCPM supports
    /// dependencies-of-dependencies (transitive dependencies). Currently returns
    /// an empty vector as transitive analysis is not yet implemented.
    ///
    /// # Planned Functionality
    ///
    /// When implemented, this method will:
    /// 1. **Build Dependency Tree**: Map entire transitive dependency graph
    /// 2. **Detect Deep Redundancy**: Find redundant patterns across dependency levels
    /// 3. **Analyze Impact**: Calculate storage and maintenance implications
    /// 4. **Suggest Optimizations**: Recommend dependency tree restructuring
    ///
    /// # Example Future Analysis
    ///
    /// ```text
    /// Direct:     app-agent → community:agents/helper.md v1.0.0
    /// Transitive: app-agent → tool-lib → community:agents/helper.md v2.0.0
    ///
    /// Result: Transitive redundancy detected - app-agent indirectly depends
    ///         on two versions of the same resource.
    /// ```
    ///
    /// # Implementation Challenges
    ///
    /// - **Circular Dependencies**: Detection and handling of cycles
    /// - **Version Compatibility**: Analyzing semantic version compatibility
    /// - **Performance**: Efficient analysis of large dependency trees
    /// - **Cache Management**: Handling cached vs. fresh transitive data
    ///
    /// # Returns
    ///
    /// Currently returns an empty vector. Future implementation will return
    /// detected transitive redundancies.
    #[must_use]
    pub fn check_transitive_redundancies(&self) -> Vec<Redundancy> {
        // TODO: When we add support for dependencies having their own dependencies,
        // we'll need to check for redundancies across the entire dependency tree
        Vec::new()
    }

    /// Generates actionable consolidation strategies for a specific redundancy.
    ///
    /// This method analyzes a detected redundancy pattern and provides specific,
    /// actionable suggestions for resolving or managing the redundancy. The
    /// suggestions are tailored to the specific pattern of version usage.
    ///
    /// # Strategy Categories
    ///
    /// ## Version Alignment
    /// For redundancies with multiple specific versions:
    /// - Suggest adopting a single version across all resources
    /// - Recommend the most recent or most commonly used version
    ///
    /// ## Constraint Standardization
    /// For mixed latest/specific version patterns:
    /// - Suggest using specific versions for reproducibility
    /// - Explain benefits of version pinning
    ///
    /// ## Impact Assessment
    /// For all redundancies:
    /// - Clarify that resources will be installed independently
    /// - Explain that each resource will function correctly
    /// - List all affected resource names
    ///
    /// # Suggestion Algorithm
    ///
    /// 1. **Analyze Version Pattern**: Identify specific vs. latest usage
    /// 2. **Generate Alignment Suggestions**: Recommend version standardization
    /// 3. **Provide Context**: Explain implications and benefits
    /// 4. **List Affected Resources**: Show impact scope
    ///
    /// # Parameters
    ///
    /// - `redundancy`: The redundancy pattern to analyze
    ///
    /// # Returns
    ///
    /// A vector of suggestion strings, ordered by priority:
    /// 1. Primary suggestions (version alignment)
    /// 2. Best practice recommendations (reproducibility)
    /// 3. Impact clarification (what will actually happen)
    ///
    /// # Example Output
    ///
    /// For a redundancy with mixed version constraints:
    /// ```text
    /// "Consider using version v2.0.0 for all resources using 'community:agents/helper.md'"
    /// "Consider using specific versions for all resources for reproducibility"
    /// "Note: Each resource (app-helper, tool-helper) will be installed independently"
    /// ```
    ///
    /// # Use Cases
    ///
    /// - **CLI Tools**: Generate help text for redundancy warnings
    /// - **IDE Extensions**: Provide quick-fix suggestions
    /// - **Automated Tools**: Implement dependency optimization utilities
    /// - **Documentation**: Generate project-specific optimization guides
    #[must_use]
    pub fn suggest_consolidation(&self, redundancy: &Redundancy) -> Vec<String> {
        let mut suggestions = Vec::new();

        // Collect all versions being used
        let versions: Vec<_> = redundancy
            .usages
            .iter()
            .filter_map(|u| u.version.as_ref())
            .collect();

        if !versions.is_empty() {
            // Suggest using the same version for consistency
            if let Some(version) = versions.first() {
                suggestions.push(format!(
                    "Consider using version {} for all resources using '{}'",
                    version, redundancy.source_file
                ));
            }
        }

        // If mixing latest and specific versions
        let has_latest = redundancy.usages.iter().any(|u| u.version.is_none());
        let has_specific = redundancy.usages.iter().any(|u| u.version.is_some());

        if has_latest && has_specific {
            suggestions.push(
                "Consider using specific versions for all resources for reproducibility"
                    .to_string(),
            );
        }

        // Explain that this isn't breaking
        suggestions.push(format!(
            "Note: Each resource ({}) will be installed independently",
            redundancy
                .usages
                .iter()
                .map(|u| &u.resource_name)
                .cloned()
                .collect::<Vec<_>>()
                .join(", ")
        ));

        suggestions
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::DetailedDependency;

    /// Tests basic redundancy detection with different versions of the same resource.
    ///
    /// This test verifies that the detector correctly identifies when multiple
    /// resources reference the same source file with different version constraints.
    #[test]
    fn test_detect_simple_redundancy() {
        let mut detector = RedundancyDetector::new();

        // Add resources using different versions of the same source file
        detector.add_usage(
            "app-agent".to_string(),
            &ResourceDependency::Detailed(DetailedDependency {
                source: Some("community".to_string()),
                path: "agents/shared.md".to_string(),
                version: Some("v1.0.0".to_string()),
                branch: None,
                rev: None,
                command: None,
                args: None,
            }),
        );

        detector.add_usage(
            "tool-agent".to_string(),
            &ResourceDependency::Detailed(DetailedDependency {
                source: Some("community".to_string()),
                path: "agents/shared.md".to_string(),
                version: Some("v2.0.0".to_string()),
                branch: None,
                rev: None,
                command: None,
                args: None,
            }),
        );

        let redundancies = detector.detect_redundancies();
        assert_eq!(redundancies.len(), 1);

        let redundancy = &redundancies[0];
        assert_eq!(redundancy.source_file, "community:agents/shared.md");
        assert_eq!(redundancy.usages.len(), 2);
    }

    /// Tests that resources using the same version are not flagged as redundant.
    ///
    /// This test ensures the detector doesn't generate false positives when
    /// multiple resources legitimately use the same source file and version.
    #[test]
    fn test_no_redundancy_same_version() {
        let mut detector = RedundancyDetector::new();

        // Add resources using the same version - not considered redundant
        detector.add_usage(
            "agent1".to_string(),
            &ResourceDependency::Detailed(DetailedDependency {
                source: Some("community".to_string()),
                path: "agents/shared.md".to_string(),
                version: Some("v1.0.0".to_string()),
                branch: None,
                rev: None,
                command: None,
                args: None,
            }),
        );

        detector.add_usage(
            "agent2".to_string(),
            &ResourceDependency::Detailed(DetailedDependency {
                source: Some("community".to_string()),
                path: "agents/shared.md".to_string(),
                version: Some("v1.0.0".to_string()),
                branch: None,
                rev: None,
                command: None,
                args: None,
            }),
        );

        let redundancies = detector.detect_redundancies();
        assert_eq!(redundancies.len(), 0);
    }

    /// Tests detection of mixed latest/specific version redundancy patterns.
    ///
    /// This test verifies that the detector identifies redundancy when some
    /// resources use latest (no version) while others specify explicit versions.
    #[test]
    fn test_redundancy_latest_vs_specific() {
        let mut detector = RedundancyDetector::new();

        // One wants latest, another wants specific version - this is redundant
        detector.add_usage(
            "agent1".to_string(),
            &ResourceDependency::Detailed(DetailedDependency {
                source: Some("community".to_string()),
                path: "agents/shared.md".to_string(),
                version: None, // latest
                branch: None,
                rev: None,
                command: None,
                args: None,
            }),
        );

        detector.add_usage(
            "agent2".to_string(),
            &ResourceDependency::Detailed(DetailedDependency {
                source: Some("community".to_string()),
                path: "agents/shared.md".to_string(),
                version: Some("v1.0.0".to_string()),
                branch: None,
                rev: None,
                command: None,
                args: None,
            }),
        );

        let redundancies = detector.detect_redundancies();
        assert_eq!(redundancies.len(), 1);
    }

    /// Tests that local dependencies are properly filtered out of redundancy analysis.
    ///
    /// This test ensures that local file dependencies don't participate in
    /// redundancy detection since they don't have the source/version complexity.
    #[test]
    fn test_local_dependencies_ignored() {
        let mut detector = RedundancyDetector::new();

        // Local dependencies are not tracked for redundancy
        detector.add_usage(
            "local1".to_string(),
            &ResourceDependency::Simple("../agents/agent1.md".to_string()),
        );

        detector.add_usage(
            "local2".to_string(),
            &ResourceDependency::Simple("../agents/agent2.md".to_string()),
        );

        let redundancies = detector.detect_redundancies();
        assert_eq!(redundancies.len(), 0);
    }

    /// Tests the generation of comprehensive warning messages for redundancies.
    ///
    /// This test verifies that the warning message generator produces appropriate
    /// content including resource names, versions, and helpful guidance.
    #[test]
    fn test_generate_redundancy_warning() {
        let mut detector = RedundancyDetector::new();

        detector.add_usage(
            "app".to_string(),
            &ResourceDependency::Detailed(DetailedDependency {
                source: Some("community".to_string()),
                path: "agents/shared.md".to_string(),
                version: Some("v1.0.0".to_string()),
                branch: None,
                rev: None,
                command: None,
                args: None,
            }),
        );

        detector.add_usage(
            "tool".to_string(),
            &ResourceDependency::Detailed(DetailedDependency {
                source: Some("community".to_string()),
                path: "agents/shared.md".to_string(),
                version: Some("v2.0.0".to_string()),
                branch: None,
                rev: None,
                command: None,
                args: None,
            }),
        );

        let redundancies = detector.detect_redundancies();
        let warning = detector.generate_redundancy_warning(&redundancies);

        assert!(warning.contains("Redundant dependencies detected"));
        assert!(warning.contains("app"));
        assert!(warning.contains("tool"));
        assert!(warning.contains("not an error"));
    }

    /// Tests the generation of consolidation suggestions for redundancy patterns.
    ///
    /// This test verifies that the suggestion generator produces actionable
    /// recommendations for resolving detected redundancy patterns.
    #[test]
    fn test_suggest_consolidation() {
        let mut detector = RedundancyDetector::new();

        detector.add_usage(
            "app".to_string(),
            &ResourceDependency::Detailed(DetailedDependency {
                source: Some("community".to_string()),
                path: "agents/shared.md".to_string(),
                version: None, // latest
                branch: None,
                rev: None,
                command: None,
                args: None,
            }),
        );

        detector.add_usage(
            "tool".to_string(),
            &ResourceDependency::Detailed(DetailedDependency {
                source: Some("community".to_string()),
                path: "agents/shared.md".to_string(),
                version: Some("v2.0.0".to_string()),
                branch: None,
                rev: None,
                command: None,
                args: None,
            }),
        );

        let redundancies = detector.detect_redundancies();
        let suggestions = detector.suggest_consolidation(&redundancies[0]);

        assert!(!suggestions.is_empty());
        assert!(suggestions.iter().any(|s| s.contains("v2.0.0")));
        assert!(suggestions.iter().any(|s| s.contains("independently")));
    }
}
