# AGENTS.md - AGPM Project Context

**IMPORTANT**: This file must remain under 20,000 characters.

This document mirrors AGPM context from CLAUDE.md for Codex/OpenCode contributors. AGPM is the Claude Code Package Manager
supporting multiple tools (Claude Code, OpenCode, custom) via pluggable artifacts.

## Overview

AGPM (Claude Code Package Manager) is a Git-based package manager for AI coding assistant resources (agents, snippets, commands,
scripts, hooks, MCP servers), written in Rust. Supports multiple tools (Claude Code, OpenCode, custom) via pluggable artifacts.
Uses lockfile model (agpm.toml + agpm.lock) like Cargo for reproducible installations from Git repositories.

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
│   ├── error.rs        # Core error types
│   ├── error_formatting.rs  # User-friendly error formatting
│   └── file_error.rs   # Structured file operation error handling with context
├── git/         # Git CLI wrapper + worktrees
├── hooks/       # Hook integrations for Claude Code environments
├── installer/   # Parallel resource installation + artifact cleanup
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
│   ├── error.rs        # Enhanced template error types with user-friendly formatting
│   ├── dependencies/   # Dependency resolution for templates
│   │   ├── mod.rs        # Module exports and public API
│   │   ├── extractors.rs # Custom names and specs extraction
│   │   └── builders.rs   # Build dependencies data
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

## Rust Agents (OpenCode)

### Standard (Fast)

- `rust-expert-standard`: Implementation, refactoring, API design
- `rust-linting-standard`: Formatting, clippy fixes
- `rust-doc-standard`: Documentation
- `rust-test-standard`: Test failure fixes
- `rust-troubleshooter-standard`: Debugging, build issues

### Advanced (Complex)

- `rust-expert-advanced`: Architecture, optimization, complex features
- `rust-linting-advanced`: Complex refactoring, advanced clippy
- `rust-doc-advanced`: Architectural docs, comprehensive analysis
- `rust-test-advanced`: Property testing, fuzzing, complex scenarios
- `rust-troubleshooter-advanced`: Memory issues, UB, deep debugging

## Development

- **Best Practices**: See `local-deps/snippets/rust-best-practices.md`
- **File Size**: Max 1,000 LOC and 2,000 lines total per file (use `cloc src/file.rs --include-lang=Rust` for LOC)
- **Code Cleanup**: Delete unused code (no `_` prefixes or deprecation markers)
- **Imports**: `use crate::module::Type;` at top of file
- **Pre-commit**: Run `cargo fmt` before commits
- **Clippy**: Use `--allow-dirty` flag when uncommitted changes exist
- **Docstrings**: Use `no_run` by default, `ignore` for non-compiling examples
- **File Operations**: Use `with_file_context()` for proper error context with paths

## Template Features (v0.4.8+)

**Embed content**: `{{ agpm.deps.snippets.name.content }}` (versioned) or `{{ 'path.md' | content }}` (local). Path validation, recursive (10 levels). See [docs/templating.md](../docs/templating.md).

## Dependencies

Main: clap, tokio, toml, toml_edit, serde, serde_json, serde_yaml, anyhow, thiserror, colored, dirs, tracing, tracing-subscriber,
indicatif, tempfile, semver, shellexpand, which, uuid, chrono, walkdir, sha2, hex, regex, futures, fs4, glob, dashmap (v6.1),
reqwest, zip, petgraph, pubgrub, tokio-retry, tera

Dev: assert_cmd, predicates, serial_test

## Testing

- **cargo nextest**: Fast parallel execution (`cargo nextest run` + `cargo test --doc`)
- **Parallel-safe**: No `std::env::set_var`, each test gets own temp dir
- **Use helpers**: `TestProject` and `TestGit` from `tests/common/mod.rs` (never raw `std::process::Command`)
- **Auto-generate lockfiles**: Don't manually create (breaks on Windows path separators)
- **File size**: Module tests max 250 LOC (500 lines total), integration tests max 1,000 LOC (2,000 lines total)
- **Naming**: Use `{module}_tests.rs` (e.g., `tool_config_tests.rs`)
- **Critical**: Never use "update" in test filenames (Windows UAC), test both TTY/NON-TTY modes
- Target: 70% coverage, parallelism: max(10, 2 × CPU cores)

### Stress Tests

Stress tests (`tests/stress/`) are **excluded from default nextest runs** via `.config/nextest.toml`:

```bash
# Run with nextest (requires -P all profile)
cargo nextest run -P all -E 'binary(stress)'
cargo nextest run -P all --test stress --no-capture

# Run with cargo test
cargo test --test stress -- --nocapture
```

Modules: `large_scale`, `parallelism`, `chaos_conflict_tracking`, `pattern_performance`, `template_context_lookup`, `transitive_depth`

## Build & CI

```bash
# Full build and test suite
cargo build --release  # Optimized with LTO
cargo fmt && cargo clippy -- -D warnings && cargo nextest run && cargo test --doc

# Run all tests in a module
cargo nextest run -E 'test(cache)'      # All cache module tests
cargo test cache                         # Standard cargo test for module
cargo nextest run -E 'test(install::basic)'  # Integration test submodule

# Run a single test
cargo nextest run test_install_basic
cargo test install::basic::test_install_creates_lockfile

# Run without capturing output (see println! and dbg!)
cargo nextest run test_name --no-capture
cargo test test_name -- --nocapture  # Note: different syntax

# Run with verbose output
RUST_LOG=debug cargo nextest run test_name
```

GitHub Actions: Cross-platform tests, crates.io publish

**cargo-dist**: Uses `dist` command (NOT `cargo dist`). The binary is named `dist` in `~/.cargo/bin/`.

## Key Design Decisions

**Core Architecture**:
- System git (no git2 library), atomic operations (temp + rename), async I/O (tokio::fs)
- Copy files instead of symlinks for cross-platform compatibility
- SHA-based worktrees: one per unique commit, shared across refs to same SHA
- Command-level parallelism (default: max(10, 2 × CPU cores)) with per-worktree locks

**Dependency Resolution**:
- Centralized VersionResolver with batch SHA resolution and deduplication
- Upfront version resolution before any checkouts, single fetch per repo per command
- Graph-based transitive dependencies with cycle detection, supports versioned prefixes
- Enhanced parsing with manifest context for local vs Git detection

**Multi-Tool & Resources**:
- Pluggable tools (claude-code, opencode, agpm, custom) with tool-aware path resolution
- Resources install to tool-specific directories, pluggable MCP handlers
- Relative path preservation, automatic artifact cleanup, duplicate path elimination

**Templating & Content**:
- Opt-in templating (disabled by default, enable via `agpm.templating: true`)
- Template variable overrides per-dependency for reusable generic templates
- Content embedding with frontmatter stripping, file reference validation
- Content filter for project files with path validation, recursive rendering (10-level depth)
- Enhanced template errors with dependency chains and variable suggestions

**Configuration & Patches**:
- TOML-based patches without forking (project + private layers)
- Dual checksum system (file + context) for deterministic lockfiles
- Hash-based resource identity using SHA-256 of variant inputs
- Gitignore management: control .gitignore entries via `gitignore` field (default: true)

**Error Handling**:
- Structured file errors (FileOperationError) with operation context, path, caller, purpose
- Operation-scoped warning deduplication (no global state)
- User-friendly error formatting with actionable suggestions

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

### Resource Identity: `name` vs `manifest_alias`

Every resource in AGPM has two identity fields with distinct purposes:

**`name` (Canonical Name)**:
- **Purpose**: Deduplication and identity matching
- **Source**: Derived from the file path (e.g., `agents/helper.md` → `agents/helper`)
- **Always present**: Set for ALL dependencies (direct, transitive, pattern)
- **Uniqueness**: Combined with source, tool, and variant_hash to identify resources
- **Example**: For `agents/utils/helper.md`, name is `agents/utils/helper`

**`manifest_alias` (Manifest Key)**:
- **Purpose**: User-facing identifier from the manifest
- **Source**: The key used in `agpm.toml` (e.g., `helper-custom` in `[agents]` section)
- **Only for direct**: Present ONLY for direct manifest dependencies (None for transitive)
- **Example**: `helper-custom = { source = "...", path = "agents/helper.md", filename = "helper-custom.md" }`

**Dependency Type Behavior**:

1. **Direct Dependencies** (from manifest):
   - `name`: `agents/helper` (canonical)
   - `manifest_alias`: `helper-custom` (user's choice)
   - Both fields populated

2. **Transitive Dependencies** (from resource files):
   - `name`: `agents/helper` (canonical)
   - `manifest_alias`: `None`
   - Only name field populated

3. **Pattern Dependencies** (e.g., `agents/*.md`):
   - Each matched file: `name` = canonical path (e.g., `agents/file1`, `agents/file2`)
   - All share: `manifest_alias` = pattern key (e.g., `all-agents`)

**Deduplication Priority**:
- When same resource appears as both direct and transitive: **Direct wins**
- Logic: Resources with `manifest_alias != None` override those with `manifest_alias == None`
- Ensures manifest customizations (filename, template_vars) take precedence

### Transitive Dependencies

Declare in YAML frontmatter or JSON `dependencies` field:
```yaml
dependencies:
  agents:
    - path: agents/helper.md  # required
      version: v1.0.0          # optional (inherits)
      tool: claude-code        # optional (inherits)
      name: custom_helper      # optional (for templates)
      flatten: true            # optional (defaults vary)
      install: false           # optional (default: true)
```

Features: graph resolution, cycle detection, version/tool inheritance, parallel processing, content embedding

## Versioned Prefixes (v0.3.19+)

Monorepo-style prefixed tags: `agents-v1.0.0`, `snippets-^v2.0.0`. Prefixes isolate version namespaces (`agents-^v1.0.0` matches only `agents-v*` tags).

## Cross-Platform Path Handling

**CRITICAL**: AGPM works identically on Windows, macOS, Linux. Lockfiles use Unix `/` slashes.

**Rules**: Use `normalize_path_for_storage()` for lockfiles; `Path`/`PathBuf` for runtime; avoid Windows reserved names (CON, PRN, AUX, NUL, COM1-9, LPT1-9).

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

- **Task tool** for complex operations; **cross-platform** (Windows/macOS/Linux)
- **System git** (NO git2); **atomic ops** (temp + rename)
- **Security**: Credentials in ~/.agpm/config.toml, path traversal prevention, checksums
- **Resources**: .md, .json, .sh/.js/.py; **Hooks**: .claude/settings.local.json; **MCP**: .mcp.json

## Multi-Tool Support

Tools: **claude-code** (default), **opencode**, **agpm**, **custom**. Set via `tool` field or `[default-tools]`. agents/commands (all tools), scripts/hooks (claude-code only), mcp-servers (all), snippets (agpm default).

## Tool Configuration Merging

AGPM automatically merges user-provided tool configurations with built-in defaults for backward compatibility. This ensures transitive dependencies work correctly even with older manifests that specify partial configurations.

- **Well-known tools** (claude-code, opencode, agpm): User configurations override defaults, missing resource types are filled in automatically
- **Custom tools**: No automatic merging, user configuration used as-is

## Example agpm.toml

```toml
gitignore = true  # Default: manage .gitignore entries
# gitignore = false  # Private setups: don't manage .gitignore

[sources]
community = "https://github.com/aig787/agpm-community.git"

[agents]
example = { source = "community", path = "agents/example.md", version = "v1.0.0" }
ai-all = { source = "community", path = "agents/ai/*.md", version = "v1.0.0" }  # Pattern

[snippets]
example = { source = "community", path = "snippets/example.md", version = "v1.2.0" }

# Patches - override without forking
[patch.agents.example]
model = "claude-3-haiku"
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

1. `~/.agpm/config.toml` - Global (auth tokens, not in git)
2. `agpm.toml` - Project manifest (in git)
3. `agpm.private.toml` - User patches (not in git)

**Patch Merging**: Project patches (team-wide) + private patches (personal). Same field: private wins. Tracked in lockfile `patches`.
