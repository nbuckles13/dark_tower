#!/bin/bash
#
# Semantic Guard: Analyze Diff
#
# Multi-check semantic analysis of code diffs using LLM.
# Processes the entire diff in one call for efficiency.
#
# Checks:
#   - credential-leak: Secrets in logs, missing skip in instrument, etc.
#   - actor-blocking: Long-running operations blocking actor message loops
#   - error-context-preservation: Error context properly included in returned errors
#   - all: Run all checks (default)
#
# Exit codes:
#   0 - SAFE (no issues found)
#   1 - UNSAFE (issues found)
#   2 - UNCLEAR (manual review needed)
#   3 - Script error
#
# Usage:
#   ./analyze-diff.sh <diff-file>                    # All checks
#   ./analyze-diff.sh <diff-file> --check credential-leak
#   ./analyze-diff.sh <diff-file> --check actor-blocking
#   ./analyze-diff.sh <diff-file> --check error-context-preservation
#   cat diff.txt | ./analyze-diff.sh -              # Read from stdin
#
# Requirements:
#   - claude CLI installed and configured
#

set -euo pipefail

# Source common library
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/../common.sh"

# Default options
CHECK="all"

# Parse arguments
DIFF_FILE=""
while [[ $# -gt 0 ]]; do
    case $1 in
        --check)
            CHECK="$2"
            shift 2
            ;;
        -)
            # Stdin indicator, treat as filename
            DIFF_FILE="-"
            shift
            ;;
        -*)
            echo "Unknown option: $1" >&2
            exit 3
            ;;
        *)
            DIFF_FILE="$1"
            shift
            ;;
    esac
done

# Validate diff file
if [[ -z "$DIFF_FILE" ]]; then
    echo "Usage: $0 <diff-file> [--check <check-name>]" >&2
    exit 3
fi

# Read diff content
if [[ "$DIFF_FILE" == "-" ]]; then
    DIFF_CONTENT=$(cat)
else
    if [[ ! -f "$DIFF_FILE" ]]; then
        echo "Error: Diff file not found: $DIFF_FILE" >&2
        exit 3
    fi
    DIFF_CONTENT=$(cat "$DIFF_FILE")
fi

# Check for empty diff
if [[ -z "$DIFF_CONTENT" ]]; then
    echo "No diff content to analyze."
    exit 2
fi

# Check for claude CLI
if ! command -v claude &> /dev/null; then
    echo "Error: claude CLI not found. Install from: https://claude.ai/code" >&2
    exit 3
fi

# -----------------------------------------------------------------------------
# Build the analysis prompt
# -----------------------------------------------------------------------------

# Base prompt with context about what we're analyzing
PROMPT_HEADER="Analyze this Rust code diff for potential issues.

The diff is in unified format. Lines starting with '+' are additions, '-' are removals.
Focus ONLY on the added/changed code ('+' lines), not removed code.

"

# Credential leak check
CREDENTIAL_LEAK_CHECK='## Credential Leak Check

Look for these patterns in the ADDED code:

1. **Secrets in logs**: Passwords, tokens, secrets, or keys logged via info!, debug!, warn!, error!, trace!, or tracing macros.

2. **Missing skip_all in #[instrument]**: Functions with sensitive parameters (password, token, secret, key, credential) that use #[instrument] without skip_all.

3. **Debug formatting secrets**: Structs containing secrets being formatted with {:?} in logs or errors.

4. **Error message leaks**: Error messages (Err, anyhow!, bail!) that include secret values.

Report findings as:
FINDING [credential-leak]: <file>:<line> - <description>

'

# Actor blocking check
ACTOR_BLOCKING_CHECK='## Actor Blocking Check

In actor-based code (files in actors/ directory or with "Actor" in struct names):

**Context**: Actors use a main `select!` loop pattern:
```rust
loop {
    tokio::select! {
        Some(msg) = self.receiver.recv() => { /* handle msg */ }
        _ = cancel.cancelled() => { break; }
    }
}
```

**SAFE patterns** (do not flag):
- Awaiting in `select!` branches (this IS the actor pattern)
- Awaiting mpsc::Sender::send() (backpressure, nearly instant)
- Awaiting oneshot for request-response within same message handling
- tokio::spawn() wrapping long operations (fire-and-forget)

**UNSAFE patterns** (flag these):
- Helper methods called by the actor that await external responses
- timeout(Duration::from_secs(N)) where N > 1 in non-select! context
- Awaiting task_handle.await (waiting for child task completion)
- Awaiting Redis/gRPC calls directly without spawn()

**Key insight**: The danger is when async methods CALLED BY the actor block the message loop.

Example UNSAFE:
```rust
async fn cleanup_meeting(&self) {
    // This blocks the actor message loop!
    let _ = timeout(Duration::from_secs(5), self.task.await);
}
```

Example SAFE:
```rust
async fn cleanup_meeting(&self) {
    tokio::spawn(async move {
        let _ = timeout(Duration::from_secs(5), task.await);
    });
}
```

Report findings as:
FINDING [actor-blocking]: <file>:<line> - <description> - Suggested fix: <fix>

'

# Error context preservation check
ERROR_CONTEXT_CHECK='## Error Context Preservation Check

Look for `.map_err(|e| ...)` patterns in the ADDED code where error context may be lost:

**UNSAFE patterns** (flag these):

1. **Error logged but not included in returned error**:
```rust
.map_err(|e| {
    tracing::error!("Operation failed: {}", e);
    MyError::Internal  // ❌ Error context logged but not in returned error
})
```

2. **Generic error message without original context**:
```rust
.map_err(|e| MyError::Crypto("Encryption failed".to_string()))  // ❌ No context from e
```

3. **Error variable captured but not used**:
```rust
.map_err(|e| MyError::Internal("Something failed".to_string()))  // ❌ e captured but unused
```

**SAFE patterns** (do not flag):

1. **Error context included in returned error**:
```rust
.map_err(|e| MyError::Internal(format!("Operation failed: {}", e)))  // ✅ Context preserved
```

2. **Error context in structured error type**:
```rust
.map_err(|e| MyError::CryptoError {
    msg: "Encryption failed".to_string(),
    source: e.to_string()
})  // ✅ Context preserved
```

3. **Error context with additional context**:
```rust
.map_err(|e| MyError::InvalidAddress {
    addr: addr.clone(),
    reason: e.to_string()
})  // ✅ Context preserved
```

**Special considerations**:

- **Validation errors**: For client input validation (auth failures, invalid tokens), logging MAY be appropriate for monitoring, but error context should still be included in the returned error for server-side debugging.

- **Internal vs External**: Server-side errors should preserve full context. Client-facing errors can use generic messages, but the underlying error should capture full context for server logs.

**Key principle**: The error variable `e` should be included in the RETURNED error type, not just logged and discarded.

Report findings as:
FINDING [error-context-preservation]: <file>:<line> - <description> - Current: <current-code> - Should be: <suggested-fix>

'

# Verdict section
VERDICT_SECTION='## Final Verdict

After analyzing ALL added code, provide your verdict:

**VERDICT RULES**:
- If ANY FINDING was reported: respond with UNSAFE
- If no findings but analysis was incomplete: respond with UNCLEAR
- If no findings and analysis was complete: respond with SAFE

Your response MUST end with exactly one of these lines:
SAFE: No issues found in the diff.
UNSAFE: Found N issue(s) that need to be addressed.
UNCLEAR: Could not fully analyze - manual review needed.

Now analyze this diff:
'

# Build final prompt based on check type
case "$CHECK" in
    "credential-leak")
        PROMPT="${PROMPT_HEADER}${CREDENTIAL_LEAK_CHECK}${VERDICT_SECTION}"
        ;;
    "actor-blocking")
        PROMPT="${PROMPT_HEADER}${ACTOR_BLOCKING_CHECK}${VERDICT_SECTION}"
        ;;
    "error-context-preservation")
        PROMPT="${PROMPT_HEADER}${ERROR_CONTEXT_CHECK}${VERDICT_SECTION}"
        ;;
    "all")
        PROMPT="${PROMPT_HEADER}${CREDENTIAL_LEAK_CHECK}${ACTOR_BLOCKING_CHECK}${ERROR_CONTEXT_CHECK}${VERDICT_SECTION}"
        ;;
    *)
        echo "Unknown check: $CHECK" >&2
        echo "Valid checks: credential-leak, actor-blocking, error-context-preservation, all" >&2
        exit 3
        ;;
esac

# -----------------------------------------------------------------------------
# Run analysis
# -----------------------------------------------------------------------------

echo "## Semantic Analysis"
echo "Check: $CHECK"
echo "Model: $GUARD_SEMANTIC_MODEL"
echo ""

# Run analysis with Claude
ANALYSIS=$(echo "${PROMPT}

\`\`\`diff
${DIFF_CONTENT}
\`\`\`" | claude --print --model "$GUARD_SEMANTIC_MODEL" 2>&1) || {
    echo "Error: Claude analysis failed" >&2
    exit 3
}

# Output the full analysis
echo "$ANALYSIS"
echo ""

# Extract verdict from the response
VERDICT=$(echo "$ANALYSIS" | grep -oiE '^(SAFE|UNSAFE|UNCLEAR):' | tail -1 | tr -d ':' | tr '[:lower:]' '[:upper:]')
VERDICT="${VERDICT:-UNCLEAR}"

# Count findings
FINDING_COUNT=$(echo "$ANALYSIS" | grep -c '^FINDING' || true)

echo "## Summary"
echo "Findings: $FINDING_COUNT"
echo "Verdict: $VERDICT"

# Exit based on verdict
case "$VERDICT" in
    "SAFE")
        exit 0
        ;;
    "UNSAFE")
        exit 1
        ;;
    "UNCLEAR")
        exit 2
        ;;
    *)
        echo "Warning: Could not parse verdict, assuming UNCLEAR" >&2
        exit 2
        ;;
esac
