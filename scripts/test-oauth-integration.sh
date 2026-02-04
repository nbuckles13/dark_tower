#!/bin/bash
# Quick manual test plan for OAuth integration
# Tests TokenManager integration in GC and MC

set -e

echo "======================================"
echo "OAuth Integration Manual Test Plan"
echo "======================================"
echo ""

# Colors
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

pass() {
    echo -e "${GREEN}✓ PASS${NC}: $1"
}

fail() {
    echo -e "${RED}✗ FAIL${NC}: $1"
}

info() {
    echo -e "${YELLOW}ℹ INFO${NC}: $1"
}

# Phase 1: Service Health & Token Acquisition
echo "Phase 1: Service Health & Token Acquisition"
echo "============================================"
echo ""

info "1.1 Checking all services are running..."
kubectl get pods -n dark-tower
echo ""

info "1.2 Checking GC token acquisition..."
GC_TOKEN_LOGS=$(kubectl logs -n dark-tower -l app=global-controller --tail=50 2>/dev/null | grep -i "token\|415" || true)
if echo "$GC_TOKEN_LOGS" | grep -q "Token acquired successfully\|token acquired"; then
    pass "GC acquired OAuth token"
elif echo "$GC_TOKEN_LOGS" | grep -q "415"; then
    fail "GC still getting 415 errors"
    echo "$GC_TOKEN_LOGS"
else
    info "GC token status unclear, showing recent logs:"
    echo "$GC_TOKEN_LOGS"
fi
echo ""

info "1.3 Checking MC token acquisition..."
MC_TOKEN_LOGS=$(kubectl logs -n dark-tower -l app=meeting-controller --tail=50 2>/dev/null | grep -i "token\|415" || true)
if echo "$MC_TOKEN_LOGS" | grep -q "Token acquired successfully\|token acquired"; then
    pass "MC acquired OAuth token"
elif echo "$MC_TOKEN_LOGS" | grep -q "415"; then
    fail "MC still getting 415 errors"
    echo "$MC_TOKEN_LOGS"
else
    info "MC token status unclear, showing recent logs:"
    echo "$MC_TOKEN_LOGS"
fi
echo ""

# Phase 2: OAuth Token Validation
echo "Phase 2: OAuth Token Validation"
echo "================================"
echo ""

info "2.1 Checking AC issued tokens..."
AC_TOKEN_LOGS=$(kubectl logs -n dark-tower -l app=ac-service --tail=100 2>/dev/null | grep "service/token" || true)
TOKEN_SUCCESS=$(echo "$AC_TOKEN_LOGS" | grep -c "200" || echo "0")
TOKEN_415=$(echo "$AC_TOKEN_LOGS" | grep -c "415" || echo "0")

if [ "$TOKEN_SUCCESS" -gt 0 ]; then
    pass "AC issued $TOKEN_SUCCESS successful tokens"
else
    info "No successful token issuances found"
fi

if [ "$TOKEN_415" -gt 0 ]; then
    fail "AC returned $TOKEN_415 '415 Unsupported Media Type' errors"
else
    pass "No 415 errors from AC"
fi
echo ""

info "2.2 Checking MC registration with GC..."
MC_REG_LOGS=$(kubectl logs -n dark-tower -l app=global-controller --tail=50 2>/dev/null | grep -iE "controller.*register|heartbeat" || true)
if [ -n "$MC_REG_LOGS" ]; then
    pass "MC is communicating with GC"
    echo "$MC_REG_LOGS" | head -5
else
    info "No MC registration logs found yet (may be normal if just started)"
fi
echo ""

# Phase 3: Service Connectivity
echo "Phase 3: Service Connectivity Check"
echo "===================================="
echo ""

info "3.1 Checking if port-forward is needed..."
if ! curl -s http://localhost:8080/health > /dev/null 2>&1; then
    info "GC not accessible on localhost:8080"
    info "To test API, run: kubectl port-forward -n dark-tower svc/global-controller 8080:8080"
else
    pass "GC accessible on localhost:8080"

    info "3.2 Testing GC health endpoint..."
    HEALTH_RESPONSE=$(curl -s http://localhost:8080/health)
    if echo "$HEALTH_RESPONSE" | grep -q "ok\|healthy"; then
        pass "GC health check passed"
    else
        fail "GC health check unexpected response: $HEALTH_RESPONSE"
    fi
fi
echo ""

# Summary
echo "======================================"
echo "Test Summary"
echo "======================================"
echo ""
echo "To run comprehensive tests:"
echo "  cargo test -p env-tests --features smoke,flows,resilience"
echo ""
echo "To test meeting creation (requires port-forward):"
echo "  kubectl port-forward -n dark-tower svc/global-controller 8080:8080"
echo "  curl -X POST http://localhost:8080/api/v1/meetings \\"
echo "    -H 'Content-Type: application/json' \\"
echo "    -d '{\"meeting_id\": \"test-oauth-meeting-001\"}'"
echo ""
