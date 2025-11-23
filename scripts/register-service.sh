#!/bin/bash
set -e

if [ $# -ne 2 ]; then
    echo "Usage: ./scripts/register-service.sh <service_type> <region>"
    echo ""
    echo "Examples:"
    echo "  ./scripts/register-service.sh global-controller us-west-1"
    echo "  ./scripts/register-service.sh meeting-controller us-west-1"
    echo "  ./scripts/register-service.sh media-handler us-west-1"
    echo ""
    exit 1
fi

SERVICE_TYPE=$1
REGION=$2
AC_URL=${AC_URL:-http://localhost:8082}

echo "Registering $SERVICE_TYPE in region $REGION..."
echo ""

# Check if jq is installed
if ! command -v jq &> /dev/null; then
    echo "⚠️  jq is not installed. Install with:"
    echo "   - macOS: brew install jq"
    echo "   - Ubuntu: sudo apt install jq"
    echo ""
    echo "Showing raw response instead..."
    echo ""

    curl -s -X POST "$AC_URL/api/v1/admin/services/register" \
      -H "Content-Type: application/json" \
      -d "{\"service_type\": \"$SERVICE_TYPE\", \"region\": \"$REGION\"}"
    echo ""
    exit 0
fi

RESPONSE=$(curl -s -X POST "$AC_URL/api/v1/admin/services/register" \
  -H "Content-Type: application/json" \
  -d "{\"service_type\": \"$SERVICE_TYPE\", \"region\": \"$REGION\"}")

if echo "$RESPONSE" | jq -e '.error' > /dev/null 2>&1; then
    echo "❌ Registration failed:"
    echo "$RESPONSE" | jq -r '.error.message'
    exit 1
fi

echo "✅ Service registered successfully!"
echo ""
echo "Add these environment variables to your $SERVICE_TYPE .env or shell:"
echo ""
echo "$RESPONSE" | jq -r '"AC_CLIENT_ID=" + .client_id'
echo "$RESPONSE" | jq -r '"AC_CLIENT_SECRET=" + .client_secret'
echo "AC_URL=$AC_URL"
echo ""
echo "⚠️  Store client_secret securely. It will not be shown again."
