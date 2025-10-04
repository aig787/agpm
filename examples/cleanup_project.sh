#!/bin/bash
# Cleanup script for AGPM example project
# This script removes all dependencies added by setup_project.sh and cleans up the project directory

set -e  # Exit on error

# Colors for output
GREEN='\033[0;32m'
RED='\033[0;31m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

echo -e "${RED}╔════════════════════════════════════════════╗${NC}"
echo -e "${RED}║     AGPM Project Cleanup Script            ║${NC}"
echo -e "${RED}╚════════════════════════════════════════════╝${NC}"
echo ""

# Get project name from argument or use default
PROJECT_NAME="${1:-test}"

# Setup paths
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$SCRIPT_DIR/projects/$PROJECT_NAME"

# Check if project exists
if [ ! -d "$PROJECT_DIR" ]; then
    echo -e "${YELLOW}→ Project '$PROJECT_NAME' does not exist at $PROJECT_DIR${NC}"
    echo "  Nothing to clean up."
    exit 0
fi

# Ensure agpm is available
cd "$(dirname "$SCRIPT_DIR")"
if [ ! -f "target/release/agpm" ]; then
    echo "→ Building agpm for cleanup operations"
    cargo build --release
fi

# Add to PATH for this script
export PATH="$PWD/target/release:$PATH"

echo -e "${GREEN}✓ Using agpm from: $(which agpm)${NC}"
echo ""

# Change to project directory
cd "$PROJECT_DIR"

# Check if agpm.toml exists
if [ ! -f "agpm.toml" ]; then
    echo -e "${YELLOW}→ No agpm.toml found in project${NC}"
    echo "→ Skipping dependency removal, proceeding to directory cleanup"
else
    echo "→ Starting cleanup of AGPM project '$PROJECT_NAME'"
    echo ""
    
    # List current resources before removal
    echo "→ Current installed resources:"
    agpm list || true
    echo ""
    
    echo -e "${YELLOW}═══ Removing All Dependencies ═══${NC}"
    echo ""
    
    # Remove MCP servers
    echo "→ Removing MCP servers..."
    agpm remove dep mcp-server filesystem 2>/dev/null && echo "  ✓ Removed filesystem" || true
    agpm remove dep mcp-server fetch 2>/dev/null && echo "  ✓ Removed fetch" || true
    
    # Remove hooks
    echo ""
    echo "→ Removing hooks..."
    agpm remove dep hook pre-tool-use 2>/dev/null && echo "  ✓ Removed pre-tool-use" || true
    agpm remove dep hook user-prompt-submit 2>/dev/null && echo "  ✓ Removed user-prompt-submit" || true
    
    # Remove scripts
    echo ""
    echo "→ Removing scripts..."
    agpm remove dep script build 2>/dev/null && echo "  ✓ Removed build" || true
    agpm remove dep script test 2>/dev/null && echo "  ✓ Removed test" || true
    
    # Remove commands
    echo ""
    echo "→ Removing commands..."
    agpm remove dep command git-auto-commit 2>/dev/null && echo "  ✓ Removed git-auto-commit" || true
    agpm remove dep command format-json 2>/dev/null && echo "  ✓ Removed format-json" || true
    
    # Remove snippets
    echo ""
    echo "→ Removing snippets..."
    agpm remove dep snippet error-analysis 2>/dev/null && echo "  ✓ Removed error-analysis" || true
    agpm remove dep snippet unit-tests 2>/dev/null && echo "  ✓ Removed unit-tests" || true
    agpm remove dep snippet security-review 2>/dev/null && echo "  ✓ Removed security-review" || true
    agpm remove dep snippet rest-api 2>/dev/null && echo "  ✓ Removed rest-api" || true
    agpm remove dep snippet test-coverage 2>/dev/null && echo "  ✓ Removed test-coverage" || true
    
    # Remove agents
    echo ""
    echo "→ Removing agents..."
    agpm remove dep agent rust-haiku 2>/dev/null && echo "  ✓ Removed rust-haiku" || true
    agpm remove dep agent javascript-haiku 2>/dev/null && echo "  ✓ Removed javascript-haiku" || true
    
    # Remove sources (should be empty now, so this should succeed)
    echo ""
    echo "→ Removing sources..."
    agpm remove source local-deps 2>/dev/null && echo "  ✓ Removed local-deps source" || echo "  ⚠ Could not remove local-deps (may still be in use)"
    
    # Show what's left (should be empty)
    echo ""
    echo "→ Remaining resources after cleanup:"
    agpm list || true
    
    # Show the cleaned manifest
    echo ""
    echo "→ Manifest after cleanup:"
    echo "----------------------------------------"
    cat agpm.toml
    echo "----------------------------------------"
fi

# Show final project structure
echo ""
echo "→ Final project structure:"
tree -a -L 3

# Clean up installed directories
echo ""
echo -e "${YELLOW}═══ Removing Installation Directories ═══${NC}"
echo "→ Removing .claude directory..."
rm -rf .claude

# Move out of the directory before deleting it
cd "$SCRIPT_DIR"

echo "→ Deleting project directory..."
rm -rf "$PROJECT_DIR"
echo -e "${GREEN}✓ Project directory deleted${NC}"
echo ""