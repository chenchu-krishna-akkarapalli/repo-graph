#!/bin/bash
# Code Review Script for my-agent
echo "=== Running Code Review ==="

# Check if rustc/cargo is available to run tests/clippy
if command -v cargo &> /dev/null
then
    echo "Running cargo clippy..."
    cargo clippy --all-targets -- -D warnings
else
    echo "Cargo not found, skipping clippy checks."
fi

# Check for trailing whitespace in source files
echo "Checking for trailing whitespace..."
git diff --check
echo "=== Review Complete ==="
