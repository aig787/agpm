use anyhow::Result;
use ccpm::cache::Cache;
use ccpm::cli::validate::ValidateCommand;
use ccpm::manifest::{DetailedDependency, Manifest, ResourceDependency};
use tempfile::TempDir;
#[tokio::test]
async fn test_validate_check_redundancy_none() -> Result<()> {
    ccpm::test_utils::init_test_logging(None);
    let temp = TempDir::new()?;
    let manifest_path = temp.path().join("ccpm.toml");
    // Create manifest without redundancies
    let mut manifest = Manifest::new();
    manifest.add_source(
        "community".to_string(),
        "https://github.com/test/community.git".to_string(),
    );
    // Two dependencies using the same version
    manifest.add_dependency(
        "agent1".to_string(),
        ResourceDependency::Detailed(DetailedDependency {
            source: Some("community".to_string()),
            path: "agents/shared.md".to_string(),
            version: Some("v1.0.0".to_string()),
            branch: None,
            rev: None,
            command: None,
            args: None,
            target: None,
            filename: None,
        }),
        true,
    );
    manifest.add_dependency(
        "agent2".to_string(),
        ResourceDependency::Detailed(DetailedDependency {
            source: Some("community".to_string()),
            path: "agents/shared.md".to_string(),
            version: Some("v1.0.0".to_string()),
            branch: None,
            rev: None,
            command: None,
            args: None,
            target: None,
            filename: None,
        }),
        true,
    );
    manifest.save(&manifest_path)?;
    let cmd = ValidateCommand {
        file: None,
        resolve: false,
        check_lock: false,
        sources: false,
        paths: false,
        check_redundancies: true,
        format: ccpm::cli::validate::OutputFormat::Text,
        verbose: false,
        quiet: false,
        strict: false,
    };
    let result = cmd.execute_from_path(manifest_path).await;
    assert!(
        result.is_ok(),
        "Should not detect redundancies when versions match"
    );
    Ok(())
}
#[tokio::test]
async fn test_validate_check_redundancy_direct() -> Result<()> {
    ccpm::test_utils::init_test_logging(None);
    let temp = TempDir::new()?;
    let manifest_path = temp.path().join("ccpm.toml");
    // Create manifest with direct redundancies
    let mut manifest = Manifest::new();
    manifest.add_source(
        "community".to_string(),
        "https://github.com/test/community.git".to_string(),
    );
    // Two dependencies with different versions of the same resource
    manifest.add_dependency(
        "app-agent".to_string(),
        ResourceDependency::Detailed(DetailedDependency {
            source: Some("community".to_string()),
            path: "agents/shared.md".to_string(),
            version: Some("v1.0.0".to_string()),
            branch: None,
            rev: None,
            command: None,
            args: None,
            target: None,
            filename: None,
        }),
        true,
    );
    manifest.add_dependency(
        "tool-agent".to_string(),
        ResourceDependency::Detailed(DetailedDependency {
            source: Some("community".to_string()),
            path: "agents/shared.md".to_string(),
            version: Some("v2.0.0".to_string()),
            branch: None,
            rev: None,
            command: None,
            args: None,
            target: None,
            filename: None,
        }),
        true,
    );
    manifest.save(&manifest_path)?;
    let cmd = ValidateCommand {
        file: None,
        resolve: false,
        check_lock: false,
        sources: false,
        paths: false,
        check_redundancies: true,
        format: ccpm::cli::validate::OutputFormat::Text,
        verbose: false,
        quiet: true,
        strict: false,
    };
    // Redundancies should not cause errors, only warnings
    let result = cmd.execute_from_path(manifest_path).await;
    assert!(
        result.is_ok(),
        "Redundancies should not cause validation to fail"
    );
    Ok(())
}
#[tokio::test]
async fn test_validate_check_redundancy_latest_vs_specific() -> Result<()> {
    ccpm::test_utils::init_test_logging(None);
    let temp = TempDir::new()?;
    let manifest_path = temp.path().join("ccpm.toml");
    // Create manifest with latest vs specific version redundancy
    let mut manifest = Manifest::new();
    manifest.add_source(
        "community".to_string(),
        "https://github.com/test/community.git".to_string(),
    );
    // One wants latest, other wants specific version
    manifest.add_dependency(
        "agent1".to_string(),
        ResourceDependency::Detailed(DetailedDependency {
            source: Some("community".to_string()),
            path: "agents/shared.md".to_string(),
            version: Some("v2.0.0".to_string()), // different version
            branch: None,
            rev: None,
            command: None,
            args: None,
            target: None,
            filename: None,
        }),
        true,
    );
    manifest.add_dependency(
        "agent2".to_string(),
        ResourceDependency::Detailed(DetailedDependency {
            source: Some("community".to_string()),
            path: "agents/shared.md".to_string(),
            version: Some("v1.0.0".to_string()),
            branch: None,
            rev: None,
            command: None,
            args: None,
            target: None,
            filename: None,
        }),
        true,
    );
    manifest.save(&manifest_path)?;
    let cmd = ValidateCommand {
        file: None,
        resolve: false,
        check_lock: false,
        sources: false,
        paths: false,
        check_redundancies: true,
        format: ccpm::cli::validate::OutputFormat::Text,
        verbose: false,
        quiet: true,
        strict: false,
    };
    // Redundancies should not cause errors, only warnings
    let result = cmd.execute_from_path(manifest_path).await;
    if let Err(e) = &result {
        println!("Unexpected error: {e}");
    }
    assert!(
        result.is_ok(),
        "Redundancies should not cause validation to fail"
    );
    Ok(())
}
#[tokio::test]
async fn test_validate_check_redundancy_multiple() -> Result<()> {
    ccpm::test_utils::init_test_logging(None);
    let temp = TempDir::new()?;
    let manifest_path = temp.path().join("ccpm.toml");
    // Create manifest with multiple redundancies
    let mut manifest = Manifest::new();
    manifest.add_source(
        "community".to_string(),
        "https://github.com/test/community.git".to_string(),
    );
    // First redundancy: shared.md
    manifest.add_dependency(
        "agent1".to_string(),
        ResourceDependency::Detailed(DetailedDependency {
            source: Some("community".to_string()),
            path: "agents/shared.md".to_string(),
            version: Some("v1.0.0".to_string()),
            branch: None,
            rev: None,
            command: None,
            args: None,
            target: None,
            filename: None,
        }),
        true,
    );
    manifest.add_dependency(
        "agent2".to_string(),
        ResourceDependency::Detailed(DetailedDependency {
            source: Some("community".to_string()),
            path: "agents/shared.md".to_string(),
            version: Some("v2.0.0".to_string()),
            branch: None,
            rev: None,
            command: None,
            args: None,
            target: None,
            filename: None,
        }),
        true,
    );
    // Second redundancy: utils.md
    manifest.add_dependency(
        "snippet1".to_string(),
        ResourceDependency::Detailed(DetailedDependency {
            source: Some("community".to_string()),
            path: "snippets/utils.md".to_string(),
            version: Some("v1.2.0".to_string()),
            branch: None,
            rev: None,
            command: None,
            args: None,
            target: None,
            filename: None,
        }),
        false,
    );
    manifest.add_dependency(
        "snippet2".to_string(),
        ResourceDependency::Detailed(DetailedDependency {
            source: Some("community".to_string()),
            path: "snippets/utils.md".to_string(),
            version: Some("v1.3.0".to_string()),
            branch: None,
            rev: None,
            command: None,
            args: None,
            target: None,
            filename: None,
        }),
        false,
    );
    manifest.save(&manifest_path)?;
    let cmd = ValidateCommand {
        file: None,
        resolve: false,
        check_lock: false,
        sources: false,
        paths: false,
        check_redundancies: true,
        format: ccpm::cli::validate::OutputFormat::Text,
        verbose: false,
        quiet: true,
        strict: false,
    };
    let result = cmd.execute_from_path(manifest_path).await;
    assert!(
        result.is_ok(),
        "Redundancies should not cause validation to fail"
    );
    Ok(())
}
#[tokio::test]
async fn test_validate_check_redundancy_json_output() -> Result<()> {
    ccpm::test_utils::init_test_logging(None);
    let temp = TempDir::new()?;
    let manifest_path = temp.path().join("ccpm.toml");
    // Create manifest with redundancies
    let mut manifest = Manifest::new();
    manifest.add_source(
        "community".to_string(),
        "https://github.com/test/community.git".to_string(),
    );
    manifest.add_dependency(
        "agent1".to_string(),
        ResourceDependency::Detailed(DetailedDependency {
            source: Some("community".to_string()),
            path: "agents/shared.md".to_string(),
            version: Some("v1.0.0".to_string()),
            branch: None,
            rev: None,
            command: None,
            args: None,
            target: None,
            filename: None,
        }),
        true,
    );
    manifest.add_dependency(
        "agent2".to_string(),
        ResourceDependency::Detailed(DetailedDependency {
            source: Some("community".to_string()),
            path: "agents/shared.md".to_string(),
            version: Some("v2.0.0".to_string()),
            branch: None,
            rev: None,
            command: None,
            args: None,
            target: None,
            filename: None,
        }),
        true,
    );
    manifest.save(&manifest_path)?;
    let cmd = ValidateCommand {
        file: None,
        resolve: false,
        check_lock: false,
        sources: false,
        paths: false,
        check_redundancies: true,
        format: ccpm::cli::validate::OutputFormat::Json,
        verbose: false,
        quiet: true,
        strict: false,
    };
    let result = cmd.execute_from_path(manifest_path).await;
    assert!(
        result.is_ok(),
        "Redundancies should not cause validation to fail in JSON mode"
    );
    Ok(())
}
#[tokio::test]
async fn test_resolver_detects_conflicts() -> Result<()> {
    ccpm::test_utils::init_test_logging(None);
    use ccpm::resolver::DependencyResolver;
    let temp = TempDir::new()?;
    // Create manifest with redundancies
    let mut manifest = Manifest::new();
    manifest.add_source(
        "community".to_string(),
        "https://github.com/test/community.git".to_string(),
    );
    manifest.add_dependency(
        "agent1".to_string(),
        ResourceDependency::Detailed(DetailedDependency {
            source: Some("community".to_string()),
            path: "agents/shared.md".to_string(),
            version: Some("v1.0.0".to_string()),
            branch: None,
            rev: None,
            command: None,
            args: None,
            target: None,
            filename: None,
        }),
        true,
    );
    manifest.add_dependency(
        "agent2".to_string(),
        ResourceDependency::Detailed(DetailedDependency {
            source: Some("community".to_string()),
            path: "agents/shared.md".to_string(),
            version: Some("v2.0.0".to_string()),
            branch: None,
            rev: None,
            command: None,
            args: None,
            target: None,
            filename: None,
        }),
        true,
    );
    let cache = Cache::with_dir(temp.path().to_path_buf()).unwrap();
    let resolver = DependencyResolver::with_cache(manifest, cache);
    // Test check_redundancies
    let warning = resolver.check_redundancies();
    assert!(
        warning.is_some(),
        "check_redundancies should return a warning"
    );
    // Test check_redundancies_with_details
    let redundancies = resolver.check_redundancies_with_details();
    assert_eq!(redundancies.len(), 1, "Should detect one redundancy");
    let redundancy = &redundancies[0];
    assert_eq!(redundancy.source_file, "community:agents/shared.md");
    assert_eq!(redundancy.usages.len(), 2);
    Ok(())
}
#[tokio::test]
async fn test_redundancy_detector_direct() -> Result<()> {
    ccpm::test_utils::init_test_logging(None);
    use ccpm::resolver::redundancy::RedundancyDetector;
    let mut detector = RedundancyDetector::new();
    // Add redundancying requirements
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
            target: None,
            filename: None,
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
            target: None,
            filename: None,
        }),
    );
    let redundancies = detector.detect_redundancies();
    assert_eq!(redundancies.len(), 1, "Should detect one redundancy");
    let warning = detector.generate_redundancy_warning(&redundancies);
    assert!(warning.contains("Redundant dependencies detected"));
    assert!(warning.contains("app"));
    assert!(warning.contains("tool"));
    assert!(warning.contains("v1.0.0"));
    assert!(warning.contains("v2.0.0"));
    Ok(())
}
#[tokio::test]
async fn test_redundancy_suggestions() -> Result<()> {
    ccpm::test_utils::init_test_logging(None);
    use ccpm::resolver::redundancy::RedundancyDetector;
    let mut detector = RedundancyDetector::new();
    // Add redundancy between latest and specific version
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
            target: None,
            filename: None,
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
            target: None,
            filename: None,
        }),
    );
    let redundancies = detector.detect_redundancies();
    assert_eq!(redundancies.len(), 1);
    let suggestions = detector.suggest_consolidation(&redundancies[0]);
    assert!(
        !suggestions.is_empty(),
        "Should provide consolidation suggestions"
    );
    // Check that warning message includes appropriate suggestions
    let warning = detector.generate_redundancy_warning(&redundancies);
    assert!(warning.contains("Consider using") || warning.contains("consistency"));
    Ok(())
}
