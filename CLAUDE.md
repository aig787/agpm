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
- **File Size**: Keep source code files under 1,000 lines of code (excluding empty lines and comments). Files exceeding this limit should be refactored into smaller, focused modules. Use `cloc` to count lines of code: `cloc src/file.rs --include-lang=Rust`
- **Code Cleanup**: Prefer removing unused code over marking it as deprecated or prefixing variables/arguments with `_`. Delete dead imports, unused functions, and obsolete dependencies entirely.
- **Imports**: Prefer `use crate::module::Type;` at top of file vs `crate::module::Type` throughout code
- **Pre-commit**: Always run `cargo fmt` before committing code
- **Note**: `cargo clippy --fix` requires `--allow-dirty` flag when there are uncommitted changes
- **Docstrings**: Use `no_run` attribute for code examples by default unless they should be executed as tests; use
  `ignore` for examples that won't compile
- **File Operations**: All file operations must use `with_file_context()` or return `FileOperationError` to ensure proper error context with file paths. Use `.with_context()` only for non-file operations.

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
- **Test File Size**: Keep colocated module tests (e.g., `{module}_tests.rs` files in `src/`) under 250 lines. Standalone integration test files in `tests/` directory are subject to the 1,000 line limit. Tests exceeding their respective limits should be broken out into separate files with descriptive names (e.g., `test_install_basic.rs`, `test_install_transitive.rs`).
- **Test File Naming**: For module-specific tests, use `{module}_tests.rs` naming convention (e.g., `tool_config_tests.rs`) instead of placing `tests.rs` files within subdirectories. This keeps test files at the same level as the modules they test and follows Rust conventions.

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
- **Operation-scoped context**: Warning deduplication via OperationContext (CLI→Resolver→Extractor), no global state
- **Dual Checksum System** (v0.4.8+): File checksum + context checksum for deterministic lockfiles
- **Deterministic Lockfile Format**: TOML with consistent ordering using toml_edit
- **Hash-based Identity**: Resource identity uses SHA-256 hash of variant inputs for consistency
- **Template Variable Overrides** (v0.4.9+): Per-dependency template variable overrides for reusable generic templates

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
      template_vars: {}        # optional (v0.4.9+)
```

Features: graph resolution, cycle detection, version/tool inheritance, parallel processing, content embedding

## Template Features (v0.4.8+)

**Embed content**: `{{ agpm.deps.snippets.name.content }}` (versioned) or `{{ 'path.md' | content }}` (local). Markdown: frontmatter stripped. JSON: pretty-printed.

**Template Variable Overrides** (v0.4.9+): Override context per-dependency:
```toml
python = { source = "c", path = "agents/generic.md", template_vars = { project.language = "python" } }
rust = { source = "c", path = "agents/generic.md", template_vars = { project.language = "rust" } }
```
Deep merge: objects recursively merged, primitives/arrays replaced.

## Versioned Prefixes (v0.3.19+)

Monorepo prefixed tags: `agents-v1.0.0`, `snippets-^v2.0.0`. Prefixes isolate namespaces (`agents-^v1.0.0` matches only `agents-v*`).

## Cross-Platform Path Handling

**CRITICAL**: AGPM must work identically on Windows, macOS, Linux. Lockfiles use Unix-style forward slashes.

**Rules**: Forward slashes in lockfiles/manifests; `normalize_path_for_storage()` for lockfile paths; `Path`/`PathBuf` for runtime; Windows gotchas: reserved names (CON, PRN, AUX, NUL, COM1-9, LPT1-9); Tests: use `TestProject`, let `agpm install` generate lockfiles.

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

Centralized `VersionResolver` batch-resolves versions to SHAs upfront. Two-phase: collection → resolution. Single bare repo per source, single fetch per command. SHA-keyed worktrees (one per unique commit). Per-worktree locks for parallelism. Semver constraint support ("^1.0", "~2.1"). Auto-deduplication: refs → same commit share worktree.

## Multi-Tool Support

Supports **claude-code** (default), **opencode**, **agpm** (snippets), **custom**. Each tool has base directory, resource paths, MCP strategy. Set via `tool` field or `[default-tools]` section. Resources: agents/commands (both), scripts/hooks (claude-code only), mcp-servers (both), snippets (agpm default).

## Key Requirements

- **Use Task tool** for complex operations
- **Cross-platform**: Windows, macOS, Linux
- **NO git2**: Use system git command
- **Security**: Credentials only in ~/.agpm/config.toml, path traversal prevention, checksums
- **Atomic ops**: Temp file + rename
- **Resources**: .md, .json, .sh/.js/.py files
- **Hooks**: Configure in .claude/settings.local.json
- **MCP**: Configure in .mcp.json

## Example agpm.toml

```toml
[sources]
community = "https://github.com/aig787/agpm-community.git"

[agents]
example = { source = "community", path = "agents/example.md", version = "v1.0.0" }
ai-all = { source = "community", path = "agents/ai/*.md", version = "v1.0.0" }  # Pattern
helper-custom = { source = "community", path = "agents/helper.md", filename = "helper-custom.md" }  # Custom name
python-dev = { source = "community", path = "agents/generic.md", template_vars = { project.language = "python" } }

[snippets]
example = { source = "community", path = "snippets/example.md", version = "v1.2.0" }

# Patches - override without forking
[patch.agents.example]
model = "claude-3-haiku"
```

## Dual Checksum System (v0.4.8+)

- **File checksum**: SHA-256 of rendered content (determines reinstallation)
- **Context checksum**: SHA-256 of template inputs (audit/debug only)
- **Deterministic lockfiles**: toml_edit ensures consistent ordering across runs
- **Hash-based identity**: `variant_inputs_hash` from template inputs for deduplication
- **Field migration**: `template_vars` (manifest/lockfile) = `variant_inputs` (internal)

## Example agpm.lock

```toml
[[agents]]
name = "example"
manifest_alias = "example"
source = "community"
path = "agents/example.md"
version = "v1.0.0"
resolved_commit = "abc123..."
checksum = "sha256:..."
installed_at = ".claude/agents/example.md"
patches = ["model", "temperature"]

[[agents]]
name = "agents/helper"  # Canonical name
manifest_alias = "helper-custom"  # Manifest key
template_vars = "{\"project\": {\"language\": \"python\"}}"
variant_inputs_hash = "sha256:9i0j1k2l..."
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
