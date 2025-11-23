#!/bin/bash
set -e

echo "Generating AC_MASTER_KEY (32 bytes, base64-encoded)..."
echo ""

KEY=$(openssl rand -base64 32)

echo "✅ Generated cryptographically secure master key"
echo ""
echo "Add this to your .env file:"
echo ""
echo "AC_MASTER_KEY=$KEY"
echo ""
echo "⚠️  WARNING: Never commit this key to git!"
echo "⚠️  Store in .env file (git-ignored)"
echo "⚠️  Never log or expose in API responses"
