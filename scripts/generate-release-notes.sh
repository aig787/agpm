#!/bin/bash
# Generate release notes for a given tag using git-cliff

TAG="${1:-}"
if [ -z "$TAG" ]; then
    echo "Usage: $0 <tag>"
    exit 1
fi

# Generate changelog for just this release
git-cliff --config cliff.toml --tag "$TAG" --strip all --current