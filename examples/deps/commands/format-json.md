---
title: Format JSON
description: Pretty-print and validate JSON files
author: AGPM Team
version: 1.0.0
tags:
  - json
  - formatting
  - validation
dependencies:
  agents:
    - path: agents/javascript-haiku.md
  snippets:
    - path: snippets/data-validation.md
---

# Format JSON

A command-line tool to format, validate, and manipulate JSON files.

## Usage

```bash
# Format a JSON file
format-json input.json

# Format with custom indentation
format-json --indent 4 input.json

# Validate without formatting
format-json --validate-only input.json

# Sort keys alphabetically
format-json --sort-keys input.json

# Output to different file
format-json input.json -o output.json
```

## Features

- **Pretty Printing**: Format JSON with configurable indentation
- **Validation**: Check JSON syntax and report errors with line numbers
- **Key Sorting**: Sort object keys alphabetically for consistency
- **Minification**: Remove whitespace for compact output
- **Schema Validation**: Validate against JSON Schema (optional)
- **Path Extraction**: Extract specific paths using JSONPath syntax

## Options

- `--indent, -i`: Indentation level (default: 2)
- `--sort-keys, -s`: Sort object keys alphabetically
- `--minify, -m`: Minify output (remove whitespace)
- `--validate-only, -v`: Only validate, don't output
- `--output, -o`: Output file (default: stdout)
- `--schema`: JSON Schema file for validation
- `--extract`: JSONPath expression to extract

## Examples

```bash
# Format package.json with 4-space indent
format-json --indent 4 package.json

# Validate API response
curl https://api.example.com/data | format-json --validate-only

# Extract specific field
format-json --extract "$.users[0].name" data.json

# Sort keys and save to new file
format-json --sort-keys config.json -o config.formatted.json
```