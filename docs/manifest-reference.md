# Manifest Reference

This guide summarizes every field that appears in `agpm.toml` and how CLI inputs map to the manifest schema. Use it alongside the command reference when editing manifests manually or generating them with `agpm add`.

## Manifest Layout

```toml
[sources]                 # Named Git or local repositories
[tools.claude-code]       # Optional: Configure Claude Code tool
[tools.opencode]          # Optional: Configure OpenCode tool
[tools.agpm]              # Optional: Configure AGPM tool
[agents]                  # Resource sections share the same dependency schema
[snippets]
[commands]
[scripts]
[hooks]
[mcp-servers]
```

Each resource table maps a dependency name (key) to either a simple string path or an inline table with detailed settings.

## Dependency Forms

| Form | When to use | Example | Manifest shape |
| --- | --- | --- | --- |
| Simple path | Local files with no extra metadata | `helper = "../shared/helper.md"` | `ResourceDependency::Simple` |
| Detailed table | Remote Git resources, patterns, custom install behavior | `ai-helper = { source = "community", path = "agents/helper.md", version = "^1.0" }` | `ResourceDependency::Detailed` |

### Detailed Dependency Fields

| Field | Required | Applies to | Description | CLI mapping |
| --- | --- | --- | --- | --- |
| `source` | Only for Git resources | agents/snippets/commands/scripts/hooks/mcp-servers | Name from `[sources]`; omit for local filesystem paths. | Parsed from the `source:` prefix (e.g., `community:...`). |
| `path` | Yes | All | File path inside the repo (Git) or filesystem path/glob (local). Patterns are detected by `*`, `?`, or `[]`. | Parsed from the middle portion of the spec. |
| `version` | Default `"main"` for Git | Git resources | Tag, semantic range, `latest`, or branch alias. Used when no explicit `branch`/`rev` are provided. | Parsed from `@value` when using `agpm add dep`. Defaults to `main` if omitted. |
| `tool` | Default varies by resource | All | Target tool: `claude-code`, `opencode`, `agpm`, or custom. **Defaults**: snippets → `agpm`, all others → `claude-code`. Routes resources to tool-specific directories. | Manual edit. |
| `branch` | No | Git resources | Track a branch tip. Overrides `version` when present. Requires manual manifest edit today. | Add manually: `{ branch = "develop" }`. |
| `rev` | No | Git resources | Exact commit SHA (short or full). Highest precedence when set. | Add manually; not provided by current CLI shorthand. |
| `command` | MCP servers | MCP | Launch command (e.g., `npx`, `uvx`). | Use inline table or edit manifest. |
| `args` | MCP servers | MCP | Command arguments array. | Manual edit. |
| `target` | Optional | All | Override install subdirectory relative to artifact base directory. | Manual edit. |
| `filename` | Optional | All | Force output filename (with extension). | Manual edit. |
| `dependencies` | Auto-generated | All | Extracted transitive dependencies from resource metadata. Do not edit by hand. | Populated during install. |

> **Priority rules**: `rev` (commit) overrides `branch`, which overrides `version`. If you set multiple selectors, AGPM picks the most specific one.

### CLI Spec → Manifest Examples

```text
community:agents/reviewer.md@v1.0.0   → { source = "community", path = "agents/reviewer.md", version = "v1.0.0" }
community:agents/reviewer.md          → { source = "community", path = "agents/reviewer.md", version = "main" }
./local/agent.md --name helper        → helper = "./local/agent.md"
```

To track a branch or commit, edit the manifest entry manually:

```toml
[agents]
nightly = { source = "community", path = "agents/dev.md", branch = "main" }
pinned  = { source = "community", path = "agents/dev.md", rev = "abc123def" }
```

## Pattern Dependencies

- Specify glob characters (`*`, `?`, `[]`, `**`) in `path` to install multiple files.
- Provide a descriptive dependency name (`ai-agents`, `all-snippets`) so lockfile entries are easy to read.
- AGPM expands the pattern during install and records every concrete match in `agpm.lock` under the resolved dependency, using `resource_type/name@resolved_version` entries.
- Conflicts are detected after expansion—if two patterns resolve to the same install location, the install fails with a duplicate-path error (see the conflicts section for remediation guidance).

## Naming Overrides

| Setting | Section | Purpose | Example |
| --- | --- | --- | --- |
| `target` field | Dependency table | Move a single resource | `tool = { ..., target = "custom/tools" }` |
| `filename` field | Dependency table | Override installed filename | `tool = { ..., filename = "dev-tool.md" }` |

## Tool Configuration

AGPM supports multiple AI coding assistants through configurable tools. Each tool defines where resources are installed.

> ⚠️ **Alpha Feature**: OpenCode support is currently in alpha. While functional, it may have incomplete features or breaking
> changes in future releases. Claude Code support is stable and production-ready.

### Default Tools

| Tool | Base Directory | Supported Resources | Status |
| --- | --- | --- | --- |
| `claude-code` (default) | `.claude` | agents, commands, scripts, hooks, mcp-servers, snippets | ✅ Stable |
| `opencode` | `.opencode` | agents, commands, mcp-servers | 🚧 Alpha |
| `agpm` | `.agpm` | snippets | ✅ Stable |

### Using the Tool Field

Add `tool` to any dependency to route it to a specific tool:

```toml
[agents]
# Default: installs to .claude/agents/helper.md (agents default to claude-code)
claude-agent = { source = "community", path = "agents/helper.md", version = "v1.0.0" }

# OpenCode: installs to .opencode/agent/helper.md (note: singular "agent") - Alpha
opencode-agent = { source = "community", path = "agents/helper.md", version = "v1.0.0", tool = "opencode" }

[snippets]
# Default: snippets install to .agpm/snippets/ (snippets default to agpm, not claude-code)
shared = { source = "community", path = "snippets/rust-patterns.md", version = "v1.0.0" }

# Claude Code specific: explicitly set tool to install to .claude/snippets/
claude-specific = { source = "community", path = "snippets/claude.md", version = "v1.0.0", tool = "claude-code" }
```

### Custom Tool Configuration

Override default directories or define custom tools:

```toml
[tools.claude-code]
path = ".claude"
resources = { agents = { path = "agents" }, commands = { path = "commands" } }

[tools.opencode]
path = ".opencode"
resources = { agents = { path = "agent" }, commands = { path = "command" } }

[tools.custom-tool]
path = ".mytool"
resources = { agents = { path = "agents" }, commands = { path = "cmds" } }
```

### MCP Server Configuration

MCP servers automatically route to the correct configuration file based on type:

```toml
[mcp-servers]
# Merges into .mcp.json
claude-fs = { source = "community", path = "mcp/filesystem.json", version = "v1.0.0" }

# Merges into opencode.json - Alpha
opencode-fs = { source = "community", path = "mcp/filesystem.json", version = "v1.0.0", tool = "opencode" }
```

## Recommended Workflow

1. Use `agpm add dep` for initial entries—this ensures naming and defaults are correct.
2. Edit the generated inline table when you need advanced selectors (`branch`, `rev`, `tool`), custom install paths, or MCP launch commands.
3. Re-run `agpm install` (or `agpm validate --resolve`) after manual edits to confirm the manifest parses and resolves correctly.
