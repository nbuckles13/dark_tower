# Specialist Knowledge Audit: 2026-02-11

## Trigger

Post-migration audit after Agent Teams workflow adoption (ADR-0024). Major changes to dev-loop, debate, and review protocols warranted verifying all specialist knowledge is current. Test and code-reviewer had oversized files (986 and 716 lines respectively) needing aggressive pruning. Five specialists had placeholder files needing review.

## Results

| Specialist | Before (lines) | After (lines) | Key Action |
|------------|----------------|---------------|------------|
| auth-controller | ~495 | 514 | Added 9 entries (OAuth patterns, MC integration) |
| code-reviewer | ~1,307 | 409 | Aggressive pruning (75% reduction) |
| database | ~45 | 15 | Reset to stub (see Post-Audit Review) |
| dry-reviewer | ~420 | 408 | Updated 8 entries (file refs, dates) |
| global-controller | ~800 | 833 | Added 4 entries, updated 7 |
| infrastructure | ~180 | 221 | Added 14 entries, condensed existing |
| media-handler | ~45 | 15 | Reset to stub (see Post-Audit Review) |
| meeting-controller | ~700 | 713 | 1 added, 1 updated |
| observability | ~45 | 15 | Reset to stub (see Post-Audit Review) |
| operations | ~45 | 15 | Reset to stub (see Post-Audit Review) |
| protocol | ~45 | 15 | Reset to stub (see Post-Audit Review) |
| security | ~760 | 773 | Added 3, updated 6 |
| test | ~1,898 | 584 | Aggressive pruning (69% reduction) |
| **Total** | **~6,829** | **4,530** | |

### Summary of Changes

**Two major themes:**

1. **Aggressive pruning**: test (986→221 lines in patterns.md) and code-reviewer (716→117 lines in patterns.md) had massively oversized files. Generic Rust advice removed in favor of Dark Tower-specific knowledge.

2. **Incremental updates**: 6 specialists (auth-controller, dry-reviewer, global-controller, infrastructure, meeting-controller, security) made targeted updates — adding new patterns from recent work (OAuth/TokenManager, Prometheus wiring), updating stale file references, and refining existing entries.

### Post-Audit Review: Newly-Populated Files Reset

5 specialists (database, observability, operations, protocol, media-handler) were initially populated from scratch by audit agents that read the codebase and described what they found. On review, this content was **mostly restating what the code already says** — not capturing non-obvious knowledge.

**Decision**: Reset these to empty stubs with guidance on what makes a good entry. These specialists will accumulate real knowledge organically through `/dev-loop` reflection phases as actual implementation work happens. The stub headers explicitly state: "Don't describe what the code does — capture what surprised you, what broke, what wasn't obvious."

**Rationale**: ~28k tokens of context per dev-loop for marginal value. Real specialist knowledge comes from implementation experience (mistakes, surprises, integration traps), not from reading code and writing English descriptions of it. The 8 mature specialists earned their knowledge through actual development cycles.

### Notable Changes (Mature Specialists)

- **test**: Removed generic Rust testing advice (covered by clippy), over-specific code examples, patterns duplicating ADR content. 986→221 lines in patterns.md.
- **code-reviewer**: Removed generic code quality advice (clippy catches), verbose implementation history, test organization patterns (test specialist's domain). 716→117 lines in patterns.md.
- **auth-controller**: Added 9 entries for OAuth 2.0 client credentials patterns, TokenManager integration, MC/GC authentication flows.
- **global-controller**: Added 4 entries, updated 7 stale references.
- **infrastructure**: Expanded from 12→24 entries with Kind cluster and Terraform patterns.
- **security**: Updated 6 entries, added 3 (approved-crypto.md refined).
- **dry-reviewer**: Updated 8 entries (dates, file references).
- **meeting-controller**: 1 added, 1 updated (already well-maintained from recent dev-loops).

### Quality Observations

- All file references validated (0 missing files)
- No file exceeds 350 lines (previously test/patterns.md was 986 lines)
- Largest files: security/patterns.md (347), global-controller/patterns.md (348)
- Total context cost: ~67k tokens for 8 mature specialists (down from ~95k)

## Recommendations

1. **Let stubs grow organically**: database, observability, operations, protocol, media-handler will accumulate entries through dev-loop reflection — don't bulk-populate again
2. **Quality bar for entries**: Must capture something non-obvious — if you can learn it by reading the code, don't add it
3. **Next audit**: After Phase 7 (Media Handler implementation)
4. **Size monitoring**: Flag any file approaching 400 lines for pruning
