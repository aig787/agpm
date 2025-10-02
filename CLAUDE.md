# CLAUDE.md - CCPM Project Context

**IMPORTANT**: This file must remain under 20,000 characters.

## Overview

CCPM (Claude Code Package Manager) is a Git-based package manager for Claude Code resources (agents, snippets, commands, scripts, hooks, MCP servers), written in Rust. It uses a lockfile model (ccpm.toml + ccpm.lock) like Cargo for reproducible installations from Git repositories.

## Architecture

- **Language**: Rust 2024 edition with async/await (Tokio)
- **Distribution**: Git-based, no central registry
- **Resources**: Markdown (.md), JSON (.json), executables (.sh/.js/.py)
- **Patterns**: Glob patterns for bulk installation (`agents/*.md`)
- **Platforms**: Windows, macOS, Linux with full path support
- **Parallelism**: Git worktrees for safe concurrent operations
- **Concurrency**: Command-level parallelism (default: max(10, 2 × CPU cores))

## Key Modules

```
src/
├── cli/         # Command implementations (install, update, outdated, upgrade, etc.)
├── cache/       # Instance-level caching + worktree management
├── config/      # Global/project config
├── core/        # Error handling, resources
├── git/         # Git CLI wrapper + worktrees
├── hooks/       # Claude Code hooks
├── installer.rs # Parallel resource installation + artifact cleanup
├── lockfile/    # ccpm.lock management + staleness detection
├── manifest/    # ccpm.toml parsing
├── markdown/    # Markdown file operations
├── mcp/         # MCP server management
├── models/      # Data models
├── pattern.rs   # Glob pattern resolution
├── resolver/    # Dependency + version resolution
│   ├── mod.rs          # Core resolution + relative path handling
│   ├── redundancy.rs   # Redundant dependency detection
│   ├── version_resolution.rs  # Version constraint handling
│   └── version_resolver.rs    # Centralized SHA resolution
├── source/      # Source repository management
├── test_utils/  # Test infrastructure
├── upgrade/     # Self-update functionality
│   ├── mod.rs          # Upgrade orchestration
│   ├── self_updater.rs # Binary update logic
│   ├── version_check.rs # Version comparison
│   ├── backup.rs       # Backup management
│   ├── config.rs       # Update configuration
│   ├── verification.rs # Checksum verification
│   └── tests.rs        # Upgrade tests
├── utils/       # Cross-platform utilities + progress management
├── version/     # Version constraints + semver parsing
└── tests/       # Integration tests
```

## Rust Agents

### Standard (Fast)
- `rust-expert-standard`: Implementation, refactoring
- `rust-linting-standard`: Formatting, clippy
- `rust-doc-standard`: Documentation
- `rust-test-standard`: Test fixes
- `rust-troubleshooter-standard`: Debugging

### Advanced (Complex)
- `rust-expert-advanced`: Architecture, optimization
- `rust-linting-advanced`: Complex refactoring
- `rust-doc-advanced`: Architectural docs
- `rust-test-advanced`: Property testing, fuzzing
- `rust-troubleshooter-advanced`: Memory, UB

**Always use Task tool to delegate to agents.**

## Commands

- `/commit`: Git commit with conventional messages
- `/lint`: Format and clippy (--all-targets)
- `/pr-self-review`: PR analysis with commit range support
- `/update-all`: Update all docs in parallel
- `/update-claude`: Update CLAUDE.md (max 20k chars)
- `/update-docstrings`: Update Rust docstrings
- `/update-docs`: Update README and docs/
- `/execute`: Execute saved commands
- `/checkpoint`: Create development checkpoints with git stash
- `/squash`: Interactive commit squashing with code analysis

## CLI Commands

- `install [--frozen] [--no-cache] [--max-parallel N]` - Install from ccpm.toml
- `update [dep]` - Update dependencies
- `outdated [--check] [--no-fetch] [--format json]` - Check for dependency updates
- `upgrade [--check] [--status] [--force] [--rollback] [--no-backup] [VERSION]` - Self-update CCPM
- `list` - List installed resources
- `validate [--check-lock] [--resolve]` - Validate manifest
- `cache [clean|list]` - Manage cache
- `config [get|set]` - Global config
- `add [source|dep]` - Add to manifest
- `remove [source|dep]` - Remove from manifest
- `init [--path]` - Initialize project

## Development

- Use `Result<T, E>` for errors
- Test on Windows, macOS, Linux
- `cargo fmt && cargo clippy && cargo nextest run && cargo test --doc`
- Handle paths cross-platform
- **Note**: `cargo clippy --fix` requires `--allow-dirty` flag when there are uncommitted changes
- **Docstrings**: Use `no_run` attribute for code examples by default unless they should be executed as tests; use `ignore` for examples that won't compile

## Dependencies

Main: clap, tokio, toml, serde, serde_json, serde_yaml, anyhow, thiserror, colored, dirs, tracing, tracing-subscriber, indicatif, tempfile, semver, shellexpand, which, uuid, chrono, walkdir, sha2, hex, regex, futures, fs4, glob, once_cell, dashmap (v6.1)

Dev: assert_cmd, predicates

## Testing

- **Uses cargo nextest** for faster, parallel test execution
- **Auto-installs tools**: Makefile uses cargo-binstall for faster tool installation
- Run tests: `cargo nextest run` (integration/unit tests) + `cargo test --doc` (doctests)
- **Doc tests timing**: Doc tests can take up to 10 minutes to complete due to 432+ tests
- **All tests must be parallel-safe** - no serial_test usage
- Never use `std::env::set_var` (causes races)
- Each test gets own temp directory
- Use `tokio::fs` in async tests
- Default parallelism: max(10, 2 × CPU cores)
- 70% coverage target
- **IMPORTANT**: When running commands via Bash tool, they run in NON-TTY mode. The user sees TTY mode with spinners. Test both modes.
- **CRITICAL**: Never include "update" in integration test filenames (triggers Windows UAC elevation)

## Build & CI

```bash
cargo build --release  # Optimized with LTO
cargo fmt && cargo clippy -- -D warnings && cargo nextest run && cargo test --doc
```

GitHub Actions: Cross-platform tests, semantic-release, crates.io publish


## Key Design Decisions

- **Copy files** instead of symlinks (better compatibility)
- **Atomic operations** (temp file + rename)
- **Async I/O** with tokio::fs
- **System git** command (no git2 library)
- **SHA-based worktrees** (v0.3.2+): One worktree per unique commit
- **Centralized VersionResolver**: Batch SHA resolution with automatic deduplication
- **Upfront SHA resolution**: All versions resolved before any checkouts
- **Direct concurrency control**: Command parallelism via --max-parallel + per-worktree locking
- **Instance-level caching** with WorktreeState enum (Pending/Ready)
- **Command-level parallelism** via --max-parallel (default: max(10, 2 × CPU cores))
- **Single fetch per repo per command**: Command-instance fetch tracking
- **Enhanced dependency parsing** with manifest context for better local vs Git detection
- **Lockfile staleness detection** (v0.3.17): Automatic detection and handling of stale lockfiles
- **Self-update capability** (v0.3.15+): Platform-specific binary updates with backup/rollback
- **Relative path preservation** (v0.3.18+): Maintains source directory structure, uses basename from path not dependency name
- **Automatic artifact cleanup** (v0.3.18+): Removes old files when paths change, cleans empty directories
- **Custom target behavior** (v0.3.18+): BREAKING - Custom targets now relative to default resource directory

## Breaking Changes (v0.3.18+)

### Custom Target Behavior
- **Old**: Custom targets were relative to project root
- **New**: Custom targets are relative to default resource directory (e.g., `.claude/agents/`)
- **Migration**: Update custom targets to use paths relative to resource directory, not project root
- **Rationale**: Prevents naming conflicts between different resource types

## Resolver Architecture

The resolver uses a sophisticated two-phase approach:

1. **Collection Phase**: `VersionResolver` gathers all unique (source, version) pairs
2. **Resolution Phase**: Batch resolves versions to SHAs, with automatic deduplication

Key benefits:
- Minimizes Git operations through batching
- Enables parallel resolution across different sources
- Automatic deduplication of identical commit references
- Supports semver constraint resolution (`^1.0`, `~2.1`, etc.)



## Windows Path Gotchas

- Absolute paths: `C:\path` or `\\server\share`
- file:// URLs use forward slashes
- Reserved names: CON, PRN, AUX, NUL, COM1-9, LPT1-9
- Test on real Windows (not WSL)



## Optimized Worktree Architecture

Cache uses Git worktrees with SHA-based resolution for maximum efficiency:

```
~/.ccpm/cache/
├── sources/        # Bare repositories (.git suffix)
│   └── github_owner_repo.git/
├── worktrees/      # SHA-based worktrees (deduplicated)
│   └── github_owner_repo_abc12345/  # First 8 chars of commit SHA
└── .locks/         # File-based locks for safety
```

### SHA-Based Resolution (v0.3.2+)

- **Centralized `VersionResolver`**: Batch resolves all versions to SHAs upfront
- **Two-phase resolution**: Collection phase → Resolution phase for efficiency
- **Single clone per repo**: Bare repository shared by all operations
- **Single fetch per command**: Command-instance fetch caching prevents redundant network ops
- **SHA resolution upfront**: All versions resolved to commits before any checkout
- **SHA-keyed worktrees**: One worktree per unique commit (not per version)
- **Maximum reuse**: Tags/branches pointing to same commit share one worktree
- **Instance-level cache**: WorktreeState (Pending/Ready) tracks creation status
- **Per-worktree locks**: Fine-grained locking for parallel operations
- **Version constraint resolution**: Supports semver constraints like "^1.0", "~2.1"
- **Automatic deduplication**: Multiple refs to same commit automatically share resources

## Key Requirements

- **Use Task tool** for complex operations
- **Cross-platform**: Windows, macOS, Linux
- **NO git2**: Use system git command
- **Security**: Credentials only in ~/.ccpm/config.toml, path traversal prevention, checksums
- **Atomic ops**: Temp file + rename
- **Resources**: .md, .json, .sh/.js/.py files
- **Hooks**: Configure in .claude/settings.local.json
- **MCP**: Configure in .mcp.json

## Example ccpm.toml Format

```toml
[sources]
community = "https://github.com/aig787/ccpm-community.git"
local = "../my-local-resources"  # Local directory support

[agents]
# Single file dependency - installs as .claude/agents/example.md (basename from path)
example-agent = { source = "community", path = "agents/example.md", version = "v1.0.0" }
local-agent = { path = "../local-agents/helper.md" }  # Direct local path

# Nested paths preserve directory structure (v0.3.18+)
# "agents/ai/gpt.md" → ".claude/agents/ai/gpt.md" (preserves ai/ subdirectory)
ai-helper = { source = "community", path = "agents/ai/gpt.md", version = "v1.0.0" }

# Pattern-based dependencies (glob patterns in path field)
ai-agents = { source = "community", path = "agents/ai/*.md", version = "v1.0.0" }  # All AI agents
review-tools = { source = "community", path = "agents/**/review*.md", version = "v1.0.0" }  # All review agents recursively

# Custom target (v0.3.18+ BREAKING: relative to .claude/agents/, not project root)
special = { source = "community", path = "agents/special.md", target = "custom/special.md" }

[snippets]
example-snippet = { source = "community", path = "snippets/example.md", version = "v1.2.0" }
# Pattern for all Python snippets
python-snippets = { source = "community", path = "snippets/python/*.md", version = "v1.0.0" }

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

## Example ccpm.lock

```toml
# Auto-generated lockfile
[[agents]]
name = "example-agent"
source = "community"
path = "agents/example.md"
version = "v1.0.0"
resolved_commit = "abc123..."
checksum = "sha256:..."
installed_at = ".claude/agents/example.md"  # Uses basename from path (v0.3.18+)

[[agents]]
name = "ai-helper"
source = "community"
path = "agents/ai/gpt.md"
version = "v1.0.0"
resolved_commit = "abc123..."
checksum = "sha256:..."
installed_at = ".claude/agents/ai/gpt.md"  # Preserves subdirectory structure
# Similar format for snippets, commands, scripts, hooks, mcp-servers
```

## Config Priority

1. `~/.ccpm/config.toml` - Global config with auth tokens (not in git)
2. `ccpm.toml` - Project manifest (in git)

Keeps secrets out of version control.
