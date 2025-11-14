//! Constraint resolver implementation for multi-dependency version resolution.

use anyhow::Result;
use semver::Version;
use std::collections::HashMap;

use super::{ConstraintSet, VersionConstraint};
use crate::core::AgpmError;

/// Manages version constraints for multiple dependencies and resolves them simultaneously.
///
/// `ConstraintResolver` coordinates version resolution across an entire dependency graph,
/// ensuring that all constraints are satisfied and conflicts are detected. It maintains
/// separate [`ConstraintSet`]s for each dependency and resolves them against available
/// version catalogs.
///
/// # Multi-Dependency Resolution
///
/// Unlike [`ConstraintSet`] which manages constraints for a single dependency,
/// `ConstraintResolver` handles multiple dependencies simultaneously:
///
/// - Each dependency gets its own constraint set
/// - Constraints can be added incrementally
/// - Resolution happens across the entire dependency graph
/// - Missing dependencies are detected and reported
///
/// # Resolution Process
///
/// 1. **Collect constraints**: Gather all constraints for each dependency
/// 2. **Validate availability**: Ensure versions exist for all dependencies
/// 3. **Apply constraint sets**: Use each dependency's constraints to filter versions
/// 4. **Select best matches**: Choose optimal versions for each dependency
/// 5. **Return resolution map**: Provide final version selections
///
/// # Examples
///
/// ## Basic Multi-Dependency Resolution
///
/// ```rust,no_run
/// use agpm_cli::version::constraints::ConstraintResolver;
/// use semver::Version;
/// use std::collections::HashMap;
///
/// let mut resolver = ConstraintResolver::new();
///
/// // Add constraints for multiple dependencies
/// resolver.add_constraint("dep1", "^1.0.0")?;
/// resolver.add_constraint("dep2", "~2.1.0")?;
/// resolver.add_constraint("dep3", "main")?;
///
/// // Provide available versions for each dependency
/// let mut available = HashMap::new();
/// available.insert("dep1".to_string(), vec![Version::parse("1.5.0")?]);
/// available.insert("dep2".to_string(), vec![Version::parse("2.1.3")?]);
/// available.insert("dep3".to_string(), vec![Version::parse("3.0.0")?]);
///
/// // Resolve all dependencies
/// let resolved = resolver.resolve(&available)?;
/// assert_eq!(resolved.len(), 3);
/// # Ok::<(), anyhow::Error>(())
/// ```
///
/// ## Incremental Constraint Addition
///
/// ```rust,no_run
/// use agpm_cli::version::constraints::ConstraintResolver;
///
/// let mut resolver = ConstraintResolver::new();
///
/// // Add multiple constraints for the same dependency
/// resolver.add_constraint("my-dep", ">=1.0.0")?;
/// resolver.add_constraint("my-dep", "<2.0.0")?;
/// resolver.add_constraint("my-dep", "^1.5.0")?;
///
/// // All constraints will be combined into a single constraint set
/// # Ok::<(), anyhow::Error>(())
/// ```
///
/// # Error Conditions
///
/// The resolver reports several types of errors:
///
/// - **Missing dependencies**: A constraint exists but no versions are available
/// - **Unsatisfiable constraints**: No available version meets all requirements
/// - **Conflicting constraints**: Impossible constraint combinations
///
/// # Use Cases
///
/// This resolver is particularly useful for:
/// - Package managers resolving dependency graphs
/// - Build systems selecting compatible versions
/// - Configuration management ensuring consistent environments
/// - Update analysis determining safe upgrade paths
pub struct ConstraintResolver {
    constraints: HashMap<String, ConstraintSet>,
}

impl Default for ConstraintResolver {
    fn default() -> Self {
        Self::new()
    }
}

impl ConstraintResolver {
    /// Creates a new constraint resolver
    ///
    /// # Returns
    ///
    /// Returns a new `ConstraintResolver` with empty constraint and resolution maps
    #[must_use]
    pub fn new() -> Self {
        Self {
            constraints: HashMap::new(),
        }
    }

    /// Add a version constraint for a specific dependency.
    ///
    /// This method parses constraint string and adds it to the constraint set
    /// for the named dependency. If this is the first constraint for the dependency,
    /// a new constraint set is created. Multiple constraints for the same dependency
    /// are combined into a single set with conflict detection.
    ///
    /// # Arguments
    ///
    /// * `dependency` - The name of the dependency to constrain
    /// * `constraint` - The constraint string to parse and add (e.g., "^1.0.0", "latest")
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` if the constraint was added successfully, or `Err` if:
    /// - The constraint string is invalid
    /// - The constraint conflicts with existing constraints for this dependency
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use agpm_cli::version::constraints::ConstraintResolver;
    ///
    /// let mut resolver = ConstraintResolver::new();
    ///
    /// // Add constraints for different dependencies
    /// resolver.add_constraint("web-framework", "^2.0.0")?;
    /// resolver.add_constraint("database", "~1.5.0")?;
    /// resolver.add_constraint("auth-lib", "main")?;
    ///
    /// // Add multiple constraints for the same dependency
    /// resolver.add_constraint("api-client", ">=1.0.0")?;
    /// resolver.add_constraint("api-client", "<2.0.0")?; // Compatible range
    ///
    /// // This would fail - conflicting exact versions
    /// resolver.add_constraint("my-dep", "1.0.0")?;
    /// let result = resolver.add_constraint("my-dep", "2.0.0");
    /// assert!(result.is_err());
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    ///
    /// # Constraint Combination
    ///
    /// When multiple constraints are added for the same dependency, they are
    /// combined using AND logic. The final constraint set requires that all
    /// individual constraints be satisfied simultaneously.
    pub fn add_constraint(&mut self, dependency: &str, constraint: &str) -> Result<()> {
        let parsed = VersionConstraint::parse(constraint)?;

        self.constraints.entry(dependency.to_string()).or_default().add(parsed)?;

        Ok(())
    }

    /// Resolve all dependency constraints and return the best version for each.
    ///
    /// This method performs the core resolution algorithm, taking all accumulated
    /// constraints and finding the best matching version for each dependency from
    /// the provided catalog of available versions.
    ///
    /// # Resolution Algorithm
    ///
    /// For each dependency with constraints:
    /// 1. **Verify availability**: Check that versions exist for the dependency
    /// 2. **Apply constraints**: Filter versions using the dependency's constraint set
    /// 3. **Select best match**: Choose the highest compatible version
    /// 4. **Handle prereleases**: Apply prerelease policies appropriately
    ///
    /// # Arguments
    ///
    /// * `available_versions` - Map from dependency names to lists of available versions
    ///
    /// # Returns
    ///
    /// Returns `Ok(HashMap<String, Version>)` with the resolved version for each
    /// dependency, or `Err` if resolution fails.
    ///
    /// # Error Conditions
    ///
    /// - **Missing dependency**: Constraint exists but no versions are available
    /// - **No satisfying version**: Available versions don't meet constraints
    /// - **Internal errors**: Constraint set conflicts or parsing failures
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use agpm_cli::version::constraints::ConstraintResolver;
    /// use semver::Version;
    /// use std::collections::HashMap;
    ///
    /// let mut resolver = ConstraintResolver::new();
    /// resolver.add_constraint("web-server", "^1.0.0")?;
    /// resolver.add_constraint("database", "~2.1.0")?;
    ///
    /// // Provide version catalog
    /// let mut available = HashMap::new();
    /// available.insert(
    ///     "web-server".to_string(),
    ///     vec![
    ///         Version::parse("1.0.0")?,
    ///         Version::parse("1.2.0")?,
    ///         Version::parse("1.5.0")?, // Best match for ^1.0.0
    ///         Version::parse("2.0.0")?, // Too new
    ///     ],
    /// );
    /// available.insert(
    ///     "database".to_string(),
    ///     vec![
    ///         Version::parse("2.1.0")?,
    ///         Version::parse("2.1.3")?, // Best match for ~2.1.0
    ///         Version::parse("2.2.0")?, // Too new
    ///     ],
    /// );
    ///
    /// // Resolve dependencies
    /// let resolved = resolver.resolve(&available)?;
    /// assert_eq!(resolved["web-server"], Version::parse("1.5.0")?);
    /// assert_eq!(resolved["database"], Version::parse("2.1.3")?);
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    ///
    /// ## Error Handling
    ///
    /// ```rust,no_run
    /// use agpm_cli::version::constraints::ConstraintResolver;
    /// use std::collections::HashMap;
    ///
    /// let mut resolver = ConstraintResolver::new();
    /// resolver.add_constraint("missing-dep", "^1.0.0")?;
    ///
    /// let available = HashMap::new(); // No versions provided
    ///
    /// let result = resolver.resolve(&available);
    /// assert!(result.is_err()); // Missing dependency error
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    ///
    /// # Performance Considerations
    ///
    /// - Resolution is performed independently for each dependency
    /// - Version filtering and sorting may be expensive for large version lists
    /// - Consider pre-filtering available versions if catalogs are very large
    pub fn resolve(
        &self,
        available_versions: &HashMap<String, Vec<Version>>,
    ) -> Result<HashMap<String, Version>> {
        let mut resolved = HashMap::new();

        for (dep, constraint_set) in &self.constraints {
            let versions = available_versions.get(dep).ok_or_else(|| AgpmError::Other {
                message: format!("No versions available for dependency: {dep}"),
            })?;

            let best_match =
                constraint_set.find_best_match(versions).ok_or_else(|| AgpmError::Other {
                    message: format!("No version satisfies constraints for dependency: {dep}"),
                })?;

            resolved.insert(dep.clone(), best_match.clone());
        }

        Ok(resolved)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_constraint_resolver() {
        let mut resolver = ConstraintResolver::new();

        resolver.add_constraint("dep1", "^1.0.0").unwrap();
        resolver.add_constraint("dep2", "~2.1.0").unwrap();

        let mut available = HashMap::new();
        available.insert(
            "dep1".to_string(),
            vec![
                Version::parse("0.9.0").unwrap(),
                Version::parse("1.0.0").unwrap(),
                Version::parse("1.5.0").unwrap(),
                Version::parse("2.0.0").unwrap(),
            ],
        );
        available.insert(
            "dep2".to_string(),
            vec![
                Version::parse("2.0.0").unwrap(),
                Version::parse("2.1.0").unwrap(),
                Version::parse("2.1.5").unwrap(),
                Version::parse("2.2.0").unwrap(),
            ],
        );

        let resolved = resolver.resolve(&available).unwrap();
        assert_eq!(resolved.get("dep1"), Some(&Version::parse("1.5.0").unwrap()));
        assert_eq!(resolved.get("dep2"), Some(&Version::parse("2.1.5").unwrap()));
    }

    #[test]
    fn test_constraint_resolver_missing_dependency() {
        let mut resolver = ConstraintResolver::new();
        resolver.add_constraint("dep1", "^1.0.0").unwrap();

        let available = HashMap::new(); // No versions available

        let result = resolver.resolve(&available);
        assert!(result.is_err());
    }

    #[test]
    fn test_constraint_resolver_no_satisfying_version() {
        let mut resolver = ConstraintResolver::new();
        resolver.add_constraint("dep1", "^2.0.0").unwrap();

        let mut available = HashMap::new();
        available.insert(
            "dep1".to_string(),
            vec![Version::parse("1.0.0").unwrap()], // Only 1.x available, but we need 2.x
        );

        let result = resolver.resolve(&available);
        assert!(result.is_err());
    }

    #[test]
    fn test_constraint_resolver_add_constraint_error() {
        let mut resolver = ConstraintResolver::new();

        // Add a valid constraint first
        resolver.add_constraint("dep1", "1.0.0").unwrap();

        // Add conflicting constraint
        let result = resolver.add_constraint("dep1", "2.0.0");
        assert!(result.is_err());
    }
}
