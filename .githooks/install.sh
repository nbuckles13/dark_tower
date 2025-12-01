#!/bin/bash
#
# Install git hooks for Dark Tower project
# Run this script after cloning the repository to set up git hooks
#

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
GIT_DIR="$(git rev-parse --git-dir)"

echo "Installing git hooks..."

# Configure git to use .githooks directory
git config core.hooksPath "$SCRIPT_DIR"

echo "✅ Git hooks installed successfully!"
echo ""
echo "The following hooks are now active:"
ls -1 "$SCRIPT_DIR" | grep -v "install.sh" | grep -v "README.md"
