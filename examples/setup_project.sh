#!/bin/bash

# CCPM Project Setup Script
# Creates a new project using CCPM commands

set -e

# Check if project name was provided
if [ $# -eq 0 ]; then
    echo "Usage: $0 <project-name>"
    echo "Example: $0 my-claude-project"
    exit 1
fi

PROJECT_NAME="$1"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$SCRIPT_DIR/projects/$PROJECT_NAME"
CCPM_ROOT="$SCRIPT_DIR/.."
DEPS_DIR="$SCRIPT_DIR/deps"

# Build CCPM first
echo "Building CCPM..."
(cd "$CCPM_ROOT" && cargo build)

# Function to run CCPM commands from the current directory
ccpm() {
    "$CCPM_ROOT/target/debug/ccpm" "$@"
}

echo ""
echo "Setting up CCPM project: $PROJECT_NAME"
echo "================================"

# Remove existing project directory if it exists
if [ -d "$PROJECT_DIR" ]; then
    echo "→ Removing existing project directory: $PROJECT_DIR"
    rm -rf "$PROJECT_DIR"
fi

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

# Add the local-deps source
echo ""
echo "→ Adding source repository"
ccpm add source local-deps "file://$DEPS_DIR"

# Add agents
echo ""
echo "→ Adding agents"
ccpm add dep --agent --name rust-haiku "local-deps:agents/rust-haiku.md@v1.1"
ccpm add dep --agent --name javascript-haiku "local-deps:agents/javascript-haiku.md@v1.2"

# Add snippets
echo ""
echo "→ Adding snippets"
ccpm add dep --snippet --name error-analysis "local-deps:snippets/error-analysis.md@main"
ccpm add dep --snippet --name unit-tests "local-deps:snippets/unit-test-creation.md@main"
ccpm add dep --snippet --name security-review "local-deps:snippets/security-review.md@main"
ccpm add dep --snippet --name rest-api "local-deps:snippets/rest-api-endpoint.md@main"
ccpm add dep --snippet --name test-coverage "local-deps:snippets/test-coverage.md@main"

echo ""
echo "→ Adding snippets"


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

# Show project structure with tree
echo ""
echo "→ Project structure:"
tree -a -L 3

echo ""
echo "✅ Project setup complete!"
echo "   Location: $PROJECT_DIR"
echo ""
echo "Next steps:"
echo "  cd $PROJECT_DIR"
echo "  ccpm update    # Update dependencies"
echo "  ccpm list      # View installed resources"
echo "  ccpm validate  # Validate configuration"