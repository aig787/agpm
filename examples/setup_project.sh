#!/bin/bash
# Example setup script demonstrating CCPM with a local Git repository
# This script sets up a complete Claude Code project with agents, snippets,
# commands, and MCP servers from a local repository

set -e  # Exit on error

# Colors for output
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

echo -e "${BLUE}╔════════════════════════════════════════════╗${NC}"
echo -e "${BLUE}║     CCPM Example Project Setup Script      ║${NC}"
echo -e "${BLUE}╚════════════════════════════════════════════╝${NC}"
echo ""

# Get project name from argument or use default
PROJECT_NAME="${1:-test}"

# Clean up previous example if it exists
echo "→ Cleaning up previous example (if exists)"
rm -rf "examples/projects/$PROJECT_NAME"

# Setup paths
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$SCRIPT_DIR/projects/$PROJECT_NAME"
DEPS_DIR="$SCRIPT_DIR/deps"

# Ensure ccpm is built
echo "→ Building ccpm"
cd "$(dirname "$SCRIPT_DIR")"
cargo build --release

# Add to PATH for this script
export PATH="$PWD/target/release:$PATH"

echo ""
echo -e "${GREEN}✓ Using ccpm from: $(which ccpm)${NC}"
echo ""

# Create project directory
echo "→ Creating directory: $PROJECT_DIR"
mkdir -p "$PROJECT_DIR"
cd "$PROJECT_DIR"

# Initialize CCPM manifest
echo "→ Initializing CCPM manifest"
ccpm init

# Show initial project structure
echo ""
echo "→ Initial project structure:"
tree -a -L 3

# Add the local-deps source using local path
echo ""
echo "→ Adding source repository (local path)"
ccpm add source local-deps "$DEPS_DIR"

# IMPORTANT: This script uses only ccpm commands to manage dependencies
# We do not manually edit the ccpm.toml file - all dependencies are added
# via the 'ccpm add dep' subcommands to ensure proper manifest structure

# Add agents
echo ""
echo "→ Adding agents to manifest"
ccpm add dep agent local-deps:agents/rust-haiku.md --name rust-haiku
ccpm add dep agent local-deps:agents/javascript-haiku.md --name javascript-haiku

# Add snippets  
echo ""
echo "→ Adding snippets to manifest"
ccpm add dep snippet local-deps:snippets/error-analysis.md --name error-analysis
ccpm add dep snippet local-deps:snippets/unit-test-creation.md --name unit-tests
ccpm add dep snippet local-deps:snippets/security-review.md --name security-review
ccpm add dep snippet local-deps:snippets/rest-api-endpoint.md --name rest-api
ccpm add dep snippet local-deps:snippets/test-coverage.md --name test-coverage

# Add commands
echo ""
echo "→ Adding commands to manifest"
ccpm add dep command local-deps:commands/git-auto-commit.md --name git-auto-commit
ccpm add dep command local-deps:commands/format-json.md --name format-json

# Add MCP servers
echo ""
echo "→ Adding MCP servers to manifest"
ccpm add dep mcp-server local-deps:mcp-servers/github-mcp.json --name github --mcp-command npx --mcp-args=-y,@modelcontextprotocol/server-github
ccpm add dep mcp-server local-deps:mcp-servers/sqlite-mcp.json --name sqlite --mcp-command uvx --mcp-args=mcp-server-sqlite,--db,./data/local.db


# Show the generated manifest
echo ""
echo "→ Generated ccpm.toml:"
cat ccpm.toml

# Validate the manifest
echo ""
echo "→ Validating manifest"
ccpm validate

# Install dependencies
echo ""
echo "→ Installing dependencies with CCPM"
ccpm install

# List installed resources
echo ""
echo "→ Listing installed resources"
ccpm list

# Show final structure
echo ""
echo "→ Final project structure:"
tree -a -L 3

echo ""
echo -e "${GREEN}╔════════════════════════════════════════════╗${NC}"
echo -e "${GREEN}║           Setup Complete! 🎉               ║${NC}"
echo -e "${GREEN}╚════════════════════════════════════════════╝${NC}"
echo ""
echo "Your Claude Code project '$PROJECT_NAME' is ready with:"
echo "  • 2 agents"
echo "  • 5 snippets"
echo "  • 2 commands"
echo "  • 2 MCP servers"
echo ""
echo "Project location: $PROJECT_DIR"
echo ""