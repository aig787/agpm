# AGENTS.md - AGPM Project Context (Codex)

**IMPORTANT**: This file must remain under 20,000 characters.

This document mirrors the AGPM context from `CLAUDE.md` but focuses on architecture, workflows, and guardrails that are
especially relevant when contributing through Codex. AGPM is still the Claude Code Package Manager, so references to
`.claude/...` paths and Claude-specific resource types remain accurate; Codex contributors should respect them when
working in this repository.

## Overview

AGPM (Claude Code Package Manager) is a Git-based package manager for Claude Code resources (agents, snippets, commands,
scripts, hooks, MCP servers), written in Rust. It uses a lockfile model (agpm.toml + agpm.lock) like Cargo for
reproducible installations from Git repositories.

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
├── cli/         # Command implementations
├── cache/       # Instance-level caching + worktree management
├── config/      # Global/project config
├── core/        # Error handling, resources
├── git/         # Git CLI wrapper + worktrees
├── hooks/       # Hook integrations for Claude Code environments
├── installer.rs # Parallel resource installation
├── lockfile/    # agpm.lock management
├── manifest/    # agpm.toml parsing + transitive dependencies
│   └── dependency_spec.rs  # DependencySpec and DependencyMetadata structures
├── markdown/    # Markdown file operations
├── mcp/         # MCP server management
├── metadata/    # Metadata extraction from resource files
│   └── extractor.rs  # YAML frontmatter and JSON field extraction
├── models/      # Data models
├── pattern.rs   # Glob pattern resolution
├── resolver/    # Dependency + version resolution
│   ├── mod.rs                # Core dependency resolution logic with transitive support
│   ├── dependency_graph.rs   # Graph-based transitive dependency resolution
│   ├── version_resolution.rs # Version constraint handling
│   └── version_resolver.rs   # Centralized SHA resolution
├── source/      # Source repository management
├── test_utils/  # Test infrastructure
├── utils/       # Cross-platform utilities + progress management
├── version/     # Version constraints
└── tests/       # Integration tests
```

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

## Resource Authoring and Templating

When creating agents, snippets, or commands for AGPM, you can use Tera-based templating to create dynamic content that adapts during installation.

### Quick Start

Resources support template variables for metadata and dependencies:

```markdown
---
title: {{ agpm.resource.name }}
dependencies:
  snippets:
    - path: snippets/utils.md
      version: v1.0.0
---
# {{ agpm.resource.name }}

Version: {{ agpm.resource.version }}
Install path: `{{ agpm.resource.install_path }}`

{% if agpm.deps.snippets.utils %}
Uses helper: `{{ agpm.deps.snippets.utils.install_path }}`
{% endif %}
```

### Available Template Variables

- **`agpm.resource.*`** - Current resource metadata (name, version, install_path, source, checksum, etc.)
- **`agpm.deps.<category>.<name>.*`** - Dependency metadata for resources declared in frontmatter

**Full Variable Reference**: See [docs/templating.md](../docs/templating.md#template-variables-reference) for the complete variable table.

### Best Practices

1. **Use descriptive variable names** - Resource names become template variables (sanitized with underscores)
2. **Avoid hyphens in resource names** - Use underscores to prevent confusion (hyphens are converted to underscores in templates)
3. **Test with different dependency combinations** - Ensure conditionals work when dependencies are missing
4. **Keep templates simple** - Avoid complex logic for maintainability
5. **Use `{% raw %}...{% endraw %}` for literal template syntax** - When documenting template syntax itself

### Disabling Templating

To include literal template syntax (e.g., in documentation or examples), disable templating via frontmatter:

```markdown
---
agpm:
  templating: false
---
# This {{ template.syntax }} won't be processed
```

### Documentation

- **Complete Guide**: [docs/templating.md](../docs/templating.md) - Full syntax, examples, and troubleshooting
- **Resource Formats**: [docs/resources.md](../docs/resources.md#resource-frontmatter-and-templating) - Frontmatter structure

## Dependencies

Main: clap, tokio, toml, serde, serde_json, serde_yaml, anyhow, thiserror, colored, dirs, tracing, tracing-subscriber,
indicatif, tempfile, semver, shellexpand, which, uuid, chrono, walkdir, sha2, hex, regex, futures, fs4, glob, once_cell,
dashmap (v6.1), tera

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
- **CRITICAL**: Lockfiles (`agpm.lock`) must store manifest-relative paths only (no absolute paths) and always use Unix-style forward slashes so they remain portable across machines.

## Build & CI

```bash
cargo build --release  # Optimized with LTO
cargo fmt && cargo clippy -- -D warnings && cargo nextest run && cargo test --doc
```

GitHub Actions: Cross-platform tests, crates.io publish

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
- **Transitive dependencies**: Resources declare dependencies via YAML frontmatter or JSON
- **Graph-based resolution**: Dependency graph with cycle detection and topological ordering

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

### Conflict Detection

Conflict detection is integrated into the core resolution logic (`resolver::mod.rs`):

**Types Detected**:

- **Version conflicts**: Same resource with incompatible version constraints
- **Duplicate resources**: Resources resolved to same installation path

**Design**: Conflicts are detected during resolution phase and reported as errors, preventing ambiguous installations

## Windows Path Gotchas

- Absolute paths: `C:\\path` or `\\server\share`
- file:// URLs use forward slashes
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

## Key Requirements

- **Use Task tool** for complex operations
- **Cross-platform**: Windows, macOS, Linux
- **NO git2**: Use system git command
- **Security**: Credentials only in ~/.agpm/config.toml, path traversal prevention, checksums
- **Atomic ops**: Temp file + rename
- **Resources**: .md, .json, .sh/.js/.py files
- **Hooks**: Configure in .claude/settings.local.json
- **MCP**: Configure in .mcp.json

### Multi-Tool Support & Merge Targets

AGPM supports multiple AI coding tools (claude-code, opencode, agpm, custom) via configurable tools. Some resource types (hooks, MCP servers) don't install as individual files but merge into shared configuration files.

**Default Merge Targets**:
- **Hooks** (claude-code): `.claude/settings.local.json`
- **MCP Servers** (claude-code): `.mcp.json`
- **MCP Servers** (opencode): `.opencode/opencode.json`

**Custom Tool Configuration**:

```toml
# Override merge targets for custom tools
[tools.my-tool]
path = ".my-tool"

[tools.my-tool.resources.hooks]
merge-target = ".my-tool/hooks.json"

[tools.my-tool.resources.mcp-servers]
merge-target = ".my-tool/servers.json"

# Configure default tools per resource type
[default-tools]
snippets = "claude-code"  # Override default (agpm) for Claude-only users
agents = "opencode"       # Default agents to OpenCode
```

**Resource Configuration**:
- **`path`**: File-based resources (agents, snippets, commands, scripts) install as individual files
- **`merge-target`**: Config-based resources (hooks, MCP servers) merge into shared JSON files
- A resource type is supported if EITHER `path` OR `merge-target` is specified

**Note**: Custom tools require MCP handlers for hooks/MCP servers. Only built-in tools (claude-code, opencode) have handlers.

## Example agpm.toml Format

```toml
[sources]
community = "https://github.com/aig787/agpm-community.git"
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
installed_at = ".claude/agents/example-agent.md"
# Similar format for snippets, commands, scripts, hooks, mcp-servers
```

## Config Priority

1. `~/.agpm/config.toml` - Global config with auth tokens (not in git)
2. `agpm.toml` - Project manifest (in git)

Keeps secrets out of version control.
