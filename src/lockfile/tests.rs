#[cfg(test)]
mod lockfile_tests {
    use super::super::*;
    use tempfile::TempDir;

    #[test]
    fn test_lockfile_version() {
        let lockfile = LockFile::new();
        assert_eq!(lockfile.version, LockFile::CURRENT_VERSION);
    }

    #[test]
    fn test_add_resources() {
        let mut lockfile = LockFile::new();

        // Add a source
        lockfile.add_source(
            "test-source".to_string(),
            "https://github.com/test/repo.git".to_string(),
            "abc123".to_string(),
        );

        // Add an agent
        lockfile.add_resource(
            "test-agent".to_string(),
            LockedResource {
                name: "test-agent".to_string(),
                source: Some("test-source".to_string()),
                path: "agents/test.md".to_string(),
                version: Some("v1.0.0".to_string()),
                resolved_commit: Some("abc123".to_string()),
                checksum: "sha256:test".to_string(),
                installed_at: "agents/test-agent.md".to_string(),
dependencies: vec![],
},
            true, // is_agent
        );

        assert_eq!(lockfile.sources.len(), 1);
        assert_eq!(lockfile.agents.len(), 1);
        assert!(lockfile.has_resource("test-agent"));
    }

    #[test]
    fn test_save_and_load() {
        let temp_dir = TempDir::new().unwrap();
        let lockfile_path = temp_dir.path().join("agpm.lock");

        let mut lockfile = LockFile::new();

        // Add some data
        lockfile.add_source(
            "source1".to_string(),
            "https://github.com/test/repo1.git".to_string(),
            "commit1".to_string(),
        );

        lockfile.add_resource(
            "agent1".to_string(),
            LockedResource {
                name: "agent1".to_string(),
                source: Some("source1".to_string()),
                path: "agents/a1.md".to_string(),
                version: Some("v1.0.0".to_string()),
                resolved_commit: Some("commit1".to_string()),
                checksum: "sha256:abc".to_string(),
                installed_at: "agents/agent1.md".to_string(),
dependencies: vec![],
},
            true, // is_agent
        );

        // Save
        lockfile.save(&lockfile_path).unwrap();

        // Load
        let loaded = LockFile::load(&lockfile_path).unwrap();

        assert_eq!(loaded.sources.len(), 1);
        assert_eq!(loaded.agents.len(), 1);
        assert_eq!(loaded.get_source("source1").unwrap().commit, "commit1");
        assert_eq!(loaded.get_resource("agent1").unwrap().checksum, "sha256:abc");
    }

    #[test]
    fn test_clear() {
        let mut lockfile = LockFile::new();

        // Add data
        lockfile.add_source(
            "source".to_string(),
            "url".to_string(),
            "commit".to_string(),
        );

        lockfile.add_resource(
            "resource".to_string(),
            LockedResource {
                name: "resource".to_string(),
                source: None,
                path: "path.md".to_string(),
                version: None,
                resolved_commit: None,
                checksum: "checksum".to_string(),
                installed_at: "installed.md".to_string(),
dependencies: vec![],
},
            true, // is_agent
        );

        assert!(!lockfile.sources.is_empty());
        assert!(!lockfile.agents.is_empty());

        // Clear
        lockfile.clear();

        assert!(lockfile.sources.is_empty());
        assert!(lockfile.agents.is_empty());
        assert!(lockfile.all_resources().is_empty());
    }

    #[test]
    fn test_multiple_resources() {
        let mut lockfile = LockFile::new();

        // Add multiple resources
        lockfile.add_resource(
            "prod-agent".to_string(),
            LockedResource {
                name: "prod-agent".to_string(),
                source: None,
                path: "agents/prod.md".to_string(),
                version: None,
                resolved_commit: None,
                checksum: "sha256:prod".to_string(),
                installed_at: "agents/prod-agent.md".to_string(),
dependencies: vec![],
},
            true, // is_agent
        );

        // Add another resource
        lockfile.add_resource(
            "dev-agent".to_string(),
            LockedResource {
                name: "dev-agent".to_string(),
                source: None,
                path: "agents/dev.md".to_string(),
                version: None,
                resolved_commit: None,
                checksum: "sha256:dev".to_string(),
                installed_at: "agents/dev-agent.md".to_string(),
dependencies: vec![],
},
            true, // is_agent
        );

        // Note: production_resources() removed as dev/production concept was eliminated
        assert_eq!(lockfile.all_resources().len(), 2);
    }
}
