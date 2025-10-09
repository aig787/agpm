# CLAUDE.md - AGPM Project Context

**IMPORTANT**: This file must remain under 20,000 characters.

## Overview

AGPM (Claude Code Package Manager) is a Git-based package manager for AI coding assistant resources (agents, snippets, commands,
scripts, hooks, MCP servers), written in Rust. It supports multiple tools (Claude Code, OpenCode, custom types) via a pluggable
artifact system. Uses a lockfile model (agpm.toml + agpm.lock) like Cargo for reproducible installations from Git repositories.

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
├── lockfile/    # agpm.lock management + staleness detection
├── manifest/    # agpm.toml parsing + transitive dependencies
│   └── dependency_spec.rs  # DependencySpec and DependencyMetadata structures
├── markdown/    # Markdown file operations
├── mcp/         # MCP server management
│   └── handlers.rs  # Pluggable MCP handlers (ClaudeCode, OpenCode)
├── metadata/    # Metadata extraction from resource files
│   └── extractor.rs  # YAML frontmatter and JSON field extraction
├── models/      # Data models
├── pattern.rs   # Glob pattern resolution
├── resolver/    # Dependency + version resolution
│   ├── mod.rs                # Core resolution logic with transitive support + relative path handling
│   ├── dependency_graph.rs   # Graph-based transitive dependency resolution
│   ├── version_resolution.rs # Version constraint handling
│   └── version_resolver.rs   # Centralized SHA resolution
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

- `install [--frozen] [--no-cache] [--max-parallel N]` - Install from agpm.toml
- `update [dep]` - Update dependencies
- `outdated [--check] [--no-fetch] [--format json]` - Check for dependency updates
- `upgrade [--check] [--status] [--force] [--rollback] [--no-backup] [VERSION]` - Self-update AGPM
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
- **Docstrings**: Use `no_run` attribute for code examples by default unless they should be executed as tests; use
  `ignore` for examples that won't compile

## Dependencies

Main: clap, tokio, toml, serde, serde_json, serde_yaml, anyhow, thiserror, colored, dirs, tracing, tracing-subscriber,
indicatif, tempfile, semver, shellexpand, which, uuid, chrono, walkdir, sha2, hex, regex, futures, fs4, glob, once_cell,
dashmap (v6.1)

Dev: assert_cmd, predicates

## Testing

- **Uses cargo nextest** for faster, parallel test execution
- **Auto-installs tools**: Makefile uses cargo-binstall for faster tool installation
- Run tests: `cargo nextest run` (integration/unit tests) + `cargo test --doc` (doctests)
- **All tests must be parallel-safe** - no serial_test usage
- Never use `std::env::set_var` (causes races)
- Each test gets own temp directory
- Use `tokio::fs` in async tests
- Default parallelism: max(10, 2 × CPU cores)
- 70% coverage target
- **IMPORTANT**: When running commands via Bash tool, they run in NON-TTY mode. The user sees TTY mode with spinners.
  Test both modes.
- **CRITICAL**: Never include "update" in integration test filenames (triggers Windows UAC elevation)
- **CRITICAL**: Always use `TestProject` and `TestGit` helpers from `tests/common/mod.rs` for integration tests. Never manually configure git repos with raw `std::process::Command`. TestProject provides `sources_path()` for creating test git repos.
- **CRITICAL**: Don't manually create lockfiles in tests with hardcoded paths. Let `agpm install` generate them from the manifest. Manual lockfiles break on Windows due to path separator mismatches.

## Build & CI

```bash
cargo build --release  # Optimized with LTO
cargo fmt && cargo clippy -- -D warnings && cargo nextest run && cargo test --doc
```

GitHub Actions: Cross-platform tests, semantic-release, crates.io publish

**cargo-dist**: Uses `dist` command (NOT `cargo dist`). The binary is named `dist` in `~/.cargo/bin/`.

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
- **Multi-tool support** (v0.4.0+): Pluggable tools (claude-code, opencode, agpm, custom)
- **Tool-aware path resolution**: Resources install to tool-specific directories
- **Pluggable MCP handlers**: Tool-specific MCP server configuration (ClaudeCode, OpenCode)
- **Transitive dependencies**: Resources declare dependencies via YAML frontmatter or JSON
- **Graph-based resolution**: Dependency graph with cycle detection and topological ordering
- **Versioned prefixes** (v0.3.19+): Support for monorepo-style prefixed tags (e.g., `agents-v1.0.0`) with prefix-aware constraint matching

## Breaking Changes (v0.3.18+)

### Custom Target Behavior
- **Old**: Custom targets were relative to project root
- **New**: Custom targets are relative to default resource directory (e.g., `.claude/agents/`)
- **Migration**: Update custom targets to use paths relative to resource directory, not project root
- **Rationale**: Prevents naming conflicts between different resource types

## Resolver Architecture

The resolver uses a sophisticated multi-phase approach with transitive dependency support:

1. **Collection Phase**: `VersionResolver` gathers all unique (source, version) pairs
2. **Resolution Phase**: Batch resolves versions to SHAs, with automatic deduplication
3. **Transitive Phase**: Extracts and resolves dependencies declared in resource files

Key benefits:

- Minimizes Git operations through batching
- Enables parallel resolution across different sources
- Automatic deduplication of identical commit references
- Supports semver constraint resolution (`^1.0`, `~2.1`, etc.)
- Graph-based transitive dependency resolution with cycle detection

### Transitive Dependencies

Resources can declare dependencies within their files:

**Markdown files** (YAML frontmatter):

```yaml
---
dependencies:
  agents:
    - path: agents/helper.md
      version: v1.0.0
  snippets:
    - path: snippets/utils.md
---
```

**JSON files** (top-level field):

```json
{
  "dependencies": {
    "commands": [
      {
        "path": "commands/deploy.md",
        "version": "v2.0.0"
      }
    ]
  }
}
```

**Key Features**:

- Graph-based resolution with topological ordering
- Cycle detection prevents infinite loops
- Version inheritance when not specified
- Same-source dependency model (inherits parent's source)
- Parallel resolution for maximum efficiency

## Versioned Prefixes (v0.3.19+)

AGPM supports monorepo-style versioned prefixes, allowing independent semantic versioning for different components within a single repository.

### Syntax

Tags and constraints can include optional prefixes separated from the version by a hyphen:
- `agents-v1.0.0` - Prefixed tag
- `agents-^v1.0.0` - Prefixed constraint
- `my-tool-v2.0.0` - Multi-hyphen prefix

### Prefix Isolation

Prefixes create isolated version namespaces:
- `agents-^v1.0.0` matches only `agents-v*` tags, not `snippets-v*` or unprefixed `v*`
- Unprefixed constraints like `^v1.0.0` only match unprefixed tags
- Different prefixes never conflict with each other

### Examples

```toml
[agents]
# Prefixed constraint - matches agents-v1.x.x tags only
ai-helper = { source = "community", path = "agents/ai/gpt.md", version = "agents-^v1.0.0" }

# Different prefix - independent versioning
code-helper = { source = "community", path = "agents/code/helper.md", version = "snippets-^v2.0.0" }

# Unprefixed - traditional versioning
standard = { source = "community", path = "agents/standard.md", version = "^v1.0.0" }
```

## Cross-Platform Path Handling

**CRITICAL**: AGPM must work identically on Windows, macOS, and Linux.

### Path Separator Rules

1. **Forward slashes ONLY** in these contexts:
   - Lockfile `installed_at` fields (cross-platform consistency)
   - `.gitignore` entries (Git requirement)
   - TOML manifest files (platform-independent)
   - Any serialized/stored path representation

2. **Use `Path::display()` carefully**:
   - `Path::display()` produces platform-specific separators (backslashes on Windows)
   - Always normalize with `.replace('\\', "/")` when storing paths
   - Example: `format!("{}/{}", artifact_path.display(), filename).replace('\\', "/")`

3. **Runtime path operations**:
   - Use `Path`/`PathBuf` for filesystem operations (automatic platform handling)
   - Only convert to strings when storing/serializing
   - Use `join()` instead of string concatenation

### Testing Path Handling

- **Integration tests must pass on Windows**: CI runs all tests on Windows, macOS, Linux
- **Don't hardcode path separators in test assertions**: Use forward slashes in expected values
- **TestProject helper handles paths correctly**: Always use `TestProject::new()` in tests
- **Don't manually create lockfiles in tests**: Let `agpm install` generate them naturally

### Windows-Specific Gotchas

- Absolute paths: `C:\path` or `\\server\share`
- file:// URLs use forward slashes (even on Windows)
- Reserved names: CON, PRN, AUX, NUL, COM1-9, LPT1-9
- Test on real Windows (not WSL)

## Optimized Worktree Architecture

Cache uses Git worktrees with SHA-based resolution for maximum efficiency:

```
~/.agpm/cache/
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

## Multi-Tool Support

AGPM supports multiple AI coding tools via configurable tools:

### Supported Tools

- **claude-code** (default): Claude Code resources (agents, commands, scripts, hooks, MCP servers)
- **opencode**: OpenCode resources (agents, commands, MCP servers)
- **agpm**: AGPM-specific resources (snippets for reusable templates)
- **custom**: User-defined tools via configuration

### Tool Configuration

Each tool defines:
- **Base directory**: Where resources are installed (e.g., `.claude`, `.opencode`)
- **Resource paths**: Subdirectories for each resource type
- **MCP handling**: Tool-specific MCP server configuration strategy

### Dependency Tool Field

Dependencies can specify their target tool:

```toml
[agents]
# Defaults to claude-code
example = { source = "community", path = "agents/example.md", version = "v1.0.0" }

# Explicit type for OpenCode
opencode-agent = { source = "community", path = "agents/helper.md", tool = "opencode" }
```

### Resource Type Support Matrix

| Resource      | claude-code | opencode | agpm | Default Type |
|---------------|-------------|----------|------|--------------|
| agents        | ✅ `.claude/agents/` | ✅ `.opencode/agent/` (singular) | ❌ | `claude-code` |
| commands      | ✅ `.claude/commands/` | ✅ `.opencode/command/` (singular) | ❌ | `claude-code` |
| scripts       | ✅ `.claude/scripts/` | ❌ | ❌ | `claude-code` |
| hooks         | ✅ `.claude/hooks/` | ❌ | ❌ | `claude-code` |
| mcp-servers   | ✅ → `.mcp.json` | ✅ → `opencode.json` | ❌ | `claude-code` |
| snippets      | ✅ `.claude/agpm/snippets/` | ❌ | ✅ `.agpm/snippets/` | **`agpm`** |

**Note**: Snippets default to `agpm` tool (shared infrastructure). Use `tool = "claude-code"` to override.

### MCP Handler System

Pluggable handlers for tool-specific MCP configuration:

- **ClaudeCodeMcpHandler**: Copies to `.claude/agpm/mcp-servers/`, merges into `.mcp.json`
- **OpenCodeMcpHandler**: Copies to `.opencode/agpm/mcp-servers/`, merges into `opencode.json`
- **Tracking**: Uses `_agpm` metadata to distinguish managed vs user servers

## Key Requirements

- **Use Task tool** for complex operations
- **Cross-platform**: Windows, macOS, Linux
- **NO git2**: Use system git command
- **Security**: Credentials only in ~/.agpm/config.toml, path traversal prevention, checksums
- **Atomic ops**: Temp file + rename
- **Resources**: .md, .json, .sh/.js/.py files
- **Hooks**: Configure in .claude/settings.local.json
- **MCP**: Configure in .mcp.json

## Example agpm.toml Format

```toml
[sources]
community = "https://github.com/aig787/agpm-community.git"
local = "../my-local-resources"  # Local directory support

# Tool type configurations (optional - uses defaults if omitted)
[tools.claude-code]
path = ".claude"
resources = { agents = { path = "agents" }, commands = { path = "commands" }, scripts = { path = "scripts" }, hooks = { path = "hooks" }, mcp-servers = { path = "agpm/mcp-servers" }, snippets = { path = "agpm/snippets" } }

[tools.opencode]
path = ".opencode"
resources = { agents = { path = "agent" }, commands = { path = "command" }, mcp-servers = { path = "agpm/mcp-servers" } }

[tools.agpm]
path = ".agpm"
resources = { snippets = { path = "snippets" } }

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

# OpenCode agent (installs to .opencode/agent/)
opencode-helper = { source = "community", path = "agents/helper.md", version = "v1.0.0", tool = "opencode" }

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

## Example agpm.lock

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
tool = "claude-code"  # Defaults to claude-code, omitted if default

[[agents]]
name = "opencode-helper"
source = "community"
path = "agents/helper.md"
version = "v1.0.0"
resolved_commit = "abc123..."
checksum = "sha256:..."
installed_at = ".opencode/agent/helper.md"  # OpenCode uses singular "agent"
tool = "opencode"

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

1. `~/.agpm/config.toml` - Global config with auth tokens (not in git)
2. `agpm.toml` - Project manifest (in git)

Keeps secrets out of version control.
