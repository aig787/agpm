# Markdown Templating

AGPM supports powerful Tera-based templating for Markdown resources, enabling dynamic content generation during installation. Templates allow resources to reference each other, access installation metadata, and adapt to different environments.

## Table of Contents

- [Overview](#overview)
- [Template Variables Reference](#template-variables-reference)
- [Syntax and Features](#syntax-and-features)
- [Examples](#examples)
- [Controlling Templating](#controlling-templating)
- [Security and Sandboxing](#security-and-sandboxing)
- [Migration Guide](#migration-guide)

## Overview

When you install a Markdown resource (agent, snippet, command, etc.) that has templating enabled via frontmatter, AGPM processes any template syntax it contains. This allows you to:

- **Reference other resources**: Access install paths and versions of dependencies
- **Access installation metadata**: Use project paths, versions, and source information
- **Use conditional logic**: Show/hide content based on context
- **Iterate over collections**: Loop through dependencies or other data
- **Normalize paths**: Convert between platform-specific path formats

Templates use the [Tera](https://keats.github.io/tera/) template engine with a restricted, secure configuration.

**Templating is opt-in**: Resources must explicitly enable templating in their YAML frontmatter by setting `agpm.templating: true`. By default, all template syntax is preserved as literal text.

## Template Variables Reference

This is the canonical reference for all variables available in AGPM templates. All documentation references this table.

### Current Resource Variables

| Variable | Type | Description | Example Value |
|----------|------|-------------|---------------|
| `agpm.resource.type` | string | Resource type | `agent`, `snippet`, `command`, `mcp-server`, `script`, `hook` |
| `agpm.resource.name` | string | Logical manifest name | `helper-snippet` |
| `agpm.resource.install_path` | string | Resolved install target (platform-native separators*) | `.claude/agents/helper.md` |
| `agpm.resource.source` | string \| null | Source identifier (null for local resources) | `community` |
| `agpm.resource.version` | string \| null | Resolved version (null for local resources) | `v1.2.0` |
| `agpm.resource.resolved_commit` | string \| null | Git SHA if applicable | `abc123def456...` |
| `agpm.resource.checksum` | string | SHA256 checksum of content | `sha256:...` |
| `agpm.resource.path` | string | Source-relative path in repository | `agents/helper.md` |

**\*Platform-Native Path Separators**: The `install_path` variable uses backslashes on Windows (`.claude\agents\helper.md`) and forward slashes on Unix/macOS (`.claude/agents/helper.md`). This ensures paths match the user's platform conventions. See [Cross-Platform Path Handling](#cross-platform-path-handling) for details.

### Dependency Variables

Dependencies declared in YAML frontmatter are available in templates, organized by category and accessed by their logical name.

| Variable Pattern | Type | Description | Example |
|-----------------|------|-------------|---------|
| `agpm.deps.<category>.<name>.install_path` | string | Install path for dependency | `.agpm/snippets/utils.md` |
| `agpm.deps.<category>.<name>.version` | string \| null | Dependency version | `v1.0.0` |
| `agpm.deps.<category>.<name>.resolved_commit` | string \| null | Dependency commit SHA | `def456...` |
| `agpm.deps.<category>.<name>.checksum` | string | Dependency checksum | `sha256:...` |
| `agpm.deps.<category>.<name>.source` | string \| null | Dependency source | `community` |
| `agpm.deps.<category>.<name>.path` | string | Source-relative path | `snippets/utils.md` |

**Category Names** (plural forms): `agents`, `snippets`, `commands`, `scripts`, `hooks`, `mcp-servers`

**Example - Declaring and Accessing Dependencies**:

First, declare dependencies in your resource's YAML frontmatter:
```yaml
---
dependencies:
  snippets:
    - path: snippets/helper-utils.md
      version: v1.0.0
  agents:
    - path: agents/code-reviewer.md
---
```

Then access them in your template (note: hyphens in filenames become underscores in variable names):
```jinja2
{{ agpm.deps.snippets.helper_utils.install_path }}
{{ agpm.deps.agents.code_reviewer.version }}
```

The variable name comes from the filename (not the path), with hyphens converted to underscores.

### Important Notes

**Resource Name Sanitization**: Resource names containing hyphens are automatically converted to underscores in template variable names to avoid conflicts with Tera's minus operator. For example:
- A resource named `helper-snippet` in your manifest
- Is accessed in templates as `helper_snippet`
- Example: `{{ agpm.deps.snippets.helper_snippet.install_path }}`

## Syntax and Features

### Variable Substitution

Use double curly braces to insert variables:

```markdown
# {{ agpm.resource.name }}

This resource is installed at: `{{ agpm.resource.install_path }}`
Version: {{ agpm.resource.version }}
```

### Conditional Logic

Use `{% if %}` blocks for conditional content:

```markdown
{% if agpm.resource.source %}
This resource is from the {{ agpm.resource.source }} source.
{% else %}
This is a local resource.
{% endif %}

{% if agpm.resource.version %}
Version: {{ agpm.resource.version }}
{% endif %}
```

### Loops

Iterate over dependencies or other collections:

```markdown
## Available Helpers

{% for name, snippet in agpm.deps.snippets %}
- **{{ name }}**: `{{ snippet.install_path }}` ({{ snippet.version }})
{% endfor %}
```

### Comments

Use `{# #}` for template comments (not included in output):

```markdown
{# This comment won't appear in the installed file #}
# {{ agpm.resource.name }}
```

## Examples

### Basic Agent with Metadata

```markdown
---
title: {{ agpm.resource.name }}
---
# {{ agpm.resource.name }}

**Version**: {{ agpm.resource.version }}
**Install Location**: `{{ agpm.resource.install_path }}`
**Source**: {{ agpm.resource.source }}

## Description

This agent is managed by AGPM and automatically installed.
```

### Agent Referencing Dependencies

First, declare dependencies in frontmatter:

```yaml
---
title: Code Reviewer
dependencies:
  snippets:
    - path: snippets/style-guide.md
      version: v1.0.0
    - path: snippets/best-practices.md
      version: v1.0.0
  agents:
    - path: agents/documentation-helper.md
      version: v2.0.0
---
```

Then reference them in your template:

```markdown
# Code Reviewer Agent

This agent uses the following helper resources:

{% if agpm.deps.snippets %}
## Helper Snippets
{% for name, snippet in agpm.deps.snippets %}
- [{{ name }}]({{ snippet.install_path }}) - {{ snippet.version }}
{% endfor %}
{% endif %}

{% if agpm.deps.agents.documentation_helper %}
## Related Agent
This reviewer works with the [Documentation Helper]({{ agpm.deps.agents.documentation_helper.install_path }}).
{% endif %}
```

**Note**: The loop variable `name` will contain the sanitized filename with underscores (e.g., `style_guide`, `best_practices`), not the original filename with hyphens.

### Conditional Content

```markdown
# Installation Info

{% if agpm.resource.source %}
This resource is from the **{{ agpm.resource.source }}** repository ({{ agpm.resource.version }}).
{% else %}
This is a local resource.
{% endif %}

Install location: `{{ agpm.resource.install_path }}`
```

### Dynamic Documentation

```markdown
---
title: Project Setup
---
# Resource Dependencies

This {{ agpm.resource.type }} resource has the following dependencies:

{% if agpm.deps.agents %}
## Agents ({{ agpm.deps.agents | length }})
{% for name, agent in agpm.deps.agents %}
- `{{ agent.install_path }}` - {{ agent.version }}
{% endfor %}
{% endif %}

{% if agpm.deps.snippets %}
## Snippets ({{ agpm.deps.snippets | length }})
{% for name, snippet in agpm.deps.snippets %}
- `{{ snippet.install_path }}` - {{ snippet.version }}
{% endfor %}
{% endif %}
```

## Controlling Templating

### Enabling Templating Per-Resource

Templating is **disabled by default** for all resources. To enable template processing for a specific resource, add `templating: true` to its YAML frontmatter:

```markdown
---
title: My Agent
agpm:
  templating: true
---
# {{ agpm.resource.name }}

This resource will have its template syntax processed during installation.
```

### Disabling Templating (Default)

By default, all template syntax is kept literal and not processed. To explicitly document this intent, you can set `templating: false`:

```markdown
---
title: My Agent
agpm:
  templating: false
---
# This file contains literal {{ template.syntax }}

The template syntax above will be preserved as-is.
```

This default behavior is useful for:
- Resources that contain literal template syntax for documentation
- Example code that shows template usage
- Resources that don't need dynamic content

### Files Without Template Syntax

Plain Markdown files without any `{{`, `{%`, or `{#` syntax are passed through unchanged with minimal overhead.

## Caching Behavior

AGPM intelligently caches rendered template output to improve installation performance. Understanding how caching works helps you predict when re-rendering will occur.

### Cache Key Components

The cache is based on two factors:

1. **Source file content**: The raw Markdown file content (before rendering)
2. **Template context**: Dependency versions, installation paths, and other metadata from the lockfile

When either component changes, the cache is invalidated and the template is re-rendered.

### Automatic Cache Invalidation

Templates are automatically re-rendered when:

- **Source file changes**: Any modification to the Markdown file content
- **Dependency version updates**: A dependency updates to a new version (even if the dependency's file content hasn't changed)
- **Dependency path changes**: A dependency's installation path changes
- **New dependencies added**: Additional resources are added to the lockfile
- **Dependencies removed**: Resources are removed from the lockfile

### Cache Hits

Templates are NOT re-rendered when:

- **Source and context unchanged**: Both the source file and all dependency metadata remain identical
- **Unrelated dependency changes**: Changes to dependencies not referenced in the template
- **Non-templated files**: Plain Markdown files without template syntax skip rendering entirely

### Example Scenarios

**Scenario 1: Dependency version update**
```yaml
# Before: agpm.toml
[snippets]
helper = { source = "community", path = "snippets/helper.md", version = "v1.0.0" }

# After: agpm.toml
helper = { source = "community", path = "snippets/helper.md", version = "v1.1.0" }
```

**Result**: Any agent using `{{ agpm.deps.snippets.helper.version }}` will be re-rendered with the new version, even if `helper.md`'s content didn't change.

**Scenario 2: Unrelated dependency change**
```markdown
# agent.md
---
dependencies:
  snippets:
    - path: snippets/helper.md
---
# My Agent

Uses helper: {{ agpm.deps.snippets.helper.version }}
```

If `snippets/other-unrelated.md` updates, `agent.md` will NOT be re-rendered because it doesn't reference the changed dependency.

### Force Refresh

To bypass the cache and force re-rendering of all templates:

```bash
agpm install --force-refresh
```

This is useful for:
- Debugging template rendering issues
- Verifying that templates produce expected output
- Recovering from corrupted cache state (rare)

**Note**: `--force-refresh` re-renders ALL templates, which may be slower for large projects. Normal cache invalidation handles most scenarios automatically.

## Security and Sandboxing

AGPM's templating engine is configured with strict security restrictions:

### Disabled Features

For safety, the following Tera features are **disabled**:
- `{% include %}` tags (no file system access)
- `{% extends %}` tags (no template inheritance)
- `{% import %}` tags (no external template imports)
- Custom functions that access the file system or network

### Safe Operations

The following operations are fully supported and safe:
- Variable substitution
- Conditional logic (`{% if %}`)
- Loops (`{% for %}`)
- Built-in filters (string manipulation, formatting)
- Template comments

### Error Handling

If a template fails to render:
- **Syntax errors**: Install fails with a descriptive error message
- **Unknown variables**: Install fails with suggestions for available variables
- **Missing dependencies**: Clear error indicating which dependency is missing

## Migration Guide

### Upgrading Existing Resources

If you have existing Markdown resources with hard-coded paths:

**Before (hard-coded)**:
```markdown
This agent is installed at `.claude/agents/helper.md`.
See also: `.claude/snippets/utils.md`
```

**After (templated)** (note: hyphens become underscores):
```markdown
This agent is installed at `{{ agpm.resource.install_path }}`.
See also: `{{ agpm.deps.snippets.utils_snippet.install_path }}`
```

### Escaping Literal Braces

If you need literal `{{` or `}}` characters in your documentation:

```markdown
To use Tera syntax, write: {{ "{{" }} variable {{ "}}" }}
Or use raw blocks for code examples:

{% raw %}
{{ this.is.literal.syntax }}
{% endraw %}
```

### Testing Templates

Before committing templated resources:

1. Install locally to verify rendering:
   ```bash
   agpm install
   cat .claude/agents/your-agent.md
   ```

2. Check for template errors in the output

3. Verify all dependency references resolve correctly

### Gradual Adoption

You can mix templated and non-templated resources:
- New resources can use templates immediately
- Existing resources can be updated incrementally
- Use `agpm: { templating: false }` for resources that should remain static

## Best Practices

1. **Use descriptive variable names in manifests** - Template references use manifest names (sanitized with underscores)
2. **Avoid hyphens in resource names** - Use underscores instead to avoid confusion with template variable names
3. **Test with different dependency combinations** - Ensure conditionals work when dependencies are missing
4. **Document template variables** - Add comments explaining what each template section does
5. **Keep templates simple** - Avoid complex logic for better maintainability
6. **Test locally first** - Always install and verify templated resources locally before committing
7. **Understand cross-platform path behavior** - Template paths use platform-native separators (see below)

### Cross-Platform Path Handling

Template variables like `{{ agpm.resource.install_path }}` automatically use platform-native path separators:

- **Windows**: Paths render with backslashes (`.claude\agents\helper.md`)
- **Unix/macOS**: Paths render with forward slashes (`.claude/agents/helper.md`)

This ensures that paths in installed content match what users see in their file explorer. However, **lockfiles always use forward slashes** for cross-platform compatibility, so teams on different platforms can share the same `agpm.lock` file.

**Example**: A template like this:

```markdown
This agent is installed at: {{ agpm.resource.install_path }}
```

Will render differently based on platform:

- **Windows**: `This agent is installed at: .claude\agents\example.md`
- **Unix/macOS**: `This agent is installed at: .claude/agents/example.md`

This means the **installed content will differ by platform**, but the lockfile remains consistent.

## Troubleshooting

### "Template rendering failed"

- **Cause**: Syntax error in template
- **Solution**: Check the error message for line/column information, verify bracket matching

### "Unknown variable: agpm.deps.snippets.xyz"

- **Cause**: Referenced dependency not in lockfile
- **Solution**: Ensure the dependency is declared in `agpm.toml` and installed

### Template syntax not processed

- **Cause**: Templating disabled by default (resources must opt-in via frontmatter)
- **Solution**: Add `templating: true` to the resource's YAML frontmatter under the `agpm` key

### "Variable not found" with hyphenated names

- **Cause**: Resource names with hyphens are sanitized to underscores
- **Solution**: Use underscores in template variable names (e.g., `helper_utils` instead of `helper-utils`)

## See Also

- [Tera Template Documentation](https://keats.github.io/tera/docs/) - Full Tera syntax reference
- [AGPM Manifest Reference](manifest.md) - How to declare dependencies
- [AGPM CLI Reference](cli/) - Command-line flags and options
