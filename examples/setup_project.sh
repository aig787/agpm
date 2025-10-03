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

echo -e "${BLUE}╔════════════════════════════════════════════════════════════════════╗${NC}"
echo -e "${BLUE}║          CCPM Example Project Setup Script                         ║${NC}"
echo -e "${BLUE}║        Demonstrating Transitive Dependency Resolution              ║${NC}"
echo -e "${BLUE}╚════════════════════════════════════════════════════════════════════╝${NC}"
echo ""

# Get project name from argument or use default
PROJECT_NAME="${1:-test}"

# Setup paths
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$SCRIPT_DIR/projects/$PROJECT_NAME"
DEPS_DIR="$SCRIPT_DIR/deps"

# Clean up previous example if it exists
echo "→ Cleaning up previous example (if exists)"
rm -rf "$PROJECT_DIR"


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
tree -a -L 4

# Add the local-deps source (local directory)
echo ""
echo "→ Adding local-deps source (local directory)"
ccpm add source local-deps "$DEPS_DIR"

# Add the ccpm-community GitHub repository
echo ""
echo "→ Adding ccpm-community GitHub repository"
ccpm add source community "https://github.com/aig787/ccpm-community.git"

# Add resources with transitive dependencies via commands (using local source)
echo ""
echo -e "${YELLOW}→ Adding commands (which have transitive dependencies)${NC}"
echo "  - git-auto-commit depends on: rust-haiku agent, commit-message snippet"
echo "  - format-json depends on: javascript-haiku agent, data-validation snippet"
ccpm add dep command local-deps:commands/git-auto-commit.md --name git-auto-commit
ccpm add dep command local-deps:commands/format-json.md --name format-json

echo ""
echo -e "${YELLOW}→ Adding agents (which also have dependencies)${NC}"
echo "  - rust-haiku depends on: error-analysis, unit-test-creation snippets"
echo "  - javascript-haiku depends on: test-automation, data-validation snippets"
ccpm add dep agent local-deps:agents/rust-haiku.md --name rust-haiku
ccpm add dep agent local-deps:agents/javascript-haiku.md --name javascript-haiku

echo ""
echo -e "${YELLOW}→ Adding base snippets (dependencies of other resources)${NC}"
ccpm add dep snippet local-deps:snippets/error-analysis.md --name error-analysis
ccpm add dep snippet local-deps:snippets/unit-test-creation.md --name unit-tests
ccpm add dep snippet local-deps:snippets/commit-message.md --name commit-message
ccpm add dep snippet local-deps:snippets/data-validation.md --name data-validation
ccpm add dep snippet local-deps:snippets/test-automation.md --name test-automation

echo ""
echo "→ Adding 2 scripts via command"
ccpm add dep script local-deps:scripts/build.sh --name build
ccpm add dep script local-deps:scripts/test.js --name test

echo ""
echo "→ Adding 2 hooks via command"
ccpm add dep hook local-deps:hooks/pre-tool-use.json --name pre-tool-use
ccpm add dep hook local-deps:hooks/user-prompt-submit.json --name user-prompt-submit

echo ""
echo "→ Adding 2 MCP servers via command"
ccpm add dep mcp-server local-deps:mcp-servers/filesystem.json --name filesystem
ccpm add dep mcp-server local-deps:mcp-servers/fetch.json --name fetch

echo ""
echo "→ Adding remaining resources directly to ccpm.toml"
cat >> ccpm.toml << 'EOF'

[agents]
# Additional agents from ccpm-community
api-designer = { source = "community", path = "agents/awesome-claude-code-subagents/categories/01-core-development/api-designer.md", version = "v0.0.1" }
backend-developer = { source = "community", path = "agents/awesome-claude-code-subagents/categories/01-core-development/backend-developer.md", version = "^v0.0.1" }
frontend-developer = { source = "community", path = "agents/awesome-claude-code-subagents/categories/01-core-development/frontend-developer.md", version = "=v0.0.1" }
python-pro = { source = "community", path = "agents/awesome-claude-code-subagents/categories/02-language-specialists/python-pro.md", version = "v0.0.1" }
rust-engineer = { source = "community", path = "agents/awesome-claude-code-subagents/categories/02-language-specialists/rust-engineer.md", version = "v0.0.1" }
javascript-pro = { source = "community", path = "agents/awesome-claude-code-subagents/categories/02-language-specialists/javascript-pro.md", version = "v0.0.1" }
database-administrator = { source = "community", path = "agents/awesome-claude-code-subagents/categories/03-infrastructure/database-administrator.md", version = "v0.0.1" }
code-reviewer = { source = "community", path = "agents/awesome-claude-code-subagents/categories/04-quality-security/code-reviewer.md", version = "v0.0.1" }
test-automator = { source = "community", path = "agents/awesome-claude-code-subagents/categories/04-quality-security/test-automator.md", version = "v0.0.1" }
security-auditor = { source = "community", path = "agents/awesome-claude-code-subagents/categories/04-quality-security/security-auditor.md", version = "v0.0.1" }
devops-engineer = { source = "community", path = "agents/awesome-claude-code-subagents/categories/03-infrastructure/devops-engineer.md", version = "v0.0.1" }
cloud-architect = { source = "community", path = "agents/awesome-claude-code-subagents/categories/03-infrastructure/cloud-architect.md", version = "v0.0.1" }
documentation-engineer = { source = "community", path = "agents/awesome-claude-code-subagents/categories/06-developer-experience/documentation-engineer.md", version = "v0.0.1" }
ml-engineer = { source = "community", path = "agents/awesome-claude-code-subagents/categories/05-data-ai/ml-engineer.md", version = "v0.0.1" }
multi-agent-coordinator = { source = "community", path = "agents/awesome-claude-code-subagents/categories/09-meta-orchestration/multi-agent-coordinator.md", version = "v0.0.1" }

[snippets]
# Additional snippets (using local source)
security-review = { source = "local-deps", path = "snippets/security-review.md" }
rest-api = { source = "local-deps", path = "snippets/rest-api-endpoint.md" }
test-coverage = { source = "local-deps", path = "snippets/test-coverage.md" }
EOF


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
# Remove lockfile since we appended to the manifest
rm -f ccpm.lock
ccpm install

# List installed resources
echo ""
echo "→ Listing installed resources"
ccpm list

# Update dependencies
echo ""
echo "→ Updating dependathies with CCPM"
ccpm update


echo ""
echo -e "${GREEN}╔════════════════════════════════════════════════════════════════════╗${NC}"
echo -e "${GREEN}║                    Setup Complete! 🎉                              ║${NC}"
echo -e "${GREEN}╚════════════════════════════════════════════════════════════════════╝${NC}"
echo ""
echo "Your Claude Code project '$PROJECT_NAME' is ready:"
echo ""
ccpm tree

# Show final structure
echo ""
echo "→ Final project structure:"
tree -a -L 4

echo ""
echo "Project location: $PROJECT_DIR"
echo ""
echo "To clean up this project, run:"
echo "  ./examples/cleanup_project.sh $PROJECT_NAME"
echo ""
