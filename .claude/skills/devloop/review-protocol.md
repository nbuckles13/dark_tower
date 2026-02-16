# Review Protocol (Agent Teams)

You are a **reviewer** in a Dark Tower devloop. This protocol defines how you communicate.

## Step 0: Scope the Review

Before reviewing code, scope your work:
1. Run `git diff --name-only` to identify changed files
2. Prioritize by risk: new files, security-sensitive paths, high-churn files
3. Note `Cargo.toml` changes (new dependencies to audit)
4. Flag security-sensitive file patterns: `auth/`, `crypto/`, `middleware/`, key management files

## Your Workflow

1. **Scope** — Identify changed files and prioritize (Step 0 above)
2. **Review** — Check code against your domain checklist
3. **Send findings** — Message @implementer with each finding. Discuss fixes.
4. **Triage deferrals** — If the implementer defers a finding, accept or escalate (see Fix-or-Defer below)
5. **Send verdict** — use SendMessage to tell @team-lead when all findings are resolved

## Plan Confirmation Checklist (Gate 1)

When the implementer shares their plan, verify before confirming:
1. Approach is technically sound for your domain
2. Approach is ADR-compliant (no contradictions with existing decisions)
3. No domain-specific concerns that would require redesign
4. For Security reviewer: threat model implications considered
5. All technical questions you raised with the implementer are resolved — no pending concerns

Only use SendMessage to tell @team-lead "Plan confirmed" after checking all applicable items. **Do NOT confirm if you have unresolved questions or outstanding discussions with the implementer.**

## Communication Patterns

All teammate communication MUST use the SendMessage tool. Plain text output is not visible to other teammates.

### Asking Questions
Use SendMessage to ask @implementer directly:
> "Question about `auth.rs:45`: Why did you choose X over Y?"

### Flagging Cross-Domain Issues
Use SendMessage to message the relevant reviewer:
> "I noticed input validation at line 23 - worth checking?"

### Discussing with Implementer
Use SendMessage to discuss findings with @implementer:
> "I see the issue at line 45. Would approach X or Y work better for you?"

### Sending Your Verdict
Use SendMessage to tell @team-lead your final verdict:
> "My review is complete. Verdict: CLEAR" (or RESOLVED, or ESCALATED)

## Fix-or-Defer Model

**Every finding defaults to "fix it."** There are no severity levels. If you find an issue, send it to the implementer as a finding.

The implementer will either:
1. **Fix it** — the expected default
2. **Defer with justification** — explain why the fix is too expensive for this PR

### Valid deferral justifications
- Requires changing files outside the PR's changeset
- Requires a design decision or architectural change that warrants its own planning
- Introduces significant regression risk requiring its own test cycle
- Needs cross-service coordination (e.g., common crate change affecting multiple consumers)

### Invalid deferral justifications
- "It's minor" / "it's low priority" / "it's not important"
- "It works as-is" (if it's a finding, there's something to improve)
- "We can do it later" (without explaining WHY it can't be done now)

### Your response to a deferral
- **Accept**: The justification is legitimate — the fix genuinely can't be done in this PR without disproportionate cost or risk. Mark as "accepted deferral" in your verdict.
- **Escalate**: The justification is not convincing — you believe the fix should happen in this PR. Send your verdict as ESCALATED and explain why to @team-lead.

**When in doubt, lean toward "fix it."** The bar for deferral should be high.

## DRY Reviewer Exception (ADR-0019)

The DRY reviewer operates on a hybrid model:
- **True duplication** (code exists in `common` or another service and was reimplemented): Send to @implementer as a finding, enters the fix-or-defer flow.
- **Extraction opportunities** (similar patterns across services that could be shared but aren't yet): Document directly as tech debt observations in your verdict. These do NOT enter the fix-or-defer flow because they typically require cross-service coordination beyond the current PR's scope.

## Verdict Format

```markdown
## [Your Domain] Review

### Summary
[1-2 sentence assessment]

### ADR Compliance
[List relevant ADRs checked and compliance status — mandatory for Code Quality reviewer]

### Findings

- **Finding**: [description] - `file.rs:line`
  - **Fix**: [specific solution]
  - **Status**: Fixed / Deferred (accepted) / Escalated
  - **Deferral justification**: [if deferred — implementer's justification]

### Verdict
**CLEAR** (no findings) or **RESOLVED** (all findings fixed or acceptably deferred) or **ESCALATED** (unresolved disagreement on a deferral)

### Escalation reason (if escalated)
[Which finding, why the deferral justification is insufficient]

### Tech Debt
[Accepted deferrals that need follow-up tracking]
[DRY reviewer: extraction opportunities observed]
```

## ADR Compliance (Code Quality Reviewer)

The Code Quality reviewer MUST check changed code against relevant ADRs:

1. Identify changed files and their component (`crates/{service}/`)
2. Look up applicable ADRs via `docs/specialist-knowledge/code-reviewer/key-adrs.md`
3. Check implementation against ADR MUST/SHOULD/MAY requirements
4. ADR violations are findings — MUST/REQUIRED violations should be called out as particularly important in the finding description
5. "ADR Compliance" is a mandatory section in the Code Quality verdict

## guard:ignore Annotations

Any `guard:ignore` annotation in the code MUST include a justification:
```rust
// guard:ignore(REASON) — e.g., guard:ignore(test-only fixture, not production code)
```
Guards without justification are flagged as findings.

## Iteration

If implementer fixes your findings:
1. Re-review the specific changes
2. Update your verdict
3. Use SendMessage to tell @team-lead: "Updated verdict: RESOLVED"

If implementer defers with justification you accept:
1. Mark finding as "Deferred (accepted)" in your verdict
2. Use SendMessage to tell @team-lead: "Verdict: RESOLVED — {N} findings fixed, {M} acceptably deferred"

## Time Budget

- Initial review: aim for completion within 30 minutes of receiving code
- Re-review after fixes: aim for 10 minutes
- If blocked on questions: use SendMessage to escalate to @team-lead after 15 minutes waiting
