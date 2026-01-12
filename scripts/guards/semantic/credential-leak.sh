#!/bin/bash
#
# Semantic Guard: Credential Leak Detection
#
# Uses Claude for deeper analysis of potential credential leaks
# that simple grep patterns might miss.
#
# Exit codes:
#   0 - File is SAFE (no credential leak risks)
#   1 - File is UNSAFE (credential leak risks detected)
#   2 - Analysis UNCLEAR (manual review recommended)
#   3 - Script error
#
# Usage:
#   ./credential-leak.sh <file_path>
#   ./credential-leak.sh crates/ac-service/src/routes/auth_handler.rs
#
# Requirements:
#   - claude CLI installed and configured
#   - ANTHROPIC_API_KEY set (or claude CLI configured)
#

set -euo pipefail

# Source common library
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/../common.sh"

# Check for required arguments
if [[ $# -lt 1 ]]; then
    echo "Usage: $0 <file_path>"
    exit 3
fi

FILE_PATH="$1"

# Verify file exists
if [[ ! -f "$FILE_PATH" ]]; then
    echo "Error: File not found: $FILE_PATH"
    exit 3
fi

# Skip test files
if is_test_file "$FILE_PATH"; then
    echo -e "${GREEN}SKIPPED${NC}: Test file - $FILE_PATH"
    exit 0
fi

# Check for claude CLI
if ! command -v claude &> /dev/null; then
    echo "Error: claude CLI not found. Install from: https://claude.ai/code"
    exit 3
fi

start_timer

print_header "Semantic Guard: Credential Leak Detection
File: $FILE_PATH"

# Read the file content
FILE_CONTENT=$(cat "$FILE_PATH")

# Analysis prompt - focused on credential leak detection
PROMPT=$(cat <<'EOF'
Analyze this Rust code for potential credential leaks in logging, tracing, or error messages.

Focus on these specific risks:

1. **Logging secrets**: Are passwords, tokens, secrets, or keys being logged via info!, debug!, warn!, error!, trace!, or tracing macros?

2. **Missing skip in #[instrument]**: Do functions with secret parameters (password, token, secret, key, credential) have proper skip(...) in their #[instrument] attribute?

3. **Debug formatting secrets**: Are structs containing secrets (like request bodies with client_secret) being formatted with {:?}?

4. **Error message leaks**: Do error messages (Err, anyhow!, bail!) include secret values?

5. **Indirect leaks**: Could secrets leak through:
   - Struct fields logged via derived Debug
   - Error contexts that include request data
   - Panic messages (though panics should be avoided anyway)

For each finding, specify:
- Line number or code snippet
- Risk level (HIGH/MEDIUM/LOW)
- Specific remediation

Analyze the code thoroughly, then provide your final verdict at the end.

Your response MUST end with a final verdict section in exactly this format:

## VERDICT
SAFE|UNSAFE|UNCLEAR: <one-line explanation>

Choose:
- SAFE: No credential leak risks found
- UNSAFE: Credential leak risks detected (followed by details)
- UNCLEAR: Cannot determine, manual review needed (followed by reason)

Be conservative - if unsure, mark as UNCLEAR rather than SAFE.

Code to analyze:
EOF
)

echo -e "${BLUE}Analyzing with Claude ($GUARD_SEMANTIC_MODEL)...${NC}"
echo ""

# Run analysis with Claude
# Using --print to get just the response
ANALYSIS=$(echo "${PROMPT}

\`\`\`rust
${FILE_CONTENT}
\`\`\`" | claude --print --model "$GUARD_SEMANTIC_MODEL" 2>&1) || {
    echo "Error: Claude analysis failed"
    exit 3
}

# Extract verdict from the end of the response (after ## VERDICT section)
# The LLM analyzes first, then concludes - so verdict is at the end
VERDICT=$(echo "$ANALYSIS" | tail -20 | grep -oiE '^(SAFE|UNSAFE|UNCLEAR):' | tail -1 | tr -d ':' | tr '[:lower:]' '[:upper:]')
VERDICT="${VERDICT:-UNCLEAR}"

echo "Analysis Result:"
echo "================"
echo ""
echo "$ANALYSIS"
echo ""

print_header "Verdict"
print_elapsed_time

case "$VERDICT" in
    "SAFE")
        echo -e "${GREEN}SAFE${NC}"
        echo "No credential leak risks detected."
        exit 0
        ;;
    "UNSAFE")
        echo -e "${RED}UNSAFE${NC}"
        echo "Credential leak risks detected. Review findings above."
        exit 1
        ;;
    "UNCLEAR")
        echo -e "${YELLOW}UNCLEAR${NC}"
        echo "Manual review recommended."
        exit 2
        ;;
    *)
        echo -e "${YELLOW}UNCLEAR${NC}"
        echo "Could not parse verdict from analysis."
        exit 2
        ;;
esac
