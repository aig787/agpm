# CLAUDE.md - CCPM Project Context

**IMPORTANT**: This file must remain under 20,000 characters.

## Overview

CCPM (Claude Code Package Manager) is a Git-based package manager for Claude Code resources (agents, snippets, commands, scripts, hooks, MCP servers), written in Rust. It uses a lockfile model (ccpm.toml + ccpm.lock) like Cargo for reproducible installations from Git repositories.

## Architecture

- **Language**: Rust with async/await (Tokio)
- **Distribution**: Git-based, no central registry
- **Resources**: Markdown (.md), JSON (.json), executables (.sh/.js/.py)
- **Patterns**: Glob patterns for bulk installation (`agents/*.md`)
- **Platforms**: Windows, macOS, Linux with full path support

## Key Modules

```
src/
├── cli/         # Command implementations
├── cache/       # Git cache management
├── config/      # Global/project config
├── core/        # Error handling, resources
├── git/         # Git CLI wrapper
├── hooks/       # Claude Code hooks
├── installer.rs # Resource installation
├── lockfile/    # ccpm.lock management
├── manifest/    # ccpm.toml parsing
├── pattern.rs   # Glob pattern resolution
├── resolver/    # Dependency resolution
├── version/     # Version constraints
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
- `/lint`: Format and clippy
- `/pr-self-review`: PR analysis
- `/update-all`: Update all docs
- `/update-claude`: Update CLAUDE.md (max 20k chars)
- `/update-docstrings`: Update Rust docstrings
- `/update-docs`: Update README and docs/

## CLI Commands

- `install [--frozen] [--no-cache]` - Install from ccpm.toml
- `update [dep]` - Update dependencies
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
- `cargo fmt && cargo clippy && cargo test`
- Handle paths cross-platform

## Dependencies

Main: clap, tokio, toml, serde, anyhow, thiserror, colored, dirs, indicatif, tempfile, shellexpand, which, uuid, chrono, walkdir, sha2, hex, regex, futures, fs4, glob

Dev: assert_cmd, predicates

## Testing

- Parallel-safe tests (no WorkingDirGuard)
- Never use `std::env::set_var` (causes races)
- Each test gets own temp directory
- Use `tokio::fs` in async tests
- 70% coverage target

## Build & CI

```bash
cargo build --release  # Optimized with LTO
cargo fmt && cargo clippy -- -D warnings && cargo test
```

GitHub Actions: Cross-platform tests, semantic-release, crates.io publish


## Key Design Decisions

- **Copy files** instead of symlinks (better compatibility)
- **Atomic operations** (temp file + rename)
- **Async I/O** with tokio::fs
- **Parallel tests** without WorkingDirGuard
- **System git** command (no git2 library)



## Windows Path Gotchas

- Absolute paths: `C:\path` or `\\server\share`
- file:// URLs use forward slashes
- Reserved names: CON, PRN, AUX, NUL, COM1-9, LPT1-9
- Test on real Windows (not WSL)



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
# Single file dependency
example-agent = { source = "community", path = "agents/example.md", version = "v1.0.0" }
local-agent = { path = "../local-agents/helper.md" }  # Direct local path

# Pattern-based dependencies (glob patterns in path field)
ai-agents = { source = "community", path = "agents/ai/*.md", version = "v1.0.0" }  # All AI agents
review-tools = { source = "community", path = "agents/**/review*.md", version = "v1.0.0" }  # All review agents recursively

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
installed_at = ".claude/agents/example-agent.md"
# Similar format for snippets, commands, scripts, hooks, mcp-servers
```

## Config Priority

1. `~/.ccpm/config.toml` - Global config with auth tokens (not in git)
2. `ccpm.toml` - Project manifest (in git)

Keeps secrets out of version control.