# CLAUDE.md - Project Context for Claude

## Project Overview

CCPM (Claude Code Package Manager) is a Git-based package manager for Claude Code resources (agents, snippets, and
more), written in Rust. It follows a lockfile-based dependency management model similar to Cargo, enabling reproducible
installations of AI resources from multiple Git repositories. The system is designed to work seamlessly on Windows,
macOS, and Linux.

## Key Architecture Decisions

- **Language**: Rust for performance, safety, and reliability
- **Distribution Model**: Git-based, no central registry - fully decentralized
- **Dependency Management**: Lockfile-based (ccpm.toml + ccpm.lock) like Cargo
- **Configuration Format**: TOML for manifest and lockfile
- **Resource Format**: Markdown files (.md) for agents, snippets, and commands; JSON files (.json) for hooks and MCP servers; executable files (.sh, .js, .py) for scripts
- **MCP Servers**: JSON configuration files installed to `.mcp.json` for Claude Code
- **Hooks**: JSON configuration files configured in `.claude/settings.local.json`
- **CLI Framework**: Using Clap for command-line parsing
- **Async Runtime**: Tokio for concurrent operations
- **Git Operations**: System `git` command via CLI (like cargo with `git-fetch-with-cli`)
- **Version Management**: Git tags, branches, or specific commits

## Project Structure

```
ccpm/
├── src/
│   ├── main.rs           # CLI entry point
│   ├── cli/              # Command implementations
│   ├── cache/            # Cache management and locking
│   ├── config/           # Global and project configuration
│   ├── core/             # Core functionality and error handling
│   ├── git/              # Git CLI wrapper
│   ├── hooks/            # Git hooks and merge strategies
│   ├── lockfile/         # Lockfile (ccpm.lock) management
│   ├── manifest/         # Manifest (ccpm.toml) parsing
│   ├── markdown/         # Markdown file operations
│   ├── mcp/              # MCP server management
│   ├── models/           # Data models and structures
│   ├── resolver/         # Dependency resolution
│   ├── source/           # Source repository operations
│   ├── test_utils/       # Testing utilities and fixtures
│   ├── utils/            # Cross-platform utilities
│   └── version/          # Version constraint handling
├── tests/                # Integration tests
├── .claude/
│   ├── agents/           # Claude Code specialized agents
│   ├── commands/         # Custom Claude Code commands
│   └── snippets/         # Code snippets for development
├── Cargo.toml           # Project manifest
├── README.md            # User-facing documentation
├── USAGE.md             # Usage guide and examples
├── CONTRIBUTING.md      # Contribution guidelines
├── LICENSE.md           # MIT License
└── CLAUDE.md            # This file (AI context)
```

## Available Claude Code Agents

The project includes specialized Rust agents for different aspects of development:

- **rust-expert**: Expert Rust developer for implementation, refactoring, and API design. Delegates memory issues, undefined behavior, and deep debugging to rust-troubleshooter-opus.
- **rust-linting-expert**: Fast linting fixes using cargo fmt and clippy --fix. Delegates complex refactoring to rust-expert, memory issues to rust-troubleshooter-opus.
- **rust-doc-expert**: Expert in adding comprehensive documentation, docstrings, and architectural documentation to Rust projects. Ensures all code is properly documented with examples and explanations.
- **rust-test-fixer**: Fast test failure fixer for assertion failures, missing imports, and test setup issues. Delegates complex refactoring to rust-expert, memory/UB issues to rust-troubleshooter-opus.
- **rust-troubleshooter-opus**: Advanced Rust troubleshooting expert using Opus 4.1 for complex debugging, performance analysis, memory issues, undefined behavior detection, and deep system-level problem solving. Use when rust-expert cannot resolve the issue.

### Agent Delegation Patterns

Agents work together through delegation:
- **rust-linting-expert** → rust-expert (for complex refactoring)
- **rust-linting-expert** → rust-troubleshooter-opus (for memory issues)
- **rust-test-fixer** → rust-expert (for complex refactoring)
- **rust-test-fixer** → rust-troubleshooter-opus (for memory/UB issues)
- **rust-expert** → rust-troubleshooter-opus (for issues it cannot resolve)

## Core Commands

1. `install` - Install dependencies from ccpm.toml, generate/update ccpm.lock
   - `--frozen` - Use existing lockfile without updates (for CI/production)
   - `--no-cache` - Bypass cache and fetch directly from sources
   - Installs MCP servers to `.mcp.json` for Claude Code
   - Installs hooks to `.claude/settings.local.json`
2. `update` - Update dependencies within version constraints
   - Updates specific or all dependencies to latest compatible versions
3. `list` - List installed resources from ccpm.lock
   - Shows all installed agents, snippets, commands, scripts, hooks, and MCP servers
4. `validate` - Validate ccpm.toml syntax and source availability
   - `--check-lock` - Also validate lockfile consistency
   - `--resolve` - Perform full dependency resolution check
5. `cache` - Manage global git cache (~/.ccpm/cache/)
   - `clean` - Remove unused cache entries
   - `list` - Show cached repositories
6. `config` - Manage global configuration (~/.ccpm/config.toml)
   - `get` - Retrieve configuration values
   - `set` - Set configuration values
7. `add` - Add sources and dependencies to ccpm.toml manifest
   - `source` - Add a new source repository
   - `dep` - Add a new dependency
8. `remove` - Remove sources and dependencies from ccpm.toml manifest
   - `source` - Remove a source repository
   - `dep` - Remove a dependency
9. `init` - Initialize new CCPM project with ccpm.toml
   - `--path` - Specify custom project directory

## Development Guidelines

- Follow Rust best practices and idioms
- Use `Result<T, E>` for error handling
- Implement comprehensive error messages for CLI users
- Write unit tests for core functionality
- Write integration tests for CLI commands
- Test on Windows, macOS, and Linux
- Use `clippy` for linting: `cargo clippy`
- Format code with: `cargo fmt`
- Run tests with: `cargo test`
- Use `cfg!` macros for platform-specific code
- Handle path separators correctly across platforms

## Key Dependencies

- `clap` (4.5) - Command-line argument parsing with derive macros
- `tokio` (1.47) - Async runtime with full features
- `toml` (0.9) - TOML parsing and serialization
- `serde` (1.0) - Serialization framework with derive
- `serde_json` (1.0) - JSON support for metadata
- `serde_yaml` (0.9) - YAML parsing for configuration files
- `semver` (1.0) - Semantic version parsing for git tags
- `anyhow` (1.0) - Error handling with context
- `thiserror` (2.0) - Custom error types with derive
- `colored` (3.0) - Terminal colors for CLI output
- `dirs` (6.0) - Platform-specific directory paths
- `tracing` (0.1) - Structured, event-based diagnostics
- `tracing-subscriber` (0.3) - Utilities for tracing subscribers
- `indicatif` (0.18) - Progress bars and spinners
- `tempfile` (3.10) - Temporary file/directory management
- `shellexpand` (3.1) - Shell-like path expansion (~, env vars)
- `which` (8.0) - Command detection in PATH
- `uuid` (1.10) - Unique identifier generation
- `chrono` (0.4) - Date and time handling
- `walkdir` (2.5) - Recursive directory traversal
- `sha2` (0.10) - SHA-256 hashing for checksums
- `hex` (0.4) - Hexadecimal encoding/decoding
- `regex` (1.11) - Regular expression matching
- `futures` (0.3) - Async programming primitives
- `fs4` (0.13) - Extended file system operations with locking

## Testing Strategy

- Unit tests for core logic in each module
- Integration tests for CLI commands in `tests/` directory
- Test fixtures with sample resource repositories
- Mock Git operations for testing
- CI matrix testing on Windows, macOS, and Linux
- Test path handling on all platforms
- Test with different shells (cmd, PowerShell, bash, zsh)
- **Target Coverage**: Minimum 70% test coverage
- **Coverage Tool**: `cargo tarpaulin` for coverage reports
- Run coverage with: `make coverage` or `cargo tarpaulin --out html`

### Critical Testing Requirements

- **Parallel Test Safety**: Tests are designed to run safely in parallel after eliminating WorkingDirGuard
- **Environment variable handling in tests**:
    - **NEVER use `std::env::set_var` in regular tests** - This causes race conditions when tests run in parallel
    - **Exception**: Tests that explicitly test environment variable functionality (e.g., testing env var expansion) MAY
      use `std::env::set_var` BUT:
        - MUST be clearly documented with a comment explaining they test env var behavior
        - Should restore original values (use EnvGuard or similar)
        - Run these specific tests with `cargo test -- --test-threads=1` if flakiness occurs
        - Consider grouping such tests in a separate test module
    - **For other tests needing env vars**:
        - For subprocesses: Pass env vars to specific Command instances using `.env()`
        - For functions needing env vars: Refactor to accept them as parameters or via a config struct
- **Cache directory isolation**: Each test should use its own temp directory for cache
- **No global state**: Tests must not rely on or modify global state that could affect other tests (except when
  explicitly testing such functionality)
- **Async file I/O in tests**: Tests using async functions should use `tokio::fs` instead of `std::fs` to match production code patterns
- **Working Directory Management**: Tests no longer use WorkingDirGuard to avoid runtime spawning issues

## Build and Release

```bash
# Development build
cargo build

# Release build  
cargo build --release

# Code quality checks (MUST pass before commit)
cargo fmt                    # Format code
cargo clippy -- -D warnings  # Check for issues
cargo doc --no-deps          # Verify documentation
cargo test                   # Run all tests

# Run with verbose output
RUST_LOG=debug cargo run

# Full pre-commit check
cargo fmt && cargo clippy -- -D warnings && cargo test
```

## Module Organization

The codebase is organized into logical modules:

- **cli/** - Command-line interface implementations for all CCPM commands
- **cache/** - Global cache management and file locking for concurrent access
- **config/** - Configuration handling for both global and project settings
- **core/** - Core types, error handling, and resource abstractions
- **git/** - Git command wrapper and repository operations
- **hooks/** - Claude Code hooks support and settings.local.json management
- **lockfile/** - Lockfile generation and parsing (ccpm.lock)
- **manifest/** - Manifest parsing and validation (ccpm.toml)
- **markdown/** - Markdown file operations and frontmatter extraction
- **mcp/** - MCP server configuration and .mcp.json management
- **models/** - Data models for dependencies and resources
- **resolver/** - Dependency resolution, version matching, and conflict detection
- **source/** - Source repository management and caching
- **test_utils/** - Testing utilities, fixtures, and environment setup
- **utils/** - Cross-platform utilities, security validation, and file operations
- **version/** - Version constraint parsing and matching

## Implementation Lessons Learned

### Architecture Decisions That Worked Well

1. **Modular structure** - Each module has clear responsibilities
2. **Error context pattern** - ErrorContext struct provides suggestions and details
3. **Resource trait abstraction** - Allows easy extension for new resource types
4. **Atomic file operations** - Write to temp file then rename for safety
5. **Platform-specific code isolation** - Using cfg! macros and separate functions
6. **Async file I/O** - Using `tokio::fs` instead of `std::fs` in async contexts to prevent blocking the runtime
7. **Parallel test execution** - Eliminated WorkingDirGuard to enable safe parallel test execution without runtime spawning

### Design Decision: Copy-Based Installation

CCPM copies files from the cache to project directories rather than using symlinks. This decision provides:

- **Maximum compatibility** across Windows, macOS, and Linux without special permissions
- **Git-friendly** behavior since real files can be tracked and committed
- **Editor compatibility** with no symlink confusion
- **User flexibility** to edit local files without affecting the cache

### Key Implementation Details

1. **Dependency Management**: Manifest (ccpm.toml) + Lockfile (ccpm.lock)
2. **Resource Formats**: 
   - Agents, snippets, commands: Markdown files (.md) with optional frontmatter metadata
   - Scripts: Executable files (.sh, .js, .py)
   - Hooks: JSON configuration files defining Claude Code event handlers
   - MCP servers: JSON configuration files defining Model Context Protocol servers
3. **Source Resolution**: Named sources in manifest, cloned/cached locally
4. **Version Constraints**: Support tags, branches, and specific commits
5. **Installation**: Copy resource files from cache to project locations
6. **MCP Servers**: Install JSON files to disk and configure in .mcp.json
7. **Path handling**: Always use absolute paths internally, normalize separators
8. **Windows considerations**: Handle long paths (>260 chars), different git command
9. **Global Config**: ~/.ccpm/config.toml for auth tokens and private sources
10. **Cache Architecture**: ~/.ccpm/cache/ for cloned repositories
11. **Hooks Configuration**: Installed as files and configured in .claude/settings.local.json
12. **MCP Configuration**: Installed as files and configured in .mcp.json

### Testing Insights

1. **Integration tests are crucial** - Test actual CLI invocations
2. **Tempdir for file tests** - Use tempfile crate for isolated testing
3. **Platform-specific tests** - Use cfg(test) with platform conditions
4. **Test infrastructure** - Git daemon for realistic repository testing

### Windows Path Handling Gotchas

When working with paths on Windows, be aware of these critical issues:

1. **Absolute Path Detection**:
   - Windows absolute paths can be `C:\path` or `\\server\share` (UNC)
   - Simple checks like `starts_with('/')` won't work on Windows
   - Use dedicated helper functions to detect absolute paths across platforms
   - Watch out for `file:` prefix being mistaken for a drive letter pattern

2. **file:// URL Handling**:
   - Windows paths in file:// URLs should use forward slashes: `file://C:/path/to/repo`
   - When converting back to filesystem paths, replace forward slashes with backslashes on Windows
   - Always use helper functions like `path_to_file_url()` for consistent URL generation

3. **Path Validation**:
   - `Component::RootDir` (paths starting with `/`) is valid on Windows within projects
   - Don't reject `/path` as invalid just because it's not absolute on Windows
   - Be careful distinguishing between security issues (path traversal) and legitimate paths

4. **Reserved Device Names**:
   - Windows has reserved names like `CON`, `PRN`, `AUX`, `NUL`, `COM1-9`, `LPT1-9`
   - These cannot be used as filenames and will cause sync issues
   - Add them to .gitignore to prevent accidental commits

5. **Test Path Handling**:
   - Never use platform-specific invalid paths like `/dev/null/invalid` or `NUL:` in tests
   - Create invalid paths that work across platforms (e.g., file used as directory)
   - Use `file://` URLs in tests for better cross-platform compatibility

6. **Common Pitfalls**:
   - Don't assume colons (`:`) only appear in URLs - Windows paths have them too (`C:`)
   - Path separator normalization: always convert to forward slashes for TOML files
   - Test on actual Windows systems - WSL/Linux behavior differs significantly

### Integration Test Coverage

The `tests/` directory contains comprehensive integration tests:
- `integration_cross_platform.rs` - Cross-platform path and behavior testing
- `integration_deploy.rs` - Deployment and installation scenarios
- `integration_error_scenarios.rs` - Error handling and recovery
- `integration_gitignore.rs` - Gitignore generation and management
- `integration_list.rs` - List command functionality
- `integration_multi_resource.rs` - Multi-resource installation and management
- `integration_redundancy.rs` - Redundancy detection and handling
- `integration_test_helpers_example.rs` - Test helper utility examples
- `integration_update.rs` - Update command and version constraint handling
- `integration_validate.rs` - Manifest and lockfile validation
- `integration_versioning.rs` - Version resolution and Git reference handling

## Recent Architectural Changes

- **Removed `once_cell` dependency**: Replaced with standard library features for better maintainability
- **Eliminated WorkingDirGuard**: Refactored tests to run safely in parallel without changing working directories
- **API improvements**: CLI commands now accept optional manifest_path parameter for better flexibility
- **Enhanced agent documentation**: Added detailed agent delegation patterns and specializations

## Notes for Claude

- Focus on idiomatic Rust code
- Prioritize error handling and user-friendly error messages
- Keep the CLI interface simple and intuitive
- CRITICAL: Ensure cross-platform compatibility (Windows, macOS, Linux)
- Use async/await for potentially long-running operations (Git clones, etc.)
- Implement progress indicators for long operations
- Support multiple resource types (agents, snippets, commands, scripts, hooks, MCP servers)
- Scripts are executable files (.sh, .js, .py) that can be referenced by hooks
- Hooks are JSON configuration files that define Claude Code automation event handlers
- MCP servers are JSON configuration files that get configured in `.mcp.json`
- Hooks are JSON configuration files that get configured in `.claude/settings.local.json`
- Preserve user-managed entries in settings.local.json (non-destructive updates)
- Handle Windows-specific issues (paths, permissions, shell differences)
- Test thoroughly on all three major platforms
- Remember: NO git2 library - use system git command via process execution
- **SECURITY RULES**:
    - **Credentials Isolation**: NEVER allow credentials in ccpm.toml or any file intended for version control
        - Authentication tokens and secrets MUST only go in ~/.ccpm/config.toml
        - Reject any feature requests for inline authentication in manifests
        - Always validate that ccpm.toml files contain no sensitive data
    - **Input Validation**:
        - Sanitize all user inputs to prevent command injection when executing git
        - Validate repository URLs against allowlist patterns before cloning
        - Reject path traversal attempts (../, absolute paths outside project)
        - Validate version constraints to prevent malicious version strings
    - **Resource Verification**:
        - Implement checksum validation for installed resources (SHA-256)
        - Warn users before installing resources without checksums
        - Validate markdown file content before execution (no embedded scripts)
        - Limit resource file sizes to prevent denial of service
    - **Network Security**:
        - Use HTTPS for all git operations by default
        - Validate SSL certificates (no --insecure flags)
        - Implement rate limiting for network operations
        - Log all remote repository access for audit trails
    - **File System Protection**:
        - Never overwrite files outside the project directory
        - Use atomic file operations to prevent corruption
        - Validate file permissions before installation
        - Prevent symlink attacks by resolving all paths
    - **Dependency Security**:
        - Check for known vulnerabilities in dependencies
        - Warn about outdated dependencies with security issues
        - Implement dependency pinning via lockfile
        - Detect and prevent circular dependencies
- **PLATFORMS: Windows, macOS, Linux verified**

## Example ccpm.toml Format

```toml
[sources]
community = "https://github.com/aig787/ccpm-community.git"
local = "../my-local-resources"  # Local directory support

[agents]
example-agent = { source = "community", path = "agents/example.md", version = "v1.0.0" }
local-agent = { path = "../local-agents/helper.md" }  # Direct local path

[snippets]
example-snippet = { source = "community", path = "snippets/example.md", version = "v1.2.0" }

[commands]
deployment = { source = "community", path = "commands/deploy.md", version = "v2.0.0" }

[scripts]
build-script = { source = "community", path = "scripts/build.sh", version = "v1.0.0" }
test-runner = { source = "local", path = "scripts/test.js" }

[hooks]
pre-commit = { source = "community", path = "hooks/pre-commit.json", version = "v1.0.0" }
user-prompt-submit = { source = "local", path = "hooks/user-prompt-submit.json" }

[mcp-servers]
filesystem = { source = "community", path = "mcp-servers/filesystem.json", version = "v1.0.0" }
postgres = { source = "local", path = "mcp-servers/postgres.json" }
```

## Example ccpm.lock Format

```toml
# Auto-generated lockfile - DO NOT EDIT
version = 1

[[sources]]
name = "community"
url = "https://github.com/aig787/ccpm-community.git"
commit = "def456..."
fetched_at = "2024-01-01T00:00:00Z"

[[agents]]
name = "example-agent"
source = "community"
url = "https://github.com/aig787/ccpm-community.git"
path = "agents/example.md"
version = "v1.0.0"
resolved_commit = "abc123..."
checksum = "sha256:..."
installed_at = ".claude/agents/example-agent.md"

[[snippets]]
name = "example-snippet"
source = "community"
url = "https://github.com/aig787/ccpm-community.git"
path = "snippets/example.md"
version = "v1.2.0"
resolved_commit = "abc123..."
checksum = "sha256:..."
installed_at = ".claude/ccpm/snippets/example-snippet.md"

[[commands]]
name = "deployment"
source = "community"
url = "https://github.com/aig787/ccpm-community.git"
path = "commands/deploy.md"
version = "v2.0.0"
resolved_commit = "abc123..."
checksum = "sha256:..."
installed_at = ".claude/commands/deployment.md"

[[scripts]]
name = "build-script"
source = "community"
url = "https://github.com/aig787/ccpm-community.git"
path = "scripts/build.sh"
version = "v1.0.0"
resolved_commit = "abc123..."
checksum = "sha256:..."
installed_at = ".claude/ccpm/scripts/build-script.sh"

[[hooks]]
name = "pre-commit"
source = "community"
url = "https://github.com/aig787/ccpm-community.git"
path = "hooks/pre-commit.json"
version = "v1.0.0"
resolved_commit = "abc123..."
checksum = "sha256:..."
installed_at = ".claude/ccpm/hooks/pre-commit.json"

[[mcp-servers]]
name = "filesystem"
source = "community"
url = "https://github.com/aig787/ccpm-community.git"
path = "mcp-servers/filesystem.json"
version = "v1.0.0"
resolved_commit = "abc123..."
checksum = "sha256:..."
installed_at = ".claude/ccpm/mcp-servers/filesystem.json"
```

## Global Configuration (~/.ccpm/config.toml)

```toml
# Global sources with authentication (not committed to git)
[sources]
private = "https://oauth2:ghp_xxxx@github.com/yourcompany/private-ccpm.git"
```

### Source Priority

1. **Global sources** from ~/.ccpm/config.toml (loaded first, contain secrets)
2. **Local sources** from ccpm.toml (override global, committed to git)

This separation keeps authentication tokens out of version control while allowing teams to share project configurations.