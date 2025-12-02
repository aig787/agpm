# Multi-Tool Support Guide

> 🚧 **Important Notice**: OpenCode support is currently in **alpha**. While functional, it may have incomplete features or breaking changes in future releases. Claude Code support is stable and production-ready.

AGPM supports multiple AI coding assistants through a pluggable tool system. You can manage resources for different tools from a single manifest, enabling shared workflows and infrastructure.

## Table of Contents

- [Supported Tools](#supported-tools)
- [Resource Type Support Matrix](#resource-type-support-matrix)
- [How It Works](#how-it-works)
- [Configuration](#configuration)
- [Examples](#examples)
- [Benefits](#benefits)
- [Migration Guide](#migration-guide)

## Supported Tools

- **claude-code** - Claude Code resources (agents, commands, scripts, hooks, MCP servers) ✅ **Stable**
  - Default for: agents, commands, scripts, hooks, mcp-servers
  - Full feature support
  - Production-ready

- **opencode** - OpenCode resources for agents, commands, and MCP servers 🚧 **Alpha**
  - **Note**: Alpha status - features may change. Use with caution in production.
  - Partial feature support
  - Directory naming differs from Claude Code (singular vs plural)

- **agpm** - Shared snippets and templates usable across tools ✅ **Stable**
  - Default for: snippets
  - Provides shared infrastructure for cross-tool content

- **custom** - Define your own tools via configuration
  - Fully customizable resource paths
  - Custom merge targets for configuration files

## Resource Type Support Matrix

| Resource    | Claude Code                  | OpenCode (Alpha)              | AGPM                 |
|-------------|------------------------------|-------------------------------|----------------------|
| Agents      | ✅ `.claude/agents/agpm/`     | 🚧 `.opencode/agent/agpm/`     | ❌                    |
| Commands    | ✅ `.claude/commands/agpm/`   | 🚧 `.opencode/command/agpm/`   | ❌                    |
| Scripts     | ✅ `.claude/scripts/agpm/`    | ❌                             | ❌                    |
| Hooks       | ✅ → `.claude/settings.local.json` | ❌                      | ❌                    |
| MCP Servers | ✅ → `.mcp.json`              | 🚧 → `opencode.json`           | ❌                    |
| Snippets    | ✅ `.claude/snippets/agpm/`   | ❌                             | ✅ `.agpm/snippets/` (default) |

## How It Works

### Default Behavior

1. **Snippets** default to `agpm` (shared infrastructure at `.agpm/snippets/`)
2. **All other resources** default to `claude-code`
3. Resources without an explicit `tool` field use their type's default

### Explicit Tool Specification

Override the default by adding the `tool` field to any dependency:

```toml
[agents]
# Uses default (claude-code)
helper = { source = "community", path = "agents/helper.md", version = "v1.0.0" }

# Explicit tool specification
helper-oc = { source = "community", path = "agents/helper.md", version = "v1.0.0", tool = "opencode" }
```

### Directory Differences

**Important**: OpenCode uses singular directory names while Claude Code uses plural. All resources install to `agpm/` subdirectories:

- Agents: `.claude/agents/agpm/` vs `.opencode/agent/agpm/`
- Commands: `.claude/commands/agpm/` vs `.opencode/command/agpm/`

AGPM handles this automatically based on the `tool` field.

### MCP Server Integration

MCP servers are merged into tool-specific configuration files:
- **Claude Code**: Merged into `.mcp.json`
- **OpenCode**: Merged into `opencode.json` 🚧 Alpha

## Configuration

### Configuring Default Tools

You can override which tool is used by default for each resource type using the `[default-tools]` section:

```toml
[sources]
community = "https://github.com/aig787/agpm-community.git"

# Configure default tools per resource type
[default-tools]
snippets = "claude-code"  # Claude-only users: install to .claude/snippets/
agents = "claude-code"    # Explicit (already the default)
commands = "opencode"     # Default commands to OpenCode

[agents]
# Uses default from [default-tools]: installs to .claude/agents/
helper = { source = "community", path = "agents/helper.md", version = "v1.0.0" }

# Explicit tool overrides the default: installs to .opencode/agent/
opencode-helper = { source = "community", path = "agents/helper.md", version = "v1.0.0", tool = "opencode" }
```

### Use Cases

- **Claude Code only**: Set `snippets = "claude-code"` to install to `.claude/snippets/`
- **OpenCode preferred**: Set `agents = "opencode"` and `commands = "opencode"`
- **Mixed workflows**: Configure different defaults for different resource types

## Examples

### Basic Multi-Tool Manifest

```toml
[sources]
community = "https://github.com/aig787/agpm-community.git"

[agents]
# Claude Code (default) - installed at .claude/agents/agpm/helper.md
claude-helper = { source = "community", path = "agents/helper.md", version = "v1.0.0" }

# OpenCode (explicit tool field) - installed at .opencode/agent/agpm/helper.md
opencode-helper = { source = "community", path = "agents/helper.md", version = "v1.0.0", tool = "opencode" }

# Both tools can share the same source file - AGPM installs to the correct location

[snippets]
# Shared snippets (default to agpm) - installed at .agpm/snippets/rust/*.md
rust-patterns = { source = "community", path = "snippets/rust/*.md", version = "v1.0.0" }
```

### Complete Mixed-Tool Project

```toml
[sources]
community = "https://github.com/aig787/agpm-community.git"

[agents]
# Rust experts for both tools
rust-expert-cc = { source = "community", path = "agents/rust-expert.md", version = "v1.0.0" }
rust-expert-oc = { source = "community", path = "agents/rust-expert.md", version = "v1.0.0", tool = "opencode" }

[commands]
# Deployment command for Claude Code
deploy-cc = { source = "community", path = "commands/deploy.md", version = "v1.0.0" }
# Same command for OpenCode
deploy-oc = { source = "community", path = "commands/deploy.md", version = "v1.0.0", tool = "opencode" }

[mcp-servers]
# MCP servers for both tools (automatically routed to correct config file)
filesystem-cc = { source = "community", path = "mcp/filesystem.json", version = "v1.0.0" }
filesystem-oc = { source = "community", path = "mcp/filesystem.json", version = "v1.0.0", tool = "opencode" }  # 🚧 Alpha

[snippets]
# Snippets default to agpm (shared across all tools)
shared-patterns = { source = "community", path = "snippets/patterns/*.md", version = "v1.0.0" }
# No tool field needed - installs to .agpm/snippets/ by default
```

### Installation Results

Using the above configuration:
- `rust-expert-cc` → `.claude/agents/agpm/rust-expert.md`
- `rust-expert-oc` → `.opencode/agent/agpm/rust-expert.md` (note: singular "agent") 🚧 Alpha
- `filesystem-cc` → Merged into `.mcp.json`
- `filesystem-oc` → Merged into `opencode.json` 🚧 Alpha
- `shared-patterns` → `.agpm/snippets/patterns/*.md` (shared infrastructure)

## Benefits

- **Unified Workflow**: Manage all AI assistant resources from one place
- **Shared Infrastructure**: Reuse common snippets and patterns across tools
- **Consistent Versioning**: Lock all tools to the same resource versions
- **Easy Migration**: Switch between tools without recreating resource infrastructure
- **Team Collaboration**: Different team members can use different tools with the same resources

## Migration Guide

### From Single-Tool to Multi-Tool

If you have an existing Claude Code-only project, no changes are required. Your existing manifest will continue to work as Claude Code is the default for most resource types.

To add OpenCode support:

1. Add tool-specific entries for OpenCode resources:
```toml
[agents]
# Existing Claude Code agent (no changes needed)
my-agent = { source = "community", path = "agents/helper.md", version = "v1.0.0" }

# Add OpenCode version
my-agent-oc = { source = "community", path = "agents/helper.md", version = "v1.0.0", tool = "opencode" }
```

2. Run `agpm install` to install the new resources

### Changing Default Tools

To change your project's default tools:

1. Add the `[default-tools]` section to your manifest:
```toml
[default-tools]
agents = "opencode"    # New agents default to OpenCode
commands = "opencode"  # New commands default to OpenCode
```

2. Existing entries without explicit `tool` fields will now use the new defaults
3. Add `tool = "claude-code"` to specific entries that should remain with Claude Code

### Sharing Resources Between Teams

For teams using different tools:

1. Create shared snippets in the AGPM namespace:
```toml
[snippets]
# Accessible to all tools
shared-utils = { source = "community", path = "snippets/utils/*.md", version = "v1.0.0" }
```

2. Create tool-specific resources with clear naming:
```toml
[agents]
helper-cc = { source = "community", path = "agents/helper.md", version = "v1.0.0" }
helper-oc = { source = "community", path = "agents/helper.md", version = "v1.0.0", tool = "opencode" }
```

3. Document which tool each team member should use in your project README

## Troubleshooting

### Resources Installing to Wrong Directory

Check if you have a `[default-tools]` section that might be overriding expected behavior. Explicit `tool` fields always take precedence over defaults.

### OpenCode Resources Not Working

Remember that OpenCode support is in alpha. Check the [Resource Type Support Matrix](#resource-type-support-matrix) to ensure the resource type is supported for OpenCode.

### Shared Snippets Not Found

By default, snippets install to `.agpm/snippets/`. If you need Claude Code-specific snippets, add `tool = "claude-code"` to install to `.claude/snippets/`.

## Future Enhancements

- Additional tool support (Cursor, Continue, etc.)
- Cross-tool resource references
- Automatic resource conversion between tools
- Tool-specific resource validation

For the latest updates on multi-tool support, see the [GitHub Issues](https://github.com/aig787/agpm/issues) page.