# Resources Guide

AGPM manages six types of resources for AI coding assistants (Claude Code, OpenCode, and more), divided into two categories
based on how they're integrated. Resources can target different tools through the tool configuration system, enabling you to
manage resources for multiple AI assistants from a single manifest.

## Tool Configuration System

AGPM routes resources to different tools based on the `type` field:

- **claude-code** (default) - Claude Code resources with full feature support ✅ **Stable**
- **opencode** - OpenCode resources for agents, commands, and MCP servers 🚧 **Alpha**
- **agpm** - Shared snippets usable across tools ✅ **Stable**
- **custom** - Define your own custom tools

> ⚠️ **Alpha Feature**: OpenCode support is currently in alpha. While functional, it may have incomplete features or breaking
> changes in future releases. Claude Code support is stable and production-ready.

**Default Behavior**: Resources without an explicit `type` field default to `claude-code`, except for `snippets` which default to `agpm` (shared infrastructure).

**Example**:
```toml
[agents]
# Installs to .claude/agents/helper.md
claude-helper = { source = "community", path = "agents/helper.md", version = "v1.0.0" }

# Installs to .opencode/agent/helper.md
opencode-helper = { source = "community", path = "agents/helper.md", version = "v1.0.0", tool = "opencode" }
```

## Resource Categories

### Direct Installation Resources

These resources are copied directly to their target directories and used as standalone files:

- **Agents** - AI assistant configurations
- **Snippets** - Reusable code templates
- **Commands** - Claude Code slash commands
- **Scripts** - Executable automation files

### Configuration-Merged Resources

These resources have their configurations merged into Claude Code's settings files (no separate directory installation):

- **Hooks** - Event-based automation
- **MCP Servers** - Model Context Protocol servers

## Resource Types

### Agents

AI assistant configurations with prompts and behavioral definitions.

**Default Locations**:
- **Claude Code**: `.claude/agents/` ✅ **Stable**
- **OpenCode**: `.opencode/agent/` (singular) 🚧 **Alpha**

**Path Preservation**: AGPM preserves the source directory structure during installation.

**Examples**:
```toml
[agents]
# Claude Code - installed as .claude/agents/rust-expert.md
rust-expert = { source = "community", path = "agents/rust-expert.md", version = "v1.0.0" }

# OpenCode - installed as .opencode/agent/rust-expert.md (note: singular "agent")
rust-expert-oc = { source = "community", path = "agents/rust-expert.md", version = "v1.0.0", tool = "opencode" }

# Nested path - installed as .claude/agents/code-reviewer.md (flatten=true by default)
code-reviewer = { source = "community", path = "agents/ai/code-reviewer.md", version = "v1.0.0" }

# OpenCode nested - installed as .opencode/agent/code-reviewer.md (flatten=true by default)
code-reviewer-oc = { source = "community", path = "agents/ai/code-reviewer.md", version = "v1.0.0", tool = "opencode" }

# Preserve directory structure with flatten=false
nested-reviewer = { source = "community", path = "agents/ai/code-reviewer.md", version = "v1.0.0", flatten = false }
# → .claude/agents/ai/code-reviewer.md

# Local path - still flattens by default
local-agent = { path = "../local-agents/ai/helper.md" }  # → .claude/agents/helper.md
```

**Directory Naming Note**: OpenCode uses singular directory names (`agent`, `command`) while Claude Code uses plural
(`agents`, `commands`). AGPM handles this automatically based on the `type` field.

### Snippets

Reusable code templates and documentation fragments.

**Default Location**: `.agpm/snippets/` ✅ **Stable** (AGPM tool)

**Alternative Location**: `.claude/snippets/` (explicitly set `tool = "claude-code"`)

**Default Behavior**: Snippets automatically default to the `agpm` tool, meaning they install to `.agpm/snippets/`
by default. This is because snippets are designed as shared content that can be referenced by resources from multiple
tools (Claude Code, OpenCode, etc.).

**Example**:
```toml
[snippets]
# Default: installs to .agpm/snippets/ (agpm tool is the default)
react-component = { source = "community", path = "snippets/react-component.md", version = "v1.2.0" }

# Same as above - snippets are shared by default
rust-patterns = { source = "community", path = "snippets/rust-patterns.md", version = "v1.0.0" }

# Claude Code specific: explicitly override to install to .claude/snippets/
claude-only = { source = "community", path = "snippets/claude.md", version = "v1.0.0", tool = "claude-code" }

utils = { source = "local-deps", path = "snippets/utils.md" }
```

### Commands

Slash commands that extend AI assistant functionality.

**Default Locations**:
- **Claude Code**: `.claude/commands/` ✅ **Stable**
- **OpenCode**: `.opencode/command/` (singular) 🚧 **Alpha**

**Example**:
```toml
[commands]
# Claude Code command
deploy = { source = "community", path = "commands/deploy.md", version = "v2.0.0" }

# OpenCode command
deploy-oc = { source = "community", path = "commands/deploy.md", version = "v2.0.0", tool = "opencode" }

lint = { source = "tools", path = "commands/lint.md", branch = "main" }
```

### Scripts

Executable files (.sh, .js, .py, etc.) that can be run by hooks or independently.

**Default Location**: `.claude/scripts/`

**Example**:
```toml
[scripts]
security-check = { source = "security-tools", path = "scripts/security.sh", version = "v1.0.0" }
build = { source = "tools", path = "scripts/build.js", version = "v2.0.0" }
validate = { source = "local", path = "scripts/validate.py" }
```

Scripts must be executable and can be written in any language supported by your system.

### Hooks

Event-based automation configurations for Claude Code. JSON files that define when to run scripts.

**Merge Target**: Automatically merged into `.claude/settings.local.json` (no separate directory installation)

**How It Works**: Instead of installing hook files to a directory, AGPM merges their configuration into Claude Code's settings file. This allows Claude Code to natively recognize and execute the hooks.

#### Hook Structure

```json
{
  "events": ["PreToolUse"],
  "matcher": "Bash|Write|Edit",
  "type": "command",
  "command": ".claude/scripts/security-check.sh",
  "timeout": 5000,
  "description": "Security validation before file operations"
}
```

#### Available Events

- `PreToolUse` - Before a tool is executed
- `PostToolUse` - After a tool completes
- `UserPromptSubmit` - When user submits a prompt
- `UserPromptReceive` - When prompt is received
- `AssistantResponseReceive` - When assistant responds

#### Example Configuration

```toml
[hooks]
pre-bash = { source = "security-tools", path = "hooks/pre-bash.json", version = "v1.0.0" }
file-guard = { source = "security-tools", path = "hooks/file-guard.json", version = "v1.0.0" }
```

### MCP Servers

Model Context Protocol servers that extend AI assistant capabilities with external tools and APIs.

**Merge Targets**: Configuration automatically merged into tool-specific files (no separate directory installation)
- **Claude Code**: `.mcp.json` ✅ **Stable**
- **OpenCode**: `.opencode/opencode.json` 🚧 **Alpha**

**How It Works**: AGPM uses pluggable MCP handlers to:
1. Read MCP server JSON configurations
2. Merge them into the tool's native configuration file
3. Track managed servers with `_agpm` metadata
4. Preserve user-configured servers alongside AGPM-managed ones

AGPM routes configuration to the correct file based on the `tool` field (defaults to `claude-code`).

#### MCP Server Structure

```json
{
  "command": "npx",
  "args": [
    "-y",
    "@modelcontextprotocol/server-filesystem",
    "--root",
    "./data"
  ],
  "env": {
    "NODE_ENV": "production"
  }
}
```

#### Example Configuration

```toml
[mcp-servers]
# Claude Code - merges into .mcp.json
filesystem = { source = "community", path = "mcp-servers/filesystem.json", version = "v1.0.0" }
github = { source = "community", path = "mcp-servers/github.json", version = "v1.2.0" }

# OpenCode - merges into opencode.json
filesystem-oc = { source = "community", path = "mcp-servers/filesystem.json", version = "v1.0.0", tool = "opencode" }

postgres = { source = "local-deps", path = "mcp-servers/postgres.json" }
```

## Configuration Merging

### How It Works

Configuration-merged resources (Hooks and MCP Servers) follow a specialized installation process:

1. **Configuration Processing**: JSON configurations are processed by AGPM
2. **Tool Routing**: Settings are automatically routed to the correct tool's configuration file based on the `tool` field
3. **Configuration Merging**: Settings are merged into the target configuration file (merge target)
4. **Non-destructive Updates**: AGPM preserves user-configured entries while managing its own
5. **Tracking**: AGPM adds `_agpm` metadata to track which entries it manages

### Merge Targets

Merge targets define where configuration-merged resources are installed:

**Built-in Merge Targets**:
- **Hooks** (claude-code): `.claude/settings.local.json`
- **MCP Servers** (claude-code): `.mcp.json`
- **MCP Servers** (opencode): `.opencode/opencode.json`

**Custom Merge Targets**:

You can override merge targets for custom tools or alternative configurations:

```toml
# Override Claude Code MCP merge target
[tools.claude-code.resources.mcp-servers]
merge-target = ".claude/my-mcp-config.json"

# Define custom tool with merge targets
[tools.my-tool]
path = ".my-tool"

[tools.my-tool.resources.hooks]
merge-target = ".my-tool/hooks.json"

[tools.my-tool.resources.mcp-servers]
merge-target = ".my-tool/servers.json"
```

**Note**: Use `merge-target` (with a hyphen) in TOML, not `merge_target` (with underscore).

**Note**: Custom tools require MCP handlers for hooks/MCP servers. Only built-in tools (claude-code, opencode) have handlers. Custom merge targets work best by overriding defaults for built-in tools.

### Example: Merged .mcp.json

After installation, `.mcp.json` contains both user and AGPM-managed servers:

```json
{
  "mcpServers": {
    "my-manual-server": {
      "command": "node",
      "args": ["./custom.js"]
    },
    "filesystem": {
      "command": "npx",
      "args": [
        "-y",
        "@modelcontextprotocol/server-filesystem",
        "--root",
        "./data"
      ],
      "_agpm": {
        "managed": true,
        "config_file": ".mcp.json",
        "installed_at": "2024-01-15T10:30:00Z"
      }
    }
  }
}
```

## File Naming and Path Preservation

### Path Preservation

AGPM's path behavior depends on the resource type's `flatten` setting. By default, agents and commands flatten (use only filename), while snippets and scripts preserve directory structure:

```toml
[agents]
# Source file: agents/ai/code-reviewer.md
# Installed as: .claude/agents/code-reviewer.md (flatten=true by default for agents)
code-reviewer = { source = "community", path = "agents/ai/code-reviewer.md" }

# To preserve directory structure for agents, set flatten=false
# Installed as: .claude/agents/ai/code-reviewer.md
nested-reviewer = { source = "community", path = "agents/ai/code-reviewer.md", flatten = false }

[snippets]
# Source file: snippets/python/utils.md
# Installed as: .agpm/snippets/python/utils.md (flatten=false by default for snippets)
python-utils = { source = "community", path = "snippets/python/utils.md" }
```

### Flatten Behavior by Resource Type

| Resource Type | Default Flatten | Typical Use Case |
|--------------|----------------|------------------|
| agents | `true` | Single namespace for agent files |
| commands | `true` | Single namespace for command files |
| snippets | `false` | Preserve organizational structure |
| scripts | `false` | Preserve directory hierarchy |
| hooks | N/A | Merged into settings file |
| mcp-servers | N/A | Merged into MCP config file |

### Benefits of Flatten Control

- **Flexibility**: Choose between flat namespaces or hierarchical organization
- **Avoids conflicts**: Use `flatten=false` when files with the same name exist in different directories
- **Clear defaults**: Sensible behavior for each resource type
- **Pattern matching**: Control structure preservation for glob patterns

### Custom Filenames

You can use custom filenames with the `filename` option. The flatten setting still applies:

```toml
[agents]
# Custom filename with default flatten behavior
reviewer = {
    source = "community",
    path = "agents/ai/code-reviewer.md",
    filename = "my-reviewer.md"
}
# Installed as: .claude/agents/my-reviewer.md (flatten=true by default)

# Custom filename with flatten=false to preserve directory structure
structured-reviewer = {
    source = "community",
    path = "agents/ai/code-reviewer.md",
    filename = "my-reviewer.md",
    flatten = false
}
# Installed as: .claude/agents/ai/my-reviewer.md (preserves ai/ directory)
```

## Resource Frontmatter and Templating

Markdown resources (agents, snippets, commands) can include YAML frontmatter to control behavior and declare dependencies.

### YAML Frontmatter Structure

Frontmatter appears at the top of Markdown files between `---` delimiters:

```markdown
---
title: My Agent
description: A helpful agent
agpm:
  templating: false
dependencies:
  snippets:
    - path: snippets/utils.md
      version: v1.0.0
---
# Agent content starts here
```

### Templating Control

AGPM can process Tera template syntax in Markdown resources during installation. Templating is **disabled by default** and must be enabled per-resource via frontmatter.

**Enable templating for a resource:**
```markdown
---
agpm:
  templating: true
---
# {{ agpm.resource.name }}

This resource will have its template syntax processed during installation.
```

**Disable templating (default):**
```markdown
---
agpm:
  templating: false
---
# This file contains literal {{ template.syntax }}

The template syntax above will be preserved as-is.
```

### Template Variables

When templating is enabled in a resource's frontmatter, you can use template variables to create dynamic content:

```markdown
---
title: {{ agpm.resource.name }}
dependencies:
  snippets:
    - path: snippets/helper.md
---
# {{ agpm.resource.name }}

**Version**: {{ agpm.resource.version }}
**Install Location**: `{{ agpm.resource.install_path }}`

{% if agpm.deps.snippets.helper %}
See also: [Helper Snippet]({{ agpm.deps.snippets.helper.install_path }})
{% endif %}
```

**Available Variables:**
- `agpm.resource.*` - Current resource metadata (name, version, install_path, etc.)
- `agpm.deps.<category>.<name>.*` - Dependency metadata for resources declared in frontmatter

**Full Documentation**: See [Markdown Templating Guide](templating.md) for complete variable reference, examples, and best practices.

### Dependency Declarations

Resources can declare dependencies in frontmatter. AGPM automatically resolves and installs them:

```markdown
---
dependencies:
  agents:
    - path: agents/helper.md
      version: v1.0.0
  snippets:
    - path: snippets/utils.md
---
```

Dependencies are accessible in templates via `agpm.deps.<category>.<name>`. See the [Templating Guide](templating.md#template-variables-reference) for details.

## Custom Installation Paths

### Global Target Directories

Override default installation directories for all resources of a type:

```toml
[target]
agents = ".claude/agents"           # Default
snippets = ".agpm/snippets"         # Default (AGPM shared infrastructure)
commands = ".claude/commands"        # Default
scripts = ".claude/scripts"          # Default
# Note: hooks and mcp-servers are merged into config files, not directories

# Or use custom paths
agents = "custom/agents"
snippets = "resources/snippets"
```

**Flatten behavior applies to custom base directories too:**
```toml
[target]
agents = "my/agents"

[agents]
# Source: agents/ai/helper.md
# Installed as: my/agents/helper.md (flatten=true by default)
helper = { source = "community", path = "agents/ai/helper.md" }

# With flatten=false to preserve structure
# Installed as: my/agents/ai/helper.md
nested-helper = { source = "community", path = "agents/ai/helper.md", flatten = false }
```

### Per-Resource Custom Targets

Custom targets are relative to the default resource directory:

```toml
[agents]
example = { source = "community", path = "agents/example.md", target = "custom" }
# Installed as: .claude/agents/custom/example.md (target relative to agents directory)
```

**Examples with path preservation:**
```toml
[agents]
# Nested source with custom target
ai-helper = {
    source = "community",
    path = "agents/ai/helper.md",
    target = "specialized"
}
# Installed as: .claude/agents/specialized/ai/helper.md (preserves ai/ subdirectory)

# Nested target directory
reviewer = {
    source = "community",
    path = "agents/code-reviewer.md",
    target = "custom/reviews"
}
# Installed as: .claude/agents/custom/reviews/code-reviewer.md
```

## Version Control Strategy

By default, AGPM creates `.gitignore` entries to exclude installed files from Git:

- The `agpm.toml` manifest and `agpm.lock` lockfile are committed
- Installed resource files are automatically gitignored
- Team members run `agpm install` to get their own copies

To commit resources to Git instead:

```toml
[target]
gitignore = false  # Don't create .gitignore
```

## Pattern-Based Dependencies

Install multiple resources using glob patterns. Directory structure preservation depends on the resource type's flatten setting.

```toml
[agents]
# Install all AI agents - agents flatten by default (only filename)
# agents/ai/assistant.md → .claude/agents/assistant.md
# agents/ai/analyzer.md → .claude/agents/analyzer.md
ai-agents = { source = "community", path = "agents/ai/*.md", version = "v1.0.0" }

# Install all review tools recursively - flatten removes directory structure
# agents/code/review-expert.md → .claude/agents/review-expert.md
# agents/security/review-scanner.md → .claude/agents/review-scanner.md
review-tools = { source = "community", path = "agents/**/review*.md", version = "v1.0.0" }

# Preserve structure with flatten=false
# agents/code/review-expert.md → .claude/agents/code/review-expert.md
# agents/security/review-scanner.md → .claude/agents/security/review-scanner.md
structured-review = { source = "community", path = "agents/**/review*.md", version = "v1.0.0", flatten = false }

[snippets]
# All Python snippets - directory structure preserved
# snippets/python/utils.md → .agpm/snippets/python/utils.md
# snippets/python/helpers.md → .agpm/snippets/python/helpers.md
python-snippets = { source = "community", path = "snippets/python/*.md", version = "v1.0.0" }

# Multiple nested directories
# snippets/web/react/hooks.md → .agpm/snippets/web/react/hooks.md
# snippets/web/vue/composables.md → .agpm/snippets/web/vue/composables.md
web-snippets = { source = "community", path = "snippets/web/**/*.md", version = "v1.0.0" }
```

**Benefits**:
- **Organization preserved**: Complex directory hierarchies maintained automatically
- **No conflicts**: Files with identical names in different directories don't collide
- **Clear structure**: Installation mirrors source repository organization

## Best Practices

1. **Organize by Function**: Group related resources together
2. **Use Semantic Names**: Choose descriptive names for your dependencies
3. **Version Scripts with Hooks**: Keep scripts and their hook configurations in sync
4. **Test Locally First**: Use local sources during development
5. **Document Requirements**: Note any runtime requirements for scripts/MCP servers
6. **Preserve User Config**: Never manually edit merged configuration files

## Troubleshooting

### Scripts Not Executing

- Ensure scripts have executable permissions
- Check the script path in hook configuration
- Verify required interpreters are installed (bash, python, node, etc.)

### Hooks Not Triggering

- Check `.claude/settings.local.json` for the hook entry
- Verify the event name and matcher pattern
- Check hook timeout settings

### MCP Servers Not Starting

- Ensure required runtimes are installed (Node.js for npx, Python for uvx)
- Check `.mcp.json` for the server configuration
- Verify environment variables are set correctly

### Configuration Not Merging

- Run `agpm install` again to re-merge configurations
- Check for syntax errors in JSON files
- Ensure AGPM has write permissions to config files