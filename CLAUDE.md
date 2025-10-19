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
├── templating/  # Template rendering engine
│   ├── mod.rs   # Template context and renderer
│   └── filters.rs  # Custom filters (content)
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
- `validate [--resolve] [--check-lock] [--sources] [--paths] [--render] [--format json] [--strict]` - Validate manifest and dependencies
- `cache [clean|info]` - Manage cache
- `config [show|edit|init|add-source|remove-source]` - Global config
- `add [source|dep]` - Add to manifest
- `remove [source|dep]` - Remove from manifest
- `init [--path]` - Initialize project

## Development

- **Best Practices**: See `.agpm/snippets/rust-best-practices.md` for comprehensive coding standards
- **Imports**: Prefer `use crate::module::Type;` at top of file vs `crate::module::Type` throughout code
- **Pre-commit**: Always run `cargo fmt` before committing code
- **Note**: `cargo clippy --fix` requires `--allow-dirty` flag when there are uncommitted changes
- **Docstrings**: Use `no_run` attribute for code examples by default unless they should be executed as tests; use
  `ignore` for examples that won't compile

## Dependencies

Main: clap, tokio, toml, toml_edit, serde, serde_json, serde_yaml, anyhow, thiserror, colored, dirs, tracing, tracing-subscriber,
indicatif, tempfile, semver, shellexpand, which, uuid, chrono, walkdir, sha2, hex, regex, futures, fs4, glob, dashmap (v6.1),
reqwest, zip, petgraph, pubgrub, tokio-retry, tera

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

GitHub Actions: Cross-platform tests, crates.io publish

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
- **Patch/Override System** (v0.4.x+): TOML-based field overrides without forking, private config layer, lockfile tracking
- **Opt-in templating** (v0.4.5+): Markdown template rendering disabled by default, enabled per-resource via `agpm.templating: true` in frontmatter
- **Flatten configuration** (v0.4.5+): Pattern dependencies support `flatten` field to control directory structure preservation (defaults: agents/commands flatten, others preserve)
- **Custom dependency names** (v0.4.5+): Transitive dependencies can specify `name` field for custom template variable names
- **Duplicate path elimination** (v0.4.5+): Automatic removal of redundant directory prefixes (e.g., prevents `.claude/agents/agents/file.md`)
- **File reference validation** (v0.4.6+): Automatic auditing of markdown file references to detect broken cross-references during validation

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

**Markdown** (YAML): `dependencies.agents[].path`, `.version`, `.tool`
**JSON** (top-level): `dependencies.commands[].path`, `.version`, `.tool`

```yaml
---
dependencies:
  agents:
    - path: agents/helper.md
      version: v1.0.0
      tool: claude-code  # Optional: specify target tool
      name: custom_helper  # Optional: custom template variable name
  snippets:
    - path: snippets/utils.md
      flatten: true  # Optional: flatten directory structure
    - path: snippets/best-practices.md
      install: false  # Don't create file, only make content available in templates
      name: best_practices
    # version, tool, name, flatten, and install inherited from parent if not specified
---
```

**JSON files** (top-level field):

```json
{
  "dependencies": {
    "commands": [
      {
        "path": "commands/deploy.md",
        "version": "v2.0.0",
        "tool": "opencode"
      }
    ]
  }
}
```

**Supported Fields**:

- `path` (required): Path to the dependency file within the source repository
- `version` (optional): Version constraint (inherits from parent if not specified)
- `tool` (optional): Target tool (`claude-code`, `opencode`, `agpm`). If not specified:
  - Inherits from parent if parent's tool supports this resource type
  - Falls back to default tool for this resource type
- `name` (optional): Custom name for template variable references (defaults to sanitized filename)
- `flatten` (optional): For pattern dependencies, controls directory structure preservation (defaults: agents/commands true, others false)
- `install` (optional): Whether to write file to disk (default: `true`). When `false`, content is only available in templates via `{{ agpm.deps.<type>.<name>.content }}`

**Key Features**:

- Graph-based resolution with topological ordering
- Cycle detection prevents infinite loops
- Version inheritance when not specified
- Tool inheritance with automatic fallback
- Same-source dependency model (inherits parent's source)
- Parallel resolution for maximum efficiency
- Unknown field detection with warnings (v0.4.5+)
- **Content embedding** (v0.4.7+): All dependencies have `content` field in templates with processed file content (frontmatter stripped from Markdown, metadata removed from JSON)

## Template Features (v0.4.8+)

**Embed content** via dependency `.content` (versioned) or `content` filter (project-local):

```markdown
---
agpm.templating: true
dependencies:
  snippets:
    - path: snippets/rust-patterns.md
      name: rust_patterns
---
## Shared Patterns (versioned)
{{ agpm.deps.snippets.rust_patterns.content }}

## Project Style (local)
{{ 'project/rust-style.md' | content }}
```

**Content Filter**: `{{ 'path' | content }}` reads text files with path validation, recursive (10 levels). Markdown: frontmatter stripped. JSON: pretty-printed. Security: no traversal, text only (.md/.txt/.json/.toml/.yaml).

## Versioned Prefixes (v0.3.19+)

Monorepo-style prefixed tags: `agents-v1.0.0`, `snippets-^v2.0.0`. Prefixes isolate version namespaces (`agents-^v1.0.0` matches only `agents-v*` tags).

## Cross-Platform Path Handling

**CRITICAL**: AGPM must work identically on Windows, macOS, Linux. Lockfiles use Unix-style forward slashes only for portability.

**Path Rules**:
1. **Forward slashes** in lockfiles, .gitignore, TOML manifests
2. **`normalize_path_for_storage()`** for ALL lockfile paths (never `Path::display()` directly)
3. **`Path`/`PathBuf`** for runtime operations
4. **Windows gotchas**: `C:\path`, `\\server\share`, reserved names (CON, PRN, AUX, NUL, COM1-9, LPT1-9)
5. **Tests**: Use `TestProject`, let `agpm install` generate lockfiles, no hardcoded separators

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

AGPM supports multiple AI coding tools: **claude-code** (default), **opencode**, **agpm** (snippets), **custom**.

**Tool Configuration**: Each tool defines base directory, resource paths, MCP handling strategy.

**Dependency Tool Field**:
```toml
[agents]
example = { source = "community", path = "agents/example.md", version = "v1.0.0" }  # claude-code
opencode-agent = { source = "community", path = "agents/helper.md", tool = "opencode" }
```

**Default Tools** via `[default-tools]`: Override per-resource-type defaults (snippets→agpm, others→claude-code).

**Merge Targets**: Hooks/MCP servers merge into shared configs (`.claude/settings.local.json`, `.mcp.json`, `.opencode/opencode.json`). Custom tools can override via `[tools.my-tool.resources.hooks]` with `merge-target` field.

**Resource Support**: agents, commands (both tools), scripts (claude-code only), hooks (claude-code only), mcp-servers (both tools), snippets (agpm default). Pluggable MCP handlers: ClaudeCodeMcpHandler, OpenCodeMcpHandler.

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
local = "../my-local-resources"

# Default tools per resource type (optional)
[default-tools]
snippets = "claude-code"  # Override default for Claude-only users
agents = "claude-code"    # Explicit (already the default)

[agents]
example = { source = "community", path = "agents/example.md", version = "v1.0.0" }
ai-helper = { source = "community", path = "agents/ai/gpt.md", version = "v1.0.0" }  # Preserves subdirs
ai-all = { source = "community", path = "agents/ai/*.md", version = "v1.0.0" }  # Pattern support
opencode = { source = "community", path = "agents/helper.md", tool = "opencode" }

[snippets]
example = { source = "community", path = "snippets/example.md", version = "v1.2.0" }

[commands]
deploy = { source = "community", path = "commands/deploy.md", version = "v2.0.0" }

[hooks]
pre-commit = { source = "community", path = "hooks/pre-commit.json", version = "v1.0.0" }

[mcp-servers]
filesystem = { source = "community", path = "mcp-servers/filesystem.json", version = "v1.0.0" }

# Patches - override resource fields without forking
[patch.agents.example]
model = "claude-3-haiku"
temperature = "0.8"
```

## Example agpm.lock

```toml
# Auto-generated lockfile
[[agents]]
name = "example"
source = "community"
path = "agents/example.md"
version = "v1.0.0"
resolved_commit = "abc123..."
checksum = "sha256:..."
installed_at = ".claude/agents/example.md"
patches = ["model", "temperature"]  # Applied patches tracked

[[agents]]
name = "ai-helper"
source = "community"
path = "agents/ai/gpt.md"
version = "v1.0.0"
resolved_commit = "abc123..."
checksum = "sha256:..."
installed_at = ".claude/agents/ai/gpt.md"  # Preserves subdirs
```

## Config Priority

1. `~/.agpm/config.toml` - Global config with auth tokens (not in git)
2. `agpm.toml` - Project manifest (in git)
3. `agpm.private.toml` - User-level patches (not in git, add to .gitignore)

**Patch Merging** (v0.4.x+):
- Project patches (`agpm.toml`) define team-wide overrides
- Private patches (`agpm.private.toml`) extend with personal settings
- Different fields combine; same field in both - private silently overrides project
- Applied patches tracked in lockfile `patches` field

Keeps secrets out of version control.
