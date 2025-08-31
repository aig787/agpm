# Architecture

This document describes CCPM's technical architecture and design decisions.

## Overview

CCPM is built with Rust for performance, safety, and reliability. It uses system Git commands for maximum compatibility and respects existing Git configurations.

## Core Components

### Module Structure

```
ccpm/
├── cli/          # Command implementations
├── cache/        # Cache management and file locking
├── config/       # Configuration handling
├── core/         # Core types and abstractions
├── git/          # Git command wrapper
├── hooks/        # Claude Code hooks support
├── lockfile/     # Lockfile generation and parsing
├── manifest/     # Manifest parsing and validation
├── markdown/     # Markdown file operations
├── mcp/          # MCP server management
├── models/       # Data models
├── pattern/      # Pattern matching for globs
├── resolver/     # Dependency resolution
├── source/       # Source repository management
├── utils/        # Cross-platform utilities
└── version/      # Version constraint handling
```

### Key Components

**manifest**: Parses and validates ccpm.toml files
- TOML deserialization with serde
- Schema validation
- Pattern expansion for glob dependencies

**lockfile**: Manages ccpm.lock files
- Atomic writes for safety
- Preserves exact commit hashes
- Tracks installation metadata

**resolver**: Dependency resolution engine
- Version constraint matching
- Conflict detection
- Parallel resolution for performance

**cache**: Global Git repository cache
- File locking for concurrent access
- Automatic cleanup
- Incremental updates

**git**: Git command wrapper
- Uses system git binary
- Supports authentication
- Handles platform differences

## Design Decisions

### Copy-Based Installation

CCPM copies files from cache to project directories rather than using symlinks:

- **Maximum compatibility** across Windows, macOS, Linux
- **Git-friendly** - Real files can be tracked
- **Editor-friendly** - No symlink confusion
- **User flexibility** - Edit files without affecting cache

### Repository-Level Versioning

Versions apply to entire repositories, not individual files:

- **Git-native** - Uses tags, branches, commits
- **Simplicity** - No complex per-file tracking
- **Consistency** - All files from same version
- **Trade-off** - Less granular control

### System Git Integration

Uses system git command instead of libgit2:

- **Authentication** - Respects SSH keys, tokens
- **Compatibility** - Works with all Git features
- **Configuration** - Uses existing .gitconfig
- **Updates** - Benefits from Git improvements

### Two-Tier Configuration

Separates project manifest from global config:

- **Security** - Credentials never in repositories
- **Flexibility** - Teams share manifests safely
- **Privacy** - Personal tokens stay local
- **CI/CD friendly** - Easy token injection

## Data Flow

### Installation Process

1. **Parse manifest** - Read ccpm.toml
2. **Load global config** - Merge sources
3. **Resolve dependencies** - Match versions
4. **Fetch repositories** - Clone/update cache
5. **Copy resources** - Install to project
6. **Merge configurations** - Update settings files
7. **Generate lockfile** - Record exact versions

### Dependency Resolution

1. **Parse constraints** - Interpret version specs
2. **Fetch metadata** - Get tags/branches from Git
3. **Match versions** - Find satisfying versions
4. **Detect conflicts** - Check compatibility
5. **Select best** - Choose highest valid version
6. **Lock commits** - Record exact hashes

## File Locking

CCPM uses file locking to prevent corruption during concurrent operations:

```
~/.ccpm/cache/.locks/
├── source1.lock
├── source2.lock
└── source3.lock
```

- Each source has its own lock file
- Locks are acquired before Git operations
- Released automatically on completion
- Cross-platform via fs4 crate

## Caching Strategy

### Cache Structure

```
~/.ccpm/cache/
├── github.com/
│   ├── org1/
│   │   └── repo1/
│   └── org2/
│       └── repo2/
└── gitlab.com/
    └── org3/
        └── repo3/
```

### Cache Operations

- **Initial clone** - Full repository clone
- **Updates** - Incremental fetch
- **Cleanup** - Remove unused repositories
- **Bypass** - `--no-cache` flag for fresh clones

## Security Model

### Credential Handling

- Never store credentials in ccpm.toml
- Global config for sensitive data
- Environment variable expansion
- Token masking in output

### Path Validation

- Prevent path traversal attacks
- Validate against allowlist
- Canonicalize paths safely
- Check symlink targets

### Input Sanitization

- Validate repository URLs
- Sanitize file paths
- Check version strings
- Validate JSON/TOML syntax

## Performance Optimizations

### Parallel Operations

- Concurrent repository fetches
- Parallel file copying
- Async I/O with Tokio
- Configurable parallelism level

### Incremental Updates

- Cache Git repositories
- Fetch only new commits
- Reuse existing installations
- Skip unchanged files

### Memory Efficiency

- Stream large files
- Lazy dependency loading
- Efficient data structures
- Minimal allocations

## Cross-Platform Support

### Path Handling

- Normalize separators
- Handle long paths (Windows)
- Expand ~ and env vars
- Support UNC paths

### Line Endings

- Preserve original endings
- Git autocrlf support
- Binary file detection
- Consistent TOML format

### File System Differences

- Case sensitivity handling
- Permission model differences
- Symbolic link support
- Reserved filename checking

## Error Handling

### Error Types

- **User errors** - Invalid input, missing files
- **System errors** - I/O, permissions, network
- **Git errors** - Clone, fetch, checkout failures
- **Validation errors** - Schema, version conflicts

### Error Context

Each error includes:
- Clear message
- Suggested fixes
- Relevant file/line
- Debug information

## Testing Strategy

### Unit Tests

- Module-level testing
- Mock external dependencies
- Property-based testing
- Coverage > 70%

### Integration Tests

- End-to-end workflows
- Real Git repositories
- Cross-platform CI
- Parallel test execution

### Test Infrastructure

- TestEnvironment helper
- Fixture repositories
- Isolated temp directories
- No global state

## Future Considerations

### Potential Enhancements

- Plugin system for custom resources
- Binary distribution via registries
- Incremental compilation caching
- P2P resource sharing

### Scalability

- Repository sharding
- CDN integration
- Distributed caching
- Partial clones

### Extensibility

- Custom resource types
- Hook system for events
- External tool integration
- API for tooling

## Dependencies

Key dependencies and their purposes:

- **clap** - CLI argument parsing
- **tokio** - Async runtime
- **serde** - Serialization
- **toml** - Configuration format
- **semver** - Version parsing
- **indicatif** - Progress bars
- **fs4** - File locking
- **glob** - Pattern matching

See Cargo.toml for complete list.