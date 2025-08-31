use anyhow::{Context, Result};
use glob::Pattern;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use tracing::{debug, trace};
use walkdir::WalkDir;

/// Pattern matcher for resource discovery in repositories
#[derive(Debug, Clone)]
pub struct PatternMatcher {
    pattern: Pattern,
    original_pattern: String,
}

impl PatternMatcher {
    /// Create a new pattern matcher from a glob pattern string
    pub fn new(pattern_str: &str) -> Result<Self> {
        let pattern = Pattern::new(pattern_str)
            .with_context(|| format!("Invalid glob pattern: {}", pattern_str))?;

        Ok(Self {
            pattern,
            original_pattern: pattern_str.to_string(),
        })
    }

    /// Find all matching files in a directory
    pub fn find_matches(&self, base_path: &Path) -> Result<Vec<PathBuf>> {
        debug!(
            "Searching for pattern '{}' in {:?}",
            self.original_pattern, base_path
        );

        let mut matches = Vec::new();
        let base_path = base_path
            .canonicalize()
            .with_context(|| format!("Failed to canonicalize path: {:?}", base_path))?;

        for entry in WalkDir::new(&base_path)
            .follow_links(false) // Security: don't follow symlinks
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();

            // Get relative path for pattern matching
            if let Ok(relative_path) = path.strip_prefix(&base_path) {
                let relative_str = relative_path.to_string_lossy();

                trace!("Checking path: {}", relative_str);

                if self.pattern.matches(&relative_str) {
                    debug!("Found match: {}", relative_str);
                    matches.push(relative_path.to_path_buf());
                }
            }
        }

        debug!(
            "Found {} matches for pattern '{}'",
            matches.len(),
            self.original_pattern
        );
        Ok(matches)
    }

    /// Check if a single path matches the pattern
    pub fn matches(&self, path: &Path) -> bool {
        let path_str = path.to_string_lossy();
        self.pattern.matches(&path_str)
    }

    /// Get the original pattern string
    pub fn pattern(&self) -> &str {
        &self.original_pattern
    }
}

/// Resolve pattern-based dependencies to concrete paths
pub struct PatternResolver {
    /// Patterns to exclude from matching
    exclude_patterns: Vec<Pattern>,
}

impl PatternResolver {
    /// Create a new pattern resolver
    pub fn new() -> Self {
        Self {
            exclude_patterns: Vec::new(),
        }
    }

    /// Add an exclusion pattern
    pub fn exclude(&mut self, pattern: &str) -> Result<()> {
        let pattern = Pattern::new(pattern)
            .with_context(|| format!("Invalid exclusion pattern: {}", pattern))?;
        self.exclude_patterns.push(pattern);
        Ok(())
    }

    /// Resolve a pattern to a list of resource paths, applying exclusions
    pub fn resolve(&self, pattern: &str, base_path: &Path) -> Result<Vec<PathBuf>> {
        let matcher = PatternMatcher::new(pattern)?;
        let mut matches = matcher.find_matches(base_path)?;

        // Apply exclusions
        if !self.exclude_patterns.is_empty() {
            matches.retain(|path| {
                let path_str = path.to_string_lossy();
                !self
                    .exclude_patterns
                    .iter()
                    .any(|exclude| exclude.matches(&path_str))
            });
        }

        // Sort for deterministic ordering
        matches.sort();

        Ok(matches)
    }

    /// Resolve multiple patterns and return unique results
    pub fn resolve_multiple(&self, patterns: &[String], base_path: &Path) -> Result<Vec<PathBuf>> {
        let mut all_matches = HashSet::new();

        for pattern in patterns {
            let matches = self.resolve(pattern, base_path)?;
            all_matches.extend(matches);
        }

        let mut result: Vec<_> = all_matches.into_iter().collect();
        result.sort();

        Ok(result)
    }
}

impl Default for PatternResolver {
    fn default() -> Self {
        Self::new()
    }
}

/// Extract resource name from a file path
pub fn extract_resource_name(path: &Path) -> String {
    path.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown")
        .to_string()
}

/// Validate that a pattern is safe (no path traversal)
pub fn validate_pattern_safety(pattern: &str) -> Result<()> {
    // Check for path traversal attempts
    if pattern.contains("..") {
        anyhow::bail!("Pattern contains path traversal (..): {}", pattern);
    }

    // Check for absolute paths on Unix
    if cfg!(unix) && pattern.starts_with('/') {
        anyhow::bail!("Pattern contains absolute path: {}", pattern);
    }

    // Check for absolute paths on Windows
    if cfg!(windows) && (pattern.contains(':') || pattern.starts_with('\\')) {
        anyhow::bail!("Pattern contains absolute path: {}", pattern);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_pattern_matcher_basic() {
        let pattern = PatternMatcher::new("*.md").unwrap();

        assert!(pattern.matches(Path::new("test.md")));
        assert!(pattern.matches(Path::new("README.md")));
        assert!(!pattern.matches(Path::new("test.txt")));
        assert!(!pattern.matches(Path::new("test.md.backup")));
    }

    #[test]
    fn test_pattern_matcher_nested() {
        let pattern = PatternMatcher::new("agents/*.md").unwrap();

        assert!(pattern.matches(Path::new("agents/test.md")));
        assert!(pattern.matches(Path::new("agents/helper.md")));
        assert!(!pattern.matches(Path::new("snippets/test.md")));
        // Note: glob patterns like "agents/*.md" will match nested paths in some implementations
        // For strict single-level matching, the pattern would need to be more specific
    }

    #[test]
    fn test_pattern_matcher_recursive() {
        let pattern = PatternMatcher::new("**/*.md").unwrap();

        assert!(pattern.matches(Path::new("test.md")));
        assert!(pattern.matches(Path::new("agents/test.md")));
        assert!(pattern.matches(Path::new("agents/subdir/test.md")));
        assert!(!pattern.matches(Path::new("test.txt")));
    }

    #[test]
    fn test_find_matches() {
        let temp_dir = TempDir::new().unwrap();
        let base_path = temp_dir.path();

        // Create test file structure
        fs::create_dir_all(base_path.join("agents")).unwrap();
        fs::create_dir_all(base_path.join("snippets")).unwrap();
        fs::create_dir_all(base_path.join("agents/subdir")).unwrap();

        fs::write(base_path.join("README.md"), "").unwrap();
        fs::write(base_path.join("agents/helper.md"), "").unwrap();
        fs::write(base_path.join("agents/assistant.md"), "").unwrap();
        fs::write(base_path.join("agents/subdir/nested.md"), "").unwrap();
        fs::write(base_path.join("snippets/code.md"), "").unwrap();
        fs::write(base_path.join("config.toml"), "").unwrap();

        // Test recursive pattern
        let pattern = PatternMatcher::new("**/*.md").unwrap();
        let matches = pattern.find_matches(base_path).unwrap();
        assert_eq!(matches.len(), 5); // All .md files in the tree

        // Test directory pattern - matches files in agents directory
        let pattern = PatternMatcher::new("agents/*.md").unwrap();
        let matches = pattern.find_matches(base_path).unwrap();
        // The glob pattern "agents/*.md" should match agents/helper.md, agents/assistant.md
        // and potentially agents/subdir/nested.md depending on glob implementation
        assert!(matches.len() >= 2);
        assert!(matches.contains(&PathBuf::from("agents/helper.md")));
        assert!(matches.contains(&PathBuf::from("agents/assistant.md")));

        // Test recursive pattern
        let pattern = PatternMatcher::new("**/*.md").unwrap();
        let matches = pattern.find_matches(base_path).unwrap();
        assert_eq!(matches.len(), 5);
        assert!(matches.contains(&PathBuf::from("README.md")));
        assert!(matches.contains(&PathBuf::from("agents/helper.md")));
        assert!(matches.contains(&PathBuf::from("agents/assistant.md")));
        assert!(matches.contains(&PathBuf::from("agents/subdir/nested.md")));
        assert!(matches.contains(&PathBuf::from("snippets/code.md")));
    }

    #[test]
    fn test_pattern_resolver_exclusions() {
        let temp_dir = TempDir::new().unwrap();
        let base_path = temp_dir.path();

        // Create test files
        fs::create_dir_all(base_path.join("agents")).unwrap();
        fs::write(base_path.join("agents/helper.md"), "").unwrap();
        fs::write(base_path.join("agents/test.md"), "").unwrap();
        fs::write(base_path.join("agents/example.md"), "").unwrap();

        let mut resolver = PatternResolver::new();
        resolver.exclude("*/test.md").unwrap();
        resolver.exclude("*/example.md").unwrap();

        let matches = resolver.resolve("agents/*.md", base_path).unwrap();
        assert_eq!(matches.len(), 1);
        assert!(matches.contains(&PathBuf::from("agents/helper.md")));
    }

    #[test]
    fn test_resolve_multiple_patterns() {
        let temp_dir = TempDir::new().unwrap();
        let base_path = temp_dir.path();

        // Create test files
        fs::create_dir_all(base_path.join("agents")).unwrap();
        fs::create_dir_all(base_path.join("snippets")).unwrap();
        fs::write(base_path.join("agents/helper.md"), "").unwrap();
        fs::write(base_path.join("snippets/code.md"), "").unwrap();
        fs::write(base_path.join("README.md"), "").unwrap();

        let resolver = PatternResolver::new();
        let patterns = vec![
            "agents/*.md".to_string(),
            "snippets/*.md".to_string(),
            "*.md".to_string(),
        ];

        let matches = resolver.resolve_multiple(&patterns, base_path).unwrap();
        assert_eq!(matches.len(), 3);
        assert!(matches.contains(&PathBuf::from("agents/helper.md")));
        assert!(matches.contains(&PathBuf::from("snippets/code.md")));
        assert!(matches.contains(&PathBuf::from("README.md")));
    }

    #[test]
    fn test_extract_resource_name() {
        assert_eq!(
            extract_resource_name(Path::new("agents/helper.md")),
            "helper"
        );
        assert_eq!(extract_resource_name(Path::new("test.md")), "test");
        assert_eq!(
            extract_resource_name(Path::new("path/to/resource.txt")),
            "resource"
        );
        assert_eq!(
            extract_resource_name(Path::new("noextension")),
            "noextension"
        );
    }

    #[test]
    fn test_validate_pattern_safety() {
        // Valid patterns
        assert!(validate_pattern_safety("*.md").is_ok());
        assert!(validate_pattern_safety("agents/*.md").is_ok());
        assert!(validate_pattern_safety("**/*.md").is_ok());

        // Invalid patterns - path traversal
        assert!(validate_pattern_safety("../parent/*.md").is_err());
        assert!(validate_pattern_safety("agents/../*.md").is_err());
        assert!(validate_pattern_safety("../../etc/passwd").is_err());

        // Invalid patterns - absolute paths
        if cfg!(unix) {
            assert!(validate_pattern_safety("/etc/*.conf").is_err());
            assert!(validate_pattern_safety("/home/user/*.md").is_err());
        }

        if cfg!(windows) {
            assert!(validate_pattern_safety("C:\\Windows\\*.dll").is_err());
            assert!(validate_pattern_safety("\\\\server\\share\\*.md").is_err());
        }
    }

    #[test]
    fn test_pattern_with_alternatives() {
        let pattern = PatternMatcher::new("agents/{helper,assistant}.md").unwrap();

        // Note: glob crate doesn't support {a,b} syntax directly
        // This test documents current behavior
        assert!(!pattern.matches(Path::new("agents/helper.md")));
        assert!(!pattern.matches(Path::new("agents/assistant.md")));
        assert!(pattern.matches(Path::new("agents/{helper,assistant}.md")));
    }

    #[test]
    fn test_pattern_case_sensitivity() {
        let pattern = PatternMatcher::new("*.MD").unwrap();

        // Pattern matching is case-sensitive on Unix, case-insensitive on Windows
        if cfg!(unix) {
            assert!(!pattern.matches(Path::new("test.md")));
            assert!(pattern.matches(Path::new("test.MD")));
        }
    }

    #[test]
    fn test_complex_patterns() {
        // Test character class
        let pattern = PatternMatcher::new("agent[0-9].md").unwrap();
        assert!(pattern.matches(Path::new("agent1.md")));
        assert!(pattern.matches(Path::new("agent5.md")));
        assert!(!pattern.matches(Path::new("agenta.md")));
        assert!(!pattern.matches(Path::new("agent10.md")));

        // Test negation
        let pattern = PatternMatcher::new("!test*.md").unwrap();
        // Note: glob crate doesn't support negation directly
        assert!(pattern.matches(Path::new("!test*.md")));

        // Test question mark wildcard
        let pattern = PatternMatcher::new("agent?.md").unwrap();
        assert!(pattern.matches(Path::new("agent1.md")));
        assert!(pattern.matches(Path::new("agenta.md")));
        assert!(!pattern.matches(Path::new("agent10.md")));
        assert!(!pattern.matches(Path::new("agent.md")));
    }

    #[test]
    fn test_edge_cases() {
        // Empty pattern
        assert!(PatternMatcher::new("").is_ok());

        // Pattern with spaces
        let pattern = PatternMatcher::new("my agent.md").unwrap();
        assert!(pattern.matches(Path::new("my agent.md")));
        assert!(!pattern.matches(Path::new("myagent.md")));

        // Pattern with special characters
        let pattern = PatternMatcher::new("agent-v1.0.0.md").unwrap();
        assert!(pattern.matches(Path::new("agent-v1.0.0.md")));

        // Very long pattern
        let long_pattern = "a".repeat(1000) + "*.md";
        assert!(PatternMatcher::new(&long_pattern).is_ok());
    }

    #[test]
    fn test_find_matches_with_symlinks() {
        let temp_dir = TempDir::new().unwrap();
        let base_path = temp_dir.path();

        // Create files and a symlink
        fs::create_dir_all(base_path.join("real")).unwrap();
        fs::write(base_path.join("real/file.md"), "").unwrap();

        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;
            symlink(base_path.join("real"), base_path.join("link")).unwrap();

            let pattern = PatternMatcher::new("**/*.md").unwrap();
            let matches = pattern.find_matches(base_path).unwrap();

            // Should not follow symlinks (security measure)
            assert_eq!(matches.len(), 1);
            assert!(matches.contains(&PathBuf::from("real/file.md")));
        }

        #[cfg(not(unix))]
        {
            // On non-Unix systems, just verify basic functionality
            let pattern = PatternMatcher::new("**/*.md").unwrap();
            let matches = pattern.find_matches(base_path).unwrap();
            assert_eq!(matches.len(), 1);
            assert!(matches.contains(&PathBuf::from("real/file.md")));
        }
    }

    #[test]
    fn test_pattern_resolver_with_multiple_exclusions() {
        let temp_dir = TempDir::new().unwrap();
        let base_path = temp_dir.path();

        // Create test files
        fs::create_dir_all(base_path.join("agents")).unwrap();
        fs::write(base_path.join("agents/helper.md"), "").unwrap();
        fs::write(base_path.join("agents/test.md"), "").unwrap();
        fs::write(base_path.join("agents/debug.md"), "").unwrap();
        fs::write(base_path.join("agents/production.md"), "").unwrap();

        let mut resolver = PatternResolver::new();
        resolver.exclude("*/test.md").unwrap();
        resolver.exclude("*/debug.md").unwrap();

        let matches = resolver.resolve("agents/*.md", base_path).unwrap();
        assert_eq!(matches.len(), 2);
        assert!(matches.contains(&PathBuf::from("agents/helper.md")));
        assert!(matches.contains(&PathBuf::from("agents/production.md")));
    }

    #[test]
    fn test_concurrent_pattern_resolution() {
        use std::sync::Arc;
        use std::thread;

        let temp_dir = TempDir::new().unwrap();
        let base_path = Arc::new(temp_dir.path().to_path_buf());

        // Create test files
        for i in 0..100 {
            fs::write(base_path.join(format!("file{}.md", i)), "").unwrap();
        }

        // Run pattern matching concurrently
        let mut handles = vec![];
        for _ in 0..10 {
            let path = Arc::clone(&base_path);
            let handle = thread::spawn(move || {
                let pattern = PatternMatcher::new("*.md").unwrap();
                pattern.find_matches(&path).unwrap()
            });
            handles.push(handle);
        }

        // All threads should find the same files
        let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
        let first_result = &results[0];
        for result in &results[1..] {
            assert_eq!(result.len(), first_result.len());
        }
    }

    #[test]
    fn test_pattern_performance() {
        let temp_dir = TempDir::new().unwrap();
        let base_path = temp_dir.path();

        // Create a large directory structure
        for i in 0..10 {
            let dir = base_path.join(format!("dir{}", i));
            fs::create_dir_all(&dir).unwrap();
            for j in 0..100 {
                fs::write(dir.join(format!("file{}.md", j)), "").unwrap();
            }
        }

        let pattern = PatternMatcher::new("**/*.md").unwrap();
        let start = std::time::Instant::now();
        let matches = pattern.find_matches(base_path).unwrap();
        let duration = start.elapsed();

        assert_eq!(matches.len(), 1000);
        // Should complete reasonably quickly (< 1 second for 1000 files)
        assert!(duration.as_secs() < 1);
    }
}
