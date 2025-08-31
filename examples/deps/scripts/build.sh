#!/bin/bash
# Example build script for CCPM
# This script demonstrates how to use scripts as dependencies

set -e

echo "🔨 Building project..."
echo "  - Checking environment..."

# Check for required tools
if command -v cargo &> /dev/null; then
    echo "  ✓ Cargo found"
else
    echo "  ✗ Cargo not found"
    exit 1
fi

# Run build
echo "  - Running cargo build..."
cargo build --release

echo "✅ Build complete!"