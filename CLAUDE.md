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
- **Resource Format**: Markdown files (.md) for agents, snippets, and commands
- **MCP Servers**: Configuration management via .mcp.json (shared with user configs)
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
│   ├── core/             # Core functionality
│   ├── git/              # Git CLI wrapper
│   ├── manifest/         # Manifest (ccpm.toml) parsing
│   ├── lockfile/         # Lockfile (ccpm.lock) management
│   ├── markdown/         # Markdown file operations
│   ├── resolver/         # Dependency resolution
│   ├── source/           # Source repository operations
│   ├── version/          # Version constraint handling
│   ├── config/           # Project configuration
│   └── utils/            # Cross-platform utilities
├── tests/                # Integration tests
├── Cargo.toml           # Project manifest
├── README.md            # User-facing documentation
├── IMPLEMENTATION_PLAN.md # Paradigm shift implementation plan
└── CLAUDE.md            # This file
```

## Core Commands

1. `install` - Install dependencies from ccpm.toml, generate/update ccpm.lock
2. `update` - Update dependencies within version constraints
3. `list` - List installed resources from ccpm.lock
4. `validate` - Validate ccpm.toml syntax and source availability
5. `cache` - Manage global git cache (~/.ccpm/cache/)
6. `config` - Manage global configuration (~/.ccpm/config.toml)
7. `mcp` - Manage MCP server configurations (list, clean, status)
8. `add` - Add sources and dependencies to ccpm.toml manifest
9. `init` - Initialize new CCPM project with ccpm.toml

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
- `tokio` (1.40) - Async runtime with full features
- `toml` (0.8) - TOML parsing and serialization
- `serde` (1.0) - Serialization framework with derive
- `serde_json` (1.0) - JSON support for metadata
- `semver` (1.0) - Semantic version parsing for git tags
- `anyhow` (1.0) - Error handling with context
- `thiserror` (1.0) - Custom error types with derive
- `colored` (2.1) - Terminal colors for CLI output
- `dirs` (5.0) - Platform-specific directory paths
- `indicatif` (0.17) - Progress bars and spinners
- `tempfile` (3.10) - Temporary file/directory management
- `shellexpand` (3.1) - Shell-like path expansion (~, env vars)
- `which` (6.0) - Command detection in PATH
- `uuid` (1.10) - Unique identifier generation

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

## Target Module Structure

```
src/
├── main.rs              # Async entry point with error handling
├── cli/                 # Simplified command set
│   ├── install.rs      # Install from ccpm.toml
│   ├── update.rs       # Update within constraints
│   ├── list.rs         # List from ccpm.lock
│   ├── validate.rs     # Validate ccpm.toml
│   └── mcp.rs          # MCP server management commands (NEW)
├── manifest/           # Manifest management (NEW)
│   └── mod.rs          # Parse ccpm.toml, handle dependencies
├── lockfile/           # Lockfile management (UPDATED)
│   └── mod.rs          # Generate/parse ccpm.lock, track MCP servers
├── markdown/           # Markdown operations (NEW)
│   └── mod.rs          # Read/write .md files, extract metadata
├── mcp/                # MCP server management (NEW)
│   └── mod.rs          # Manage .mcp.json configurations
├── resolver/           # Dependency resolution (NEW)
│   └── mod.rs          # Resolve versions, detect conflicts
├── config/             # Project configuration (SIMPLIFIED)
│   └── project.rs      # Project-specific settings only
├── core/               # Core types and error handling
│   ├── error.rs        # Error types (updated)
│   └── resource.rs     # Resource traits and types (includes McpServer)
├── git/                # Git integration (SIMPLIFIED)
│   └── mod.rs          # Git CLI wrapper using auth from global config
├── source/             # Source operations (REFACTORED)
│   └── mod.rs          # Clone/cache sources from manifest
├── version/            # Version handling (SIMPLIFIED)
│   └── mod.rs          # Version constraint matching
└── utils/              # Cross-platform utilities
    ├── fs.rs           # File operations, atomic writes
    ├── platform.rs     # Platform-specific helpers
    └── progress.rs     # Progress bars and spinners
```

## Implementation Lessons Learned

### Architecture Decisions That Worked Well

1. **Modular structure** - Each module has clear responsibilities
2. **Error context pattern** - ErrorContext struct provides suggestions and details
3. **Resource trait abstraction** - Allows easy extension for new resource types
4. **Atomic file operations** - Write to temp file then rename for safety
5. **Platform-specific code isolation** - Using cfg! macros and separate functions

### Design Decision: Copy-Based Installation

CCPM copies files from the cache to project directories rather than using symlinks. This decision provides:

- **Maximum compatibility** across Windows, macOS, and Linux without special permissions
- **Git-friendly** behavior since real files can be tracked and committed
- **Editor compatibility** with no symlink confusion
- **User flexibility** to edit local files without affecting the cache

### Key Implementation Details

1. **Dependency Management**: Manifest (ccpm.toml) + Lockfile (ccpm.lock)
2. **Resource Format**: Markdown files with optional frontmatter metadata
3. **Source Resolution**: Named sources in manifest, cloned/cached locally
4. **Version Constraints**: Support tags, branches, and specific commits
5. **Installation**: Copy .md files from cache to project locations
6. **MCP Servers**: Configure in .mcp.json, preserve user-managed entries
7. **Path handling**: Always use absolute paths internally, normalize separators
8. **Windows considerations**: Handle long paths (>260 chars), different git command
9. **Global Config**: ~/.ccpm/config.toml for auth tokens and private sources
10. **Cache Architecture**: ~/.ccpm/cache/ for cloned repositories
11. **MCP Metadata**: Track CCPM-managed servers with _ccpm field in .mcp.json

### Testing Insights

1. **Integration tests are crucial** - Test actual CLI invocations
2. **Tempdir for file tests** - Use tempfile crate for isolated testing
3. **Platform-specific tests** - Use cfg(test) with platform conditions
4. **Test infrastructure** - Git daemon for realistic repository testing

## Notes for Claude

- Focus on idiomatic Rust code
- Prioritize error handling and user-friendly error messages
- Keep the CLI interface simple and intuitive
- CRITICAL: Ensure cross-platform compatibility (Windows, macOS, Linux)
- Use async/await for potentially long-running operations (Git clones, etc.)
- Implement progress indicators for long operations
- Support multiple resource types (agents, snippets, commands, MCP servers)
- MCP servers are configured, not installed as files
- Preserve user-managed entries in .mcp.json (non-destructive updates)
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

[agents]
example-agent = { source = "community", path = "agents/example.md", version = "v1.0.0" }
local-agent = { path = "../local-agents/helper.md" }

[snippets]
example-snippet = { source = "community", path = "snippets/example.md", version = "v1.2.0" }

[commands]
deployment = { source = "community", path = "commands/deploy.md", version = "v2.0.0" }

[mcp-servers]
filesystem = { command = "npx", args = ["-y", "@modelcontextprotocol/server-filesystem"] }
postgres = { command = "mcp-postgres", args = ["--connection", "${DATABASE_URL}"] }
```

## Example ccpm.lock Format

```toml
# Auto-generated lockfile - DO NOT EDIT
version = 1

[[sources]]
name = "community"
url = "https://github.com/aig787/ccpm-community.git"
commit = "def456..."

[[agents]]
name = "example-agent"
source = "community"
path = "agents/example.md"
version = "v1.0.0"
resolved_commit = "abc123..."
checksum = "sha256:..."
installed_at = "agents/example-agent.md"
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