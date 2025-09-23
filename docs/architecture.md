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

**resolver**: Dependency resolution engine with SHA-based optimization
- DependencyResolver: Main entry point for dependency resolution
- VersionResolver: Centralized batch version-to-SHA resolution
- Version constraint matching with upfront resolution (semver ranges: ^1.0, ~2.1)
- Conflict detection and parallel resolution for performance
- Command-instance caching to minimize network operations
- Two-phase operation: collection then batch SHA resolution

**cache**: Advanced Git repository cache with worktree management
- Instance-level caching with WorktreeState tracking
- File locking for safe concurrent access across processes
- Automatic cleanup with configurable retention policies
- Incremental updates with fetch operation deduplication

**git**: Git command wrapper
- Uses system git binary for maximum compatibility
- Supports authentication (SSH keys, tokens)
- Handles platform differences
- Enhanced with bare repository detection

**resolver/version_resolver**: Centralized SHA resolution engine
- VersionResolver: High-performance batch resolution of all dependency versions to commit SHAs
- Deduplication of identical (source, version) pairs for optimal efficiency
- Command-instance caching to minimize network operations
- Enhanced semver constraint support (^1.0, ~2.1, >=1.0.0, <2.0.0) with intelligent tag matching
- Two-phase operation: collection phase gathers all unique dependencies, resolution phase batch processes
- ResolvedVersion tracking with both SHA and resolved reference information
- Single fetch per repository per command execution

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

### Dependency Resolution (Centralized SHA-Based)

1. **Parse constraints** - Interpret version specs (tags, branches, semver constraints, exact commits)
2. **Collect unique versions** - VersionResolver deduplicates (source, version) pairs across all dependencies
3. **Batch resolution** - Single operation per repository resolves all required versions to commit SHAs
4. **Constraint resolution** - Enhanced semver matching finds best tags for constraints (^1.0, ~2.1, >=1.0.0, <2.0.0)
5. **Fetch optimization** - Command-instance caching prevents redundant network operations
6. **SHA validation** - Validate all resolved SHAs are valid 40-character hex strings
7. **Conflict detection** - Check compatibility across all resolved dependencies
8. **Worktree optimization** - SHA-based worktree creation maximizes reuse for identical commits
9. **Lock generation** - Record exact SHAs and resolved references in ccpm.lock

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

CCPM v0.3.2+ uses a sophisticated SHA-based caching architecture with centralized version resolution:

```
~/.ccpm/cache/
├── sources/                 # Bare repositories (shared storage)
│   ├── github_org1_repo1.git/         # Single bare repo per source
│   ├── github_org2_repo2.git/         # Optimized for worktree creation
│   └── gitlab_org3_repo3.git/         # All Git objects stored here
├── worktrees/              # SHA-based worktrees (maximum deduplication)
│   ├── github_org1_repo1_abc12345/    # First 8 chars of commit SHA
│   ├── github_org1_repo1_def67890/    # Different SHA = different worktree
│   ├── github_org1_repo1_abc12345/    # Same SHA = shared worktree (reused)
│   └── github_org2_repo2_456789ab/    # Cross-repository SHA uniqueness
└── .locks/                 # Per-repository file locks
    ├── github_org1_repo1.lock         # Repository-level locking
    └── github_org2_repo2.lock         # Not per-worktree for efficiency
```

### Worktree Architecture Benefits

- **Parallel Safety**: Multiple operations can access different versions simultaneously
- **Resource Efficiency**: Single bare repository supports unlimited concurrent checkouts
- **Version Isolation**: Each worktree can be at a different commit/tag/branch
- **Fast Operations**: No blocking on shared repository state
- **UUID Paths**: Prevents conflicts in parallel operations

### Cache Operations

- **Centralized Version Resolution** - VersionResolver handles batch SHA resolution before any worktree operations
- **Initial clone** - Clone as bare repository with `--bare` flag for optimal worktree support
- **SHA-based worktree naming** - Worktrees named by first 8 chars of commit SHA for maximum deduplication
- **Two-phase operation** - Collection phase followed by batch resolution phase
- **Instance-level caching** - WorktreeState enum tracks creation status (Pending/Ready) within single command
- **Command-instance fetch caching** - Single fetch per repository per command execution
- **Intelligent deduplication** - Multiple references (tags/branches) to same commit share one worktree
- **Parallel access** - Independent worktrees enable safe concurrent operations with zero conflicts
- **Enhanced constraint matching** - Support for complex semver ranges (^1.0, ~2.1, >=1.0.0, <2.0.0)
- **Fast cleanup** - Simple directory removal without complex Git state management
- **Incremental updates** - Fetch to bare repository, shared across all worktrees
- **Cache bypass** - `--no-cache` flag for fresh clones when needed

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

## Concurrency Model

CCPM v0.3.0 implements a sophisticated concurrency system designed for maximum performance while maintaining safety:

### Command-Level Parallelism

- **Direct Control**: `--max-parallel` flag provides direct parallelism control
- **Smart Defaults**: Default parallelism is `max(10, 2 × CPU cores)` for optimal performance
- **No Global Bottlenecks**: Removed Git semaphore in favor of fine-grained locking
- **Configurable**: Users can tune parallelism based on system resources and network capacity

### Worktree-Based Concurrency

- **Parallel-Safe Operations**: Git worktrees enable safe concurrent access to repositories
- **Version Isolation**: Each operation gets its own working directory with specific version
- **UUID-Based Paths**: Prevent naming conflicts in concurrent operations
- **Instance-Level State**: WorktreeState enum (Pending/Ready) tracks creation across threads

### Fetch Optimization

- **Per-Command Caching**: Network operations cached per command instance to reduce redundant fetches
- **Per-Repository Locking**: Fine-grained locks instead of global Git semaphore
- **Batch Operations**: Multiple dependencies from same source share fetch operations
- **Concurrent Fetches**: Different repositories can be fetched simultaneously

### File System Safety

- **Per-Worktree Locks**: Each worktree operation is independently locked
- **Atomic Operations**: File copying uses temp-file + rename pattern
- **Cross-Platform Locking**: `fs4` crate provides platform-agnostic file locking
- **Clean Isolation**: Operations don't interfere with each other

## Performance Optimizations

### Parallel Operations

- **Worktree-based concurrency**: Each dependency gets its own isolated Git worktree for parallel processing
- **Configurable parallelism**: User-controlled via `--max-parallel` flag (default: max(10, 2 × CPU cores))
- **Instance-Level Caching**: WorktreeState tracking with per-command fetch caching
- **Smart Batching**: Operations on same repository share worktrees when possible
- **Async I/O with Tokio**: Non-blocking file operations and network requests
- **Context-aware logging**: Dependency names included in Git operation logs for debugging

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

## Worktree-Based Parallel Architecture

CCPM's advanced parallel processing system uses Git worktrees to enable safe concurrent access to different versions of the same repository, dramatically improving installation performance.

### Core Benefits

- **True Parallelism**: Multiple dependencies from the same repository can be processed simultaneously
- **Version Isolation**: Each worktree operates at a different commit/tag/branch without conflicts
- **Performance Optimization**: Eliminates blocking on shared repository state
- **Resource Efficiency**: Single bare repository supports unlimited concurrent checkouts
- **Safe Concurrency**: No race conditions or corruption during parallel operations

### Implementation Details

1. **Bare Repository Foundation**: Each source is cloned once as a bare repository optimized for worktree creation
2. **UUID-Based Worktrees**: Temporary worktrees created with unique identifiers for each operation
3. **Instance-Level Caching**: Worktrees are cached and reused within a single command execution
4. **Parallel Resource Installation**: Each dependency uses its own worktree for conflict-free processing
5. **Deferred Cleanup**: Worktrees remain for potential reuse, cleaned up by cache management

### Enhanced Directory Structure

```
~/.ccpm/cache/
├── sources/                           # Bare repositories for worktree use
│   ├── github_owner_repo.git/         # Optimized bare repo
│   └── gitlab_org_project.git/        # Multiple sources supported
├── worktrees/                         # Temporary worktrees for parallel ops
│   ├── github_owner_repo_uuid1/       # Worktree at v1.0.0 for dependency A
│   ├── github_owner_repo_uuid2/       # Worktree at v2.0.0 for dependency B
│   ├── github_owner_repo_uuid3/       # Worktree at main for dependency C
│   └── gitlab_org_project_uuid4/      # Different source, parallel processing
└── .locks/                            # Repository-level locking
    ├── github_owner_repo.lock         # Per-source locks for safety
    └── gitlab_org_project.lock        # Prevents concurrent modifications
```

### Parallelism Control

- **Command-Level**: `--max-parallel` flag controls dependency concurrency (default: max(10, 2 × CPU cores))
- **Git-Level**: Global semaphore prevents Git process overload (internal limit)
- **Per-Repository**: File locks ensure safe concurrent access to bare repositories
- **Worktree-Level**: Each dependency gets isolated working directory for conflict-free operations

## Advanced Concurrency Control

CCPM implements a sophisticated multi-layered concurrency system designed for optimal performance while maintaining safety across all operations.

### User-Controlled Parallelism

- **`--max-parallel` Flag**: Users can control dependency-level concurrency
- **Smart Defaults**: Default limit of max(10, 2 × CPU cores) balances performance and resource usage
- **Per-Command Configuration**: Different commands can use different parallelism levels
- **Runtime Adaptation**: System resources are considered when setting limits

### Internal Concurrency Layers

#### Git Operation Semaphore
- **Purpose**: Prevents system overload from excessive concurrent Git processes
- **Scope**: All Git operations (clone, fetch, worktree creation, checkout)
- **Adaptive Limiting**: Automatically adjusts based on system capabilities
- **Queue Management**: Efficiently schedules Git operations to prevent bottlenecks

#### Repository-Level Locking
- **Per-Source Isolation**: Each repository source has its own lock file
- **Atomic Operations**: Lock acquisition prevents race conditions during repository modifications
- **Cross-Process Safety**: Multiple CCPM instances can run simultaneously without conflicts
- **Platform Compatibility**: Uses `fs4` crate for consistent cross-platform file locking

#### Worktree Isolation
- **UUID-Based Paths**: Each worktree has a unique identifier preventing path conflicts
- **Version Isolation**: Multiple versions of the same repository can be checked out simultaneously
- **Zero-Conflict Operations**: Dependencies from the same source process in parallel safely
- **Efficient Cleanup**: Directory removal without complex Git state management

### Performance Optimizations

#### Instance-Level Caching
- **Worktree Reuse**: Created worktrees are cached for the duration of a command execution
- **Fetch Optimization**: Repository fetches are deduplicated within a single command
- **Context Propagation**: Dependency names are tracked through the operation chain for debugging
- **State Management**: WorktreeState enum tracks creation status for optimal resource allocation

#### Stream-Based Processing
- **Unlimited Task Concurrency**: Uses `buffer_unordered(usize::MAX)` for maximum task parallelism
- **Git Bottleneck Management**: The Git semaphore naturally limits the actual bottleneck
- **Progress Coordination**: Thread-safe progress tracking across all parallel operations
- **Error Propagation**: Atomic failure handling ensures consistent state on errors

### Enhanced Debugging and Monitoring

CCPM provides comprehensive logging and monitoring capabilities for understanding parallel operations.

#### Context-Aware Logging
- **Dependency Context**: All Git operations include the dependency name being processed
- **Structured Output**: Uses targeted logging (`target="git"`) for filtering specific operation types
- **Clean User Interface**: Production output remains clean while debug information is available
- **Operation Tracking**: Detailed tracking of worktree creation, checkout, and cleanup operations

#### Multi-Phase Progress Reporting
- **Phase Transitions**: Clear indication when moving between resolution, installation, and configuration phases
- **Real-Time Updates**: Live progress updates showing current operation and completion status
- **Thread-Safe Coordination**: Progress updates work correctly across all parallel operations
- **User Feedback**: Clear messaging about what's happening during long-running operations

#### Example Debug Output

```bash
# Context-aware Git operation logging
DEBUG git: (rust-expert-agent) Cloning bare repository: https://github.com/community/ccpm-resources.git
DEBUG git: (rust-expert-agent) Creating worktree at commit abc123: agents/rust-expert.md
DEBUG git: (react-snippets) Reusing existing bare repository cache
DEBUG git: (react-snippets) Creating worktree at tag v2.1.0: snippets/react/*.md

# Multi-phase progress updates
Resolving dependencies... ████████████████████████████████ 100%
Installing 0/15 resources... ████████████████████████████████ 100%
Updating configurations... ████████████████████████████████ 100%
```

## Future Considerations

### Performance Enhancements

- **Partial Clone Support**: Use Git's partial clone for faster initial repository access
- **Incremental Worktree Creation**: Optimize worktree creation for large repositories
- **Parallel Fetch Optimization**: Further optimize network operations for multiple sources
- **Smart Cache Warming**: Pre-populate cache based on usage patterns

### Scalability Improvements

- **Repository Sharding**: Distribute large source repositories across multiple endpoints
- **CDN Integration**: Cache popular resources on content delivery networks
- **Distributed Caching**: Share cache entries across team members or CI systems
- **Bandwidth Optimization**: Implement differential sync for repository updates

### Architecture Extensions

- **Plugin System**: Custom resource types and installation handlers
- **Event Hooks**: Extensible hook system for custom workflows
- **External Tool Integration**: APIs for IDE and tooling integration
- **Distributed Coordination**: Multi-machine coordination for large-scale deployments

### Monitoring and Observability

- **Performance Metrics**: Detailed timing and resource usage tracking
- **Cache Analytics**: Insights into cache hit rates and optimization opportunities
- **Parallel Operation Insights**: Visualization of concurrency patterns and bottlenecks
- **Resource Usage Monitoring**: Track disk space, network bandwidth, and CPU utilization

## Dependencies

Key dependencies and their purposes in CCPM's architecture:

### Core Framework
- **tokio** - Async runtime enabling non-blocking I/O and concurrent operations
- **futures** - Stream processing for parallel task coordination
- **clap** - CLI argument parsing with structured command definitions

### Data Management
- **serde** - Serialization framework for manifest and lockfile handling
- **toml** - Configuration format parsing for project and global configs
- **semver** - Semantic version parsing and constraint matching

### Concurrency and Safety
- **fs4** - Cross-platform file locking for repository-level synchronization
- **uuid** - Unique identifier generation for worktree paths
- **once_cell** - Thread-safe global state management

### User Interface
- **indicatif** - Multi-phase progress bars with real-time updates
- **colored** - Terminal color output for improved user experience

### File and Network Operations
- **glob** - Pattern matching for bulk resource installation
- **walkdir** - Recursive directory traversal
- **sha2** + **hex** - Content checksumming for integrity verification
- **shellexpand** - Environment variable expansion in paths

### Testing Infrastructure
- **assert_cmd** - CLI testing framework
- **predicates** - Assertion helpers for test validation
- **serial_test** - Sequential test execution for resource-sensitive tests
- **tempfile** - Temporary directory management in tests

See Cargo.toml for complete dependency list with exact versions.