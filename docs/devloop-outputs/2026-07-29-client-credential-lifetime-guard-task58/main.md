# Devloop Output: Client Credential-Lifetime Guard + Token-Based Join (Task #58)

**Date**: 2026-07-29
**Task**: Extend guard coverage to client/non-Rust code + fix the credential-retention defect it should have caught
**Specialist**: security (paired with client + infrastructure)
**Mode**: Agent Teams (v2)
**Branch**: `feature/browser-client-join-task-58`
**Duration**: ~24h wall-clock (2026-07-29 → 2026-07-30), spanning an API-capacity outage and an auth expiry

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `72ca36c816277549f92499722dcb5230085b6d82` |
| Branch | `feature/browser-client-join-task-58` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `implementation` — **GATE 1 PASSED 2026-07-30, "Plan approved" issued** |
| Implementer | `implementer` (security identity, explicitly loaded) |
| Implementing Specialist | `security` |
| Iteration | `1` |
| Security | **`CONFIRMED`** — F-SEC-1/2/3 closed; both A-P6 conditions met |
| Test | **`CONFIRMED`** — 10 findings, all accepted, zero deferred; (c) ruled |
| Observability | **`CONFIRMED`** — 6 findings, all accepted |
| Code Quality | **`CONFIRMED`** — F1–F5 closed; wrapper upgrade accepted |
| DRY | **`CONFIRMED`** — P1/D1/D2 closed; ADR-0024 co-sign granted separately |
| Operations | **`CONFIRMED`** — (c) joint ruling; 2 items carried to Gate 2 |
| Semantic Guard | **`CONFIRMED`** — SG-1..SG-8 satisfied; SG-9/SG-10 non-blocking |
| Paired: Client | **`CONFIRMED`** — F1–F7 accepted; conditional on 2 added rows |
| Paired: Infrastructure | **`CONFIRMED`** (seat = paired-infrastructure-2) — on ack of 2 items |

## ✅ GATE 1 PASSED — 9 of 9 confirmed, "Plan approved" issued 2026-07-30

| Reviewer | Verdict |
|---|---|
| @security | Plan confirmed |
| @test | Plan confirmed |
| @observability | Plan confirmed |
| @code-reviewer | Plan confirmed |
| @dry-reviewer | Plan confirmed (+ ADR-0024 co-sign) |
| @operations | Plan confirmed |
| @semantic-guard | Plan confirmed |
| @paired-client | Plan confirmed — **unconditional**; blocking condition closed |
| @paired-infrastructure-2 | Plan confirmed |

**Classification-sanity guard**: `validate-cross-boundary-classification.sh` →
`STATUS=OK REASON=cross-boundary-classification-clean-1-files`, exit 0. (Required a
`cargo build --release -p dt-guard` first — the binary was absent and the wrapper correctly
emitted `STATUS=FAIL REASON=dt-guard-binary-missing` and exited 1. Verified the wrapper's
exit path rather than assuming it; no masking.)

### TWO BINDING PRE-IMPLEMENTATION AMENDMENTS

Approval is granted **with** these folded in first. Both are precisely specified, neither
requires a decision, and both originating reviewers stated they do not need to re-review.

**B-1 — @paired-client's blocking condition (F-SEC-3 cascade).** Make the derived view
**bidirectional** (unauthenticated + `create`/`join` → `signin`); assert the Sign-in **view**,
not the nav button; and extend the session-clear to `CreateMeeting` per the Lead ruling below.

**B-2 — F11: the kernel seam cannot express Rule 2.** @paired-infrastructure-2 corrected their
own A-P2 specification after @test raised it. `scan_source(path, content)` is per-file; Rule 2's
pass 1 is **repo-wide**. They had mapped the `cite_extract_e2e.rs` precedent (where the kernel
genuinely is per-file) onto a rule of a different shape without checking the shapes matched —
wrong from the original plan, not just after A-P18.

**The failure mode is worse than the double-exemption bug, by the same mechanism one turn
deeper**: a per-file kernel hands Rule 2 an empty declaration index, so every Rule 2 assertion
returns silent — and silent is what a negative fixture asserts. **The matrix goes fully green
with Rule 2 structurally disabled.** The double-exemption bug at least scanned zero files; this
produces a *passing matrix that reads as coverage*.

Fix: two-phase seam **plus a genuinely cross-file fixture pair** — which also subsumes the
stale-positive-fixture finding (two reviewers reached that fixture from different directions;
the cross-file pair is the cleaner fix). Wall-time measurement re-taken against the two-pass
shape.

### Lead ruling — extend the session-clear to `CreateMeeting`

@implementer scoped the clear to the join view. Overruled, backing @paired-client.
`CreateMeeting.svelte:37` passes the same `auth.userToken`, so the identical 401 lands there and
the user is equally stranded now B3 removed the nav. The truth is credential-shaped, not
view-shaped: **any 401 from a `userToken`-bearing call proves the retained credential is dead**,
and a dead credential should be dropped wherever it is discovered. Scoping to one view means the
app knowingly holds a credential it has been told is invalid — a credential-lifetime defect,
this task's own subject. ~3 LoC in a file already in the changeset; fails the
suspicious-deferral check on all three counts, and "the task didn't ask for the other instance"
is the framing-lock bug the protocol names explicitly.

## GATE 3 VERDICTS (running tally)

| Reviewer | Verdict | Findings / fixed / deferred |
|---|---|---|
| @test | **RESOLVED-FIXED** | 3 / 3 / 0 |
| @dry-reviewer | **RESOLVED-DEFERRED** | 3 / 3 / 0 — label driven solely by §Accepted Deferrals holding 3 DRY extraction opportunities |
| @paired-client | **RESOLVED-FIXED** *(unconditional — C3 landed and verified at source)* | 3 / 3 / 0 |
| @security | **RESOLVED-FIXED** | 7 / 7 / 0 |
| @observability | **RESOLVED-FIXED** | 4 / 4 / 0 |
| @operations | **RESOLVED-FIXED** | 2 / 2 / 0 (+1 withdrawn as own error) |
| @semantic-guard | **RESOLVED-FIXED** (native: UNSAFE → SAFE on re-verification) | 2 / 2 / 0 |
| @code-reviewer | **RESOLVED-FIXED** | 3 / 3 / 0 |
| @paired-infrastructure-2 | **RESOLVED-FIXED** | 4 / 4 / 0 |

## ✅ GATE 3 COMPLETE — 9 of 9 verdicts

**32 findings raised, 32 fixed. Zero deferred, zero spun out, zero escalated.**

Eight **RESOLVED-FIXED**; @dry-reviewer **RESOLVED-DEFERRED** with *zero deferred findings* —
that label is driven solely by §Accepted Deferrals holding three DRY extraction opportunities,
which the protocol counts toward it. @dry-reviewer argued explicitly for reading it that way
rather than rounding to a clean label, so three real cost-shifts stay visible.

### F-INFRA-4 — the finding to keep (@paired-infrastructure-2)

A defect **inside the fix for** F-INFRA-3. The test written to protect the retuned cap asserted
two hardcoded literals against each other (`2000 >= 468 * 3`), so it would pass forever
regardless of the tree **while claiming to detect exactly that drift**.

Third occurrence of one shape — *a value that must correspond to something else, with nothing
asserting the correspondence* — after `STEM_EXPANSIONS` keys and `HYGIENE_SOURCE_SCAN_SUBSET`.
What distinguishes it: **staleness was guaranteed rather than possible**, because the tree
changes without anyone touching the file holding the constant.

Fixed by deriving the measurement at test time through the scanner's own primitives (so a second
measuring implementation cannot disagree with what it measures), with a vacuity guard failing if
the file set is ever empty, **and the negative control actually run** — cap → 900 makes it fail,
naming file and span. The step whose absence produced the tautological env-test, applied
unprompted.

**Lead correction**: I had praised this test to @implementer as "@test's remedy applied without
being told to." It was the right *shape* and the wrong *implementation* — I endorsed it without
checking whether its inputs were any less static than the comment's claim. Same displacement,
from the Lead seat, on the fix I was holding up as exemplary. **Writing a test instead of a
comment is necessary but not sufficient if the test's inputs are as static as the comment's.**

### The loop's pattern, in @paired-infrastructure-2's closing form

> A check that runs, passes, and measures something **adjacent** to what it needs to measure. It
> appeared in the guard, in the fixtures, in the vocabulary, in the seam I specified, and finally
> in a test written to prevent it.

Every instance was caught by **running** the thing rather than reading about it — F-INFRA-3
surfaced only as a WARN line on an otherwise-green run. And it applies to reviewers'
specifications as readily as to implementers' code: two of the loop's most consequential findings
were corrections to specifications the paired owner wrote themselves.

**@code-reviewer's Ownership Lens result: 40 files, 40 rows, ZERO missing** — run as a backward
set-difference (file set derived from the diff, then differenced against the table), not a
forward row read. A forward read structurally cannot find a row that is not there, which is how
two rows were missed at Gate 1. Three late additions (`errorText.ts`, `secret_patterns.rs`,
`auth_client.rs`) all present.

**GSA upgrade condition discharged against the diff, not the commit message**:
`joinMeeting(meetingCode, { userToken })` byte-unchanged, `joinResp.token` still carried to MC.
Nothing changes which token type or scope MC accepts. No upgrade; nothing escalates.

**Trailers explicitly recorded as UNVERIFIED, not passed** — nothing is committed yet, so
@code-reviewer declined to mark that check green. Correct: a check that cannot run yet is not a
check that passed.

### THE SEAM HEURISTIC HELD UNDER TEST (@security)

The Lead's standing warning was that `--specialist=security` implementing while `security`
reviews risks correlated blind spots. @security's closing count settles it: **five of their
seven findings lived between deliverables rather than inside them** — F-SEC-1 in the B1/A2 seam,
F-SEC-3 in B1/B3, F-SEC-5 between the closure and the matchers, F-SEC-7 between the widening and
the WARN channel.

*When reviewer and implementer share an identity, both reason competently about their own halves
and neither owns the join.* That is the transferable form, and it argues for shared-identity
review being **targeted at seams** rather than avoided.

### F-SEC-5 — THE INVISIBLE BYPASS (the loop's most dangerous single defect)

Phase 2 read the *first* identifier where it needed the set, so `$state<undefined | AuthResult>`,
`readonly AuthResult[]` and `Array<AuthResult>` were all **silent** while
`$state<AuthResult | undefined>` fired.

**Reordering a union is semantically a no-op.** In a guard deliberately shipped with *no bypass
hatch*, the natural response to a finding is to edit the line until it goes green — and a
reorder does exactly that, leaving **no marker, no annotation, and nothing for a reviewer to
see.** A suppression mechanism nobody designed, in the one guard designed to have none.

Found by driving the rebuilt binary against throwaway repos, not by reading the regex.

### PAIRED-CLIENT'S SELF-CORRECTION ON THEIR OWN FIX INSTRUCTION

Their C3 instruction said *"add the same `never` assert `#authenticate` has."* Underspecified:
`#authenticate` **throws** on its default branch, which is safe — but applied literally to a
**value-producing** position it returns `exhaustive`, and `never` is erased at runtime, so a JS
caller passing `{mode:'sso'}` would put the literal string `sso` into a metric label.
Unbounded caller-controlled cardinality, in the family this review spent itself narrowing.

Generalized: **any exhaustiveness assert in a value-producing position rather than a throwing
one has this gap.** And it is the loop's shape once more — an instruction naming a mechanism
that checks a different thing from the property actually wanted. Same catching question:
*what would make this pass that shouldn't?*

### THE EIGHTH INSTANCE — the defect was in the safety net

@semantic-guard's framing, and it is the loop's thesis closing on itself: the `switch` was added
**at Gate 1, at reviewers' request**, to prevent credentials being silently misrouted — and its
exhaustiveness guard is where a credential escaped (`JSON.stringify(exhaustive)` serialising the
whole credentials object, password included, onto a thrown `Error.message`, propagating out of
`join()` to any SDK embedder).

**Four reviewers found it independently** — @code-reviewer, @semantic-guard, @observability
(F-O2), @security. Zero had found it before the new lens existed. @semantic-guard: *"the lens
caught a defect in the code that shipped the lens."*

@code-reviewer's addition names a distinct phenomenon from the unexecuted-claim pattern: this
was **a direct consequence of a change they and @paired-client asked for** — the `never` default
did not exist before Gate 1. Second instance of *a correct fix creating a new defect one layer
down*, after F1's stale fixture. **Fixes need the same scrutiny as originals.**

### THE CHECK TEXT WAS WRONG IN A WAY ONLY EXECUTION COULD REVEAL

@semantic-guard's second finding is sharper than their first. `checks.md:65`'s **definitional**
sentence was spatial ("a token reachable in lexical scope") while its **illustrative** sentence
was temporal ("holding a valid token"). They diverge on exactly one shape — a login form
declaring `let token = $state('')` — so **the definitional sentence as written flags every auth
form**, which is the failure they had themselves warned makes a lens worse than none.

The wording was **their own**, proposed at SG-2. The FP-trap fixture that caught it was **also
their own**, insisted on at SG-6. Their note: *"I'd have gone on believing 'reachable in lexical
scope' was operable indefinitely."*

They also refused to trust a green on a negative assertion: `lens_fixtures_are_silent_for_the_mechanical_guard`
asserts silence, so they **read its body** rather than the result — confirming it pins
`lens.len() == 3` and builds the declaration index over *both* fixture sets, so silence cannot be
an artefact of a missing declaration. Non-vacuous.

### COMMIT-TIME OBLIGATIONS (Lead, Step 8)

**`Approved-Cross-Boundary` trailers — @paired-client, established against the hunks (verbatim):**

```
Approved-Cross-Boundary: client token-variant union + exhaustive-switch join auth per client SDK surface ownership (ADR-0028 SDK-first)
Approved-Cross-Boundary: client AuthSession replaces AuthResult; credential-free retained session per R-23 minimisation
Approved-Cross-Boundary: client bidirectional shell view + auth-nav gating as session-state control, owner-specified
Approved-Cross-Boundary: client session-invalidation policy 401-only per GC contract meetings.rs:325-326
```

**@operations** (verbatim):

```
Approved-Cross-Boundary: operations Layer-3 failure-mode mapping per runbook §6.3/§8 convention
```

**@semantic-guard** (verbatim):

```
Approved-Cross-Boundary: semantic-guard dereference to checks.md removes 4-site enumeration drift; fixture carve-out closes the tests/ exemption that would have made verification (b) vacuous
```

@semantic-guard additionally co-signs `scripts/guards/semantic/checks.md` per SG-8 — *the lens is
operable by me as written*, a claim they noted they could only make **after running it**.

**Other Step 8 obligations:**
- Re-run **full `layer-all.sh`** against the final tree — the Gate 2 validation is stale by the
  time fixes land, and committing on it would be the same defect this loop catalogued.
- **Re-run `git ls-files -s` AFTER the commit** (@code-reviewer, instance 6) — before the
  commit it returns empty on an untracked path and establishes nothing.
- **Two separable commits**: (1) B1/B2/B3 client fix, (2) A1/A2 guard + fixtures + docs. Revert
  order is the inverse, recorded in §Rollback Procedure.

**Weak-classification note carried from @paired-client's Ownership Lens**: `packages/sdk-core/src/index.ts`
is classified **Mechanical**, but they flag it has **no guard coverage for its change-pattern** —
`tsc` will not catch a missing re-export that in-repo code reaches by direct path. ADR-0024 §6.2
makes Mechanical conditional on guards catching every partial version, so this row's premise is
weaker than the label implies. They disclosed it rather than leaving it implicit; @code-reviewer
may upgrade, which would auto-route to ESCALATE per §6.2.

### INSTANCES 10 & 11 — A GREEN LAYER IS NOT EVIDENCE THE LAYER RAN

Both found *after* Gate 3 closed, in the validation pipeline itself.

**Instance 10 (the Lead's).** Gate 2's Layer 5 reported `RESULT=OK DURATION=1`. The final-tree
run reported `RESULT=FAIL DURATION=17` on a Prettier violation. The delta was not new breakage —
**Gate 2's Layer 5 was an nx cache hit.** One second across five TypeScript projects is not a
lint run; it is `[existing outputs match the cache, left as is]`. The check ran, passed, and
established *"the cache is valid for these inputs"* rather than *"this code lints."*
Object-axis displacement, in the pipeline the Lead was using to gate everyone else's work — and
the Lead read `OK` without asking what a one-second lint across five projects could have
executed.

**Instance 11 (the implementer's), one message later.** Verifying the Prettier fix, they ran
`nx run-many -t lint`, got `Successfully ran target lint for 5 projects`, and were about to
report it — **after** the mechanism had just been explained to them in writing with the tell
named. Applying the tell to their own output showed every project was a cache hit; re-running
with `--skip-nx-cache` is what actually established anything. *Knowing the pattern is not
protection from it*, demonstrated in the shortest possible interval.

**The durable form — stronger than "run it, don't reason about it," because in both cases the
thing WAS run:**

> **A green layer is not evidence the layer ran.** Caching, skip-if-untouched and
> `SKIPPED-NO-DIFF` all produce `OK`. **`DURATION` is the cheapest available tell** — and nobody
> in this loop looked at it until it disagreed with itself (`DURATION=1` vs `DURATION=17` for
> the same work).

**Stated plainly rather than left to inference**: `STATUS=SKIPPED-NO-DIFF REASON=rust-no-diff`
means **Rust lint did not run** on the final tree — the changeset is unstaged and the layer diffs
against a base ref. Correct wrapper behaviour per ADR-0033, not a defect. But clippy coverage for
the new module therefore comes from @implementer's direct `cargo clippy -p dt-guard --all-targets`
runs and @paired-infrastructure-2's independent verification, **not** from Layer 5. A green
Layer 5 would otherwise imply a check it did not perform.

This also re-grounds the Lead's re-validation policy on better reasoning than it was adopted
with: not merely *"the tree moves during Gate 3"*, but *"a passing gate may not have inspected
anything."*

### WHY THE PATTERN RECURRED — the strongest formulation (@implementer, closing)

> **The pattern is a property of how a claim and its verification drift apart under iteration —
> every fix is a new claim, and it arrives with less scrutiny than the original precisely
> because it is responding to scrutiny.**

That explains what "be more careful" cannot: why *fixes* kept producing fresh defects. A fix
carries the authority of the finding that prompted it, so it is read as a resolution rather than
as a new assertion needing its own verification. Four instances in this loop began life as
correct fixes:

1. F1's retention gate → invalidated the item-(a) positive fixture
2. The `never` default (added at reviewers' request) → leaked a credential into `Error.message`
3. The C3 `never` assert → erased at runtime, would have put a caller-controlled string in a label
4. The cap comment written to replace an untrustworthy claim → **asserted a false justification**
5. The cap *test* written to replace that comment → **could not detect what it claimed** (F-INFRA-4)

Items 4 and 5 are three layers of remediation deep, each fixing the previous layer's version of
the same defect. **"Add a test instead of a comment" is a real improvement and is not a fix for
this class** — a test is another check, and it can establish a proposition adjacent to the one
needed exactly as a comment can.

**Two consequences worth carrying:**
- **Review remediations at the same depth as originals.** They are the least-scrutinised code in
  any loop and, in this one, the most defect-dense.
- **Re-validate the final tree rather than trusting the gate run.** The Lead adopted this after
  the Gate-2/docs-edit near-miss; the five instances above are the argument for it.

### THE ACTIONABLE PREFERENCE (@implementer, from @code-reviewer's contrast)

> **Prefer checks whose failure mode is absurd output over checks whose failure mode is a clean
> pass.**

@code-reviewer's broken process-substitution reported *"every file missing"* and died in one
glance. The tautological assertion looked right and needed @test to catch. Same underlying
error class; one is self-announcing and one is silent. **When both shapes are available, choose
the loud one.**

### FINDING 3 — the load-bearing comment that was false

The `MAX_DECL_BLOCK_LINES` comment claimed the cap *"is what makes the walk terminate."* It does
not: `cursor + 1 < lines.len()` is in the loop condition, so termination holds with or without
it. **Right conclusion, wrong justification** — the dangerous combination, because the comment
existed *specifically* to stop the next contributor re-deriving the rejected option. A reader
who checks the stated claim finds it false, and the natural next step from "the cap isn't needed
for termination" is "the cap isn't needed."

What the cap actually buys is **bounded work per declaration**: without it an unbalanced-brace
file makes each walk O(file length), so N declarations go quadratic. Corrected, with the error
left noted rather than silently overwritten.

**A load-bearing comment has to be right or it is worse than absent.**

### THE MECHANISM BEHIND THE PATTERN (@test, sharpening @implementer's T1 write-up)

The implementer wrote *"knowing the pattern is not protection from it."* @test identified **why**,
and the mechanism is better than the caution:

> The nineteen lines of comment explaining the failure mode are what made the assertion **feel**
> verified — the comment discharged the sense of obligation the assertion was supposed to
> discharge.

That is why prose cannot fix this class, and why "be careful" is not a remedy: the more
carefully the failure mode is documented next to the check, the more verified the check feels.
The only step prose cannot satisfy is the procedural one:

**For any new negative assertion, name the regression it targets and confirm it fails under that
regression.**

Plus @test's positive-control principle, now load-bearing in three places in this changeset:
*any time you assert a count is low, assert somewhere that it reads high, or you cannot
distinguish a clean result from a dead detector.*

### RE-MEASUREMENT RAN, PASSED, AND STILL MISSED IT (instance 9)

The standing re-measurement clause @operations required was exercised — the implementer **did**
re-measure the FP surface after the `class` widening, and it came back clean. The guard was
warning on **every run** at the same time.

The regression had landed in the **WARN channel**, which the clause did not name. Textbook
domain-axis displacement from @code-reviewer's taxonomy: the check ran, passed, and established
a proposition *adjacent* to the one needed. The clause now names both — **a clean run must also
be a WARN-free run.**

Found independently by @security (F-SEC-7), @observability (F-O4) and @code-reviewer — three
lenses, one constant (`MAX_DECL_BLOCK_LINES`, raised 400 → 800 after `SignalingClient` measured
468 lines).

### A THIRD "CORRECT FIX, NEW DEFECT ONE LAYER DOWN"

Fixing @paired-client's C3 broke @observability's test on the first attempt. The suggested
`const _exhaustive: never = mode; return exhaustive;` is sound at compile time — but **`never`
is erased at runtime**, so a JS caller passing `{mode:'sso'}` would have made the metric label
the literal string `sso` instead of `internal`.

**Compile-time exhaustiveness and honest runtime behaviour are two different properties; the
`never` assert buys only the first.** Caught immediately by @observability's pinned stage-label
test — a decent argument for pinning label values at all.

Three instances now of a *correct fix* creating a new defect one layer down: F1's stale fixture,
the `never`-default credential leak, and this. **Fixes need the same scrutiny as originals** —
and in this loop they earned it three times out of three.

### THE ASSERTION-DEMANDED FALLACY (@test) — and the Lead's part in it

@test's closing lesson, recorded because it names a failure the **Lead** committed:

> Every reviewer — me included — demanded the token-only property be "pinned by an explicit
> assertion," then checked that an assertion had been *added*. **Nobody checked it could fail.**

That includes the Lead. I ruled the env-test's incidental token reuse "must be pinned by an
explicit assertion — an incidental property is not a contract," and told @operations it must be
"executable and observational, not structural" and *"specified precisely enough that
@implementer cannot satisfy it with a comment."* I specified against **existence** and against
**one wrong shape**, and never against **falsifiability**. The assertion that came back compared
an immutable binding to a clone of itself.

**Transferable form**: *an assertion that was demanded is not thereby an assertion that works.
For any new negative assertion, name the regression it targets and confirm the assertion fails
under it.*

**The cheap general remedy** (@test): the counter's baseline check — assert your instrument
reads non-zero somewhere it *should*, or you cannot distinguish "clean" from "blind." A dead
counter then fails at the baseline rather than passing everywhere.

This is instance 7 on the **domain** axis of @code-reviewer's taxonomy: the check ran, passed,
and established "an assertion exists" rather than "the property holds."

### THE LOOP'S HEADLINE LESSON — one defect, five displacement axes

**Supersedes the Lead's earlier two-mode split** (kept below for provenance). @code-reviewer
showed the two modes are real but do **not** partition — instances exist that fit neither. The
correct container is one level up:

> **Every instance is a gap between the proposition asserted and the proposition actually
> established.** What differs is *what displaces* one from the other.

| Axis | Instance | What was established instead |
|---|---|---|
| **Nothing** | A-P1 (`\b(token)\b` vs `userToken`) | — reasoned, never run |
| **Object** | F11 (kernel seam) | a property of the *precedent*, not of the target |
| **Domain** | `accessToken` redundancy claim | true over *one* matcher family, claimed over all |
| **Time** | stale positive fixture | true when written; the predicate moved beneath it |
| **Agent** | the reviewers' own review split | would be established, by someone else, about a different question |

**The general catching question**, which subsumes "have I run it?" and "does the precedent's
shape match?": **name the proposition your check establishes, then compare it to the proposition
you need.**

**The corollary is the important half.** On the domain, time and agent axes the check *runs and
passes*. **Three of five instances had a green check.** That is why "add fixtures" / "run it" is
the wrong remedy — the loop's own experience refutes the obvious lesson.

#### Instance 6 — in a reviewer's own pre-committed check

@code-reviewer ran `git ls-files -s` on the new wrapper, the check the Lead had weighted
highest. **It returned nothing** — the wrapper is untracked, so it is not in the index, and
`git ls-files -s` on an untracked path exits 0 with empty output. Read as "no wrong modes
found," the highest-weighted check would have silently passed while establishing nothing.
Domain-axis displacement, inside the check pre-committed *precisely so it could not be
discretionary*. They caught it because the empty output was surprising, then pivoted:
`core.fileMode=true` + filesystem `-rwxr-xr-x` ⇒ git records `100755` at `git add`, corroborated
by the live smoke.

**→ COMMIT-TIME ACTION FOR THE LEAD**: re-run `git ls-files -s` **after** the commit, when it
will actually mean something.

#### Instance 7 — the implementer wrote the loop's defect into the fix for it

The C2 env-test assertion — added to satisfy the Lead's *"an incidental property is not a
contract"* requirement — was **tautological**: it compared `user_token` against a clone of
itself, and since the binding is not `mut` and never reassigned, the borrow checker guarantees
it holds. @test's sharper point is *which* regression it misses: a re-auth introduces a **new
binding**, leaves the original untouched, and the assertion stays green while the property is
gone. Blind to exactly the shape it was written to catch.

**It was worse than what it replaced.** The pre-#58 property was incidental but honestly
unclaimed; the fix added a 20-line comment stating it was "now pinned explicitly," so the next
maintainer trusts it and ships the regression. @test's formulation, worth keeping verbatim:
*an unasserted property with an accurate comment is safer than a vacuous assertion with a
confident one.*

The implementer's own note on why it belongs in the record rather than buried in a fix: this was
a claim about **their own test**, written immediately after documenting the failure mode, in the
file whose purpose was to close it, with §Lessons Learned already written above it.
**Knowing the pattern is not protection from it.**

---

### (Superseded) TWO DISTINCT FAILURE MODES — kept for provenance

@implementer identified that @paired-infrastructure-2's F11 error is **not** an instance of the
loop's headline pattern, and they are right. Recording both, because the catching question
differs.

**Mode 1 — the unexecuted claim.** A conclusion reasoned to and never run. Four instances across
four roles: @implementer (the `\b(token)\b` matcher; the `cred` stem; the FP matrix),
@team-lead (the ADR-0034 sprawl trigger "fired and stepped over silently" — one grep of
`docs/TODO.md` would have shown a tracked, deliberate decision with a re-trigger at 40),
@paired-infrastructure-2 (raising that premise), @dry-reviewer (citing a `checks.md` relocation
that never happened; and the "dead by convention" half of D2).

*The asymmetry is the point*: reasoning produces a confident claim at near-zero cost, execution
a correct one at slightly above zero. **Every instance in this loop that reasoning got wrong,
running got right in under a minute.** Catching question: **"have I run this, or only reasoned
it?"**

**Mode 2 — the correctly-executed analogy to the wrong precedent.** F11. The kernel seam was
modelled on `cite_extract_e2e.rs`, which *genuinely is per-file* — anyone checking that
precedent would find it sound. What failed is that Rule 2's shape (repo-wide pass 1) never
matched the precedent's shape. **"Run it" does not catch this**: the precedent runs correctly,
and the seam compiles. Catching question: **"does the precedent's shape match the shape of the
thing I am applying it to?"**

Mode 2 is the more dangerous of the two, because every local check passes.

### PATTERN FOR THE RETRO — four false-greens, all in assertions, one shape

@paired-client proposed recording this as a single pattern rather than four separate findings,
and they are right. Every instance is **an assertion that checks a different thing from the one
it names** — invisible on review unless someone re-derives what the assertion would pass *on*.

| # | Assertion | What it would have passed on |
|---|---|---|
| 1 | item-(a) fixture pair proves the guard fires | fixtures double-exempt → `run()` scans **zero files**, green |
| 2 | (e) proves join makes no AC call | passes while every participant joins **nameless** (@paired-client F2) |
| 3 | "after a 401 the Sign-in nav returns" | passes with a **blank `<main>`** — nav present, nothing rendered |
| 4 | Rule 2 negative fixtures assert no finding | per-file kernel → Rule 2 **structurally disabled**, matrix fully green (F11) |

Instance 4 is the sharpest: a *passing matrix that reads as coverage*, with the rule it measures
switched off. Instance 1 at least scanned zero files.

**The reviewer question that catches this class — and it is not currently anywhere in the review
protocol**: *"what would make this assertion pass that shouldn't?"*

Route: a follow-up amendment to `.claude/skills/devloop/review-protocol.md`, **not this loop**.
Same reasoning the Lead applied to @dry-reviewer's ADR-0019 taxonomy and @semantic-guard's drift
guard — a change to how every future loop reviews deserves review *as* an amendment, not as a
rider on a client-credential task. Credit: @paired-client, with instances contributed by
@paired-infrastructure-2, @test, @security and @implementer.

### A third false-green, this time in an assertion

@paired-client's proposed test ("after a 401 at join, the Sign-in nav returns") **passes with a
blank `<main>`** — nav present, nothing rendered. That is the third false-green this loop caught
**in an assertion rather than in code**, after the double-exempt fixtures and the stale positive
fixture. The class: *an assertion that checks the wrong side of the thing it names is invisible
to review unless someone re-derives what it would pass on.*

### F-SEC-3 CASCADE — the recovery fix reintroduces the blank-page bug it neighbours

@paired-client ruled on the `onSessionExpired` addition (their tree, their call) and **accepted
the finding into this task**, correctly: the security-correct action (drop a dead credential
from retained state) and the UX recovery are *the same edit*, and retaining a known-dead bearer
token in `$state` is itself a credential-lifetime defect — this task's actual subject.

**Blocking condition.** @implementer's reading was that clearing `auth` makes the F5 derived
view "fall back to the auth views." It does not. The F5 view redirects **one direction only** —
`signup`/`signin` → `create` when authed — with no inverse. Clearing `auth` while
`view === 'join'` leaves `effectiveView === 'join'` while the join branch requires `auth`, so
**every branch falls through and `<main>` renders empty.** That is the exact failure mode F5
existed to remove, reintroduced through the new recovery path. Fix: make the derived view
bidirectional (unauthenticated + `create`/`join` → `signin`), which also lands the user where
they need to be.

**The test would have gone false-green too**: the proposed assertion ("after a 401 at join, the
Sign-in nav returns") passes with a blank `<main>` — nav present, nothing rendered. Same shape
as @paired-client's own F2. The Sign-in **view** must be asserted, not the nav button.

@paired-client notes this was their miss as much as the implementer's: F5's derived view was
specified against the states that existed then, and F-SEC-3 adds one.

### Correction to a Lead-relied-upon ruling (@paired-client, self-reported)

@paired-client previously told the Lead that `LoginCredentials`/`RegisterCredentials` survive
the new guard "on the merits, no allowlist entry needed" — which the Lead used to close the
permanently-excepted-hole question. @security then found Rule 2 was blind to union aliases, so
`$state<JoinCredentials>` would have retained a password-carrying object and produced nothing.
**The conclusion holds after the fixpoint fix, but it held by luck until then** — an
implementation property was asserted without being verified. Recorded because the Lead relied
on it.

### @security's transferable finding — shared-identity review should target the SEAMS

The Lead's standing warning was that `--specialist=security` implementing while `security`
reviews risks correlated blind spots. @security recorded how it actually played out, and the
lesson is sharper than the warning:

Three findings survived a plan **explicitly written to satisfy their own lenses**, and the two
that mattered most both lived in **seams rather than in either half**:
- **F-SEC-1** in the B1/A2 seam — B1 introduces the `JoinCredentials` union; A2's Rule 2 could
  not see union aliases, so the guard would have been silent on retention of the very union
  whose password arms B1 keeps.
- **F-SEC-3** in the B1/B3 seam — B1 makes the retained token the sole credential; B3 removes
  the only path to replace it.

Neither is visible from inside either half. **When reviewer and implementer share an identity,
the productive place to look is between the deliverables, not inside them** — both parties
reason about their own halves competently, and neither owns the join.

### "Re-measure after the thing you measured changed"

@security required the FP matrix be re-measured *after* the union-alias closure landed, rather
than trusting the pre-closure count of 3. That surfaced a **fourth** site:
`MeetingSession.ts:335` — a function parameter in a multi-line signature, lexically
indistinguishable from a field declaration. Without the exclusion, the guard red-lines
`MeetingSession.join`'s own signature: **the function this task exists to fix.**

### Lead ruling — `docs/TODO.md` classification row, owner `team-lead`

Raised by @paired-infrastructure-2, who correctly declined to nominate the owner themselves.
`docs/TODO.md` **will** appear in the Gate 2 diff — @dry-reviewer writes two TECH_DEBT entries
at verdict time, @semantic-guard's rescoped pointer-guard entry lands there, and Step 9
deferral aggregation is the Lead's job per SKILL.md. Without a row, Layer A scope-drift flags
it as an unplanned change and @implementer ends up adjudicating a false positive against their
own plan.

**Ruled `Not mine, Mechanical`, owner `team-lead`** — same basis as the three SG-5 governance
rows. It is a shared ledger with no single specialist owner, and every entry this loop adds is
authored by a *reviewer* at verdict time or by the *Lead* at Step 9, none by the implementer;
nominating a specialist would misattribute it. Additive entries only. Any *modification* or
*removal* of an existing entry is not Mechanical and returns to the Lead.

### Vocabulary stem set — converged, no CATEGORY_A mutation

**Lead framing corrected by @dry-reviewer — the two findings are NOT the same defect from
opposite ends.** An earlier draft of this section said the open `cred` prefix was
"simultaneously too narrow and too wide." That is wrong, and the error would have recorded a
live gap as closed. The two findings are in **different places**:

- **D1 (@dry-reviewer)** is about the **new guard's matcher** — an open prefix over-matches
  `creditCard`. Under an open prefix `credential`.starts_with(`cred`) succeeds, so the new
  matcher was **never too narrow**.
- **@paired-infrastructure-2's** was about the **catalog** — CATEGORY_A carries the stem `cred`
  but not `credential`/`credentials`/`creds`.

The narrowness lives in the **three existing Rust consumers**, which match `\b(…|cred|…)\b`.
`\bcred\b` cannot match `credential` — the word boundary requires a non-word character where
there is an `e`. **So a Rust variable or `#[instrument]` param named `credential` goes unflagged
today by `rust_log_secrets` and `instrument_skip_all`; `metric_labels` catches it only in
underscore form.** That is a live gap in shipped guards.

**The convergence below does NOT close that gap, and must not be recorded as doing so.** The
three consumers read CATEGORY_A directly and never see the expansion set. Closing it means
promoting `credential`/`credentials`/`creds` to full CATEGORY_A entries — security sign-off plus
a detection-behaviour change in three shipped guards that may surface violations in existing
Rust code. Unmeasured blast radius → **follow-up, not an in-loop edit.** @dry-reviewer is
routing it.

Both reviewers nonetheless converged on the same fix for the *matcher* defect: an **enumerated
expansion**
(`cred` → `{cred, creds, credential, credentials}`) matched by the same segment-equality
primitive as everything else. This collapses two matching primitives back to one, makes the
matched set finite and auditable, and leaves A-P1's verified table unchanged cell-for-cell so
the empirical sweep carries over without a re-run.

@paired-infrastructure-2 **withdrew their own proposal** (adding terms to CATEGORY_A) on
realising it would incur blast radius across three other consumers — so there is no CATEGORY_A
mutation and **no security sign-off is required** for this item. An earlier Lead instruction to
loop in @security for it is withdrawn.

**Why it was worth fixing at Gate 1 despite zero hits in the tree today** — the sharpest
statement of this loop's central risk, from @paired-infrastructure-2: *three individually
correct amendments compound.* Full-tree scanning + always-run at Layer 3 + A-P6's deliberate
no-bypass mean **one false positive hard-blocks Layer 3 for every devloop in the repo**, with
the only remedies being "edit the guard" or "rename innocent production code."
`BillingInfo { creditCard, accessToken }` and `PricingState { credits }` are unremarkable
additions to a product with billing.

### SYSTEMIC FINDING (@dry-reviewer) — catalogs encode assumptions their consumers don't implement

The most valuable thing this loop surfaced, and it is not about credentials. **#58's guard is
the first consumer that normalizes**, which is why several instances surfaced at once rather
than one at a time.

**@dry-reviewer corrected their own framing, and the correction is load-bearing.** They first
reported "three findings, one root cause." There are **two modes requiring two different
remedies** — and @paired-infrastructure-2's proposed membership test closes only one of them:

**Mode A — referential integrity under rename.** A structure references a string that must exist
elsewhere; rename the target and the reference silently orphans.
- Instances: `HYGIENE_SOURCE_SCAN_SUBSET`; the new `STEM_EXPANSIONS` keys.
- Remedy: the key-membership test. **Complete fix for this mode.**

**Mode B — matcher-capability mismatch.** The entry is well-formed *and* correctly referenced,
but no consumer's matcher can reach it. **A membership test passes and reveals nothing.**
- Instances: `accessToken` — structurally unreachable in `metric_labels`, since lowercasing
  kills it; `cred` — self-matches fine, but was chosen as a *stem* to cover
  `credential`/`credentials` while all three consumers use `\b…\b`, which cannot do morphology.

**CORRECTION to the `accessToken` scope (@dry-reviewer, self-reported; Lead verified at
source).** An earlier version of this record — and @dry-reviewer's own D2 — said `accessToken`
was "effectively dead by convention" in `rust_log_secrets` and `instrument_skip_all` because
Rust identifiers are snake_case. **That is wrong.** Those matchers do not require a Rust
identifier: `rust_log_secrets::secret_hit` runs `SECRET_WORD_RE.find(line)` — a bare
`\b(alternation)\b` against **raw line text** — and `instrument_skip_all::sensitive_param_in_window`
does the same across the signature window. So `warn!("missing accessToken in response")` matches,
as does any string literal, doc comment, or serde-rename attribute. Verified by the Lead reading
both call sites.

**Correct statement**: *one entry, three consumers, two matchers — reachable in two, dead in
one.*

**Why the correction changes the follow-up's scope rather than just its wording.** The wrong
framing sends someone to ask "is this entry earning its place?" The accurate framing is narrower
and far more actionable: **`metric_labels` lowercases before matching and therefore cannot see
*any* camelCase catalog entry.** That is a **consumer defect, not a vocabulary one**, and it is
not `accessToken`-specific — the next camelCase entry added is dead there on arrival, silently.

@paired-infrastructure-2's condition, carried verbatim into the TODO entry because it is what
stops the follow-up closing on the easy half: *if it stays dead there after the follow-up, that
must be a deliberate documented decision rather than the current silence.*
- Remedy: a **self-match reachability test** — feed every CATEGORY_A entry to each consumer's
  matcher and assert it matches itself. `metric_labels::pii_token_hit("accessToken")` returns
  `None`, so the dead entry surfaces the day it lands. ~10 lines per consumer; **would have
  caught `accessToken` at task #11.** Not in this loop — it touches three shipped consumers and
  belongs with the CATEGORY_A promotion question already routed.

**Why the correction matters more than the finding**: if the follow-up were scoped from the
original one-root-cause framing, Mode A would close, Mode B would stay open, **and it would look
closed.** That is precisely the failure shape this loop has spent its time on, arriving one
level up — in the description of the fix rather than in the fix.

**Residual with no mechanical test**: a stem whose variants were never implemented. Intent is
not in the code. `STEM_EXPANSIONS` is itself the general remedy — it converts implicit
stem-intent into a declared enumeration. @dry-reviewer notes this is a stronger justification
for the design than the `creditCard` false positive that prompted it.

@paired-infrastructure-2 independently identified a fourth instance of the same shape from the
referential-integrity side: A-P3's rename-invariance argument, the `source_scan_patterns()`
silent drop, and the new `STEM_EXPANSIONS` keying are all *"a structure keyed by a string that
must exist in another list, with nothing asserting it, failing silently under rename."* Their
proposal is a `dt-guard` convention rather than three ad-hoc tests. **Not in this loop.**

Both route to the ADR-0019 follow-up alongside the guarded-duplication rule — same lens: *the
defect is not the duplication or the entry, it is that nothing detects the divergence between
what a catalog promises and what its consumers deliver.* @dry-reviewer writes it at verdict time
with the three instances as evidence.

**Design detail that keeps the fix honest** (@paired-infrastructure-2): the expansion spellings
must NOT go into the credential/token/neither partition lists, because A-P3's union-equality
test asserts the partition is set-equal to CATEGORY_A — putting them inside would break that
invariant and force the CATEGORY_A addition back in through the side door. `STEM_EXPANSIONS`
must be a separate structure keyed by a CATEGORY_A member, with a test that the key exists in
CATEGORY_A so a rename cannot silently orphan it. @dry-reviewer names the same resolution for
the tension in their own fix: derived knowledge *about* the catalog, not a second list beside
it — and if security later promotes `credential` to a real entry, the union-equality test forces
a bucket and the expansion goes visibly redundant instead of quietly shadowing it.

**Gate 2 trip-wire, checkable in one line rather than by judgment** (@paired-infrastructure-2):
the security-sign-off trigger is not "this finding" but *"does the diff touch
`PII_TOKENS_CATEGORY_A`'s membership."* If @implementer elects to solve it by adding terms to
the catalog instead, sign-off becomes required again.

### CASCADE FROM F1 — the item-(a) positive fixture is now a negative

Found by @paired-infrastructure-2 in round 2, and it lands squarely on the blocking amendment's
own evidence path.

A-P17 gave Rule 1 a retention predicate; A-P19 excluded interface members from counting as
retention sites. **The fixture catalog was not revised to match.**
`pos_auth_result_with_password.ts` was specified when Rule 1 was retention-blind, so a bare
`interface AuthResult { password; userToken }` sufficed. Under the new rule that file contains
no retention site and therefore **fires nothing** — the positive fixture for verification item
(a) has silently become a negative.

**Fix** (precisely specified, so no judgment needed): the positive must contain a declaration
**plus** a retention site in the same file; the negative must be that same file minus the
`password` field with the retention site **intact**, so it remains a controlled experiment
isolating the credential rather than the storage. Same check owed on
`pos_nested_credential.tsx` and `pos_retained_union_alias.ts`.

**Why this did not hold Gate 1, and what the real risk is.** It is **fail-loud** — @implementer
hits it as a test failure the moment the fixture test is written, because "pos fires" will not
hold. The residual risk is not that it ships broken; it is that **the quick repair under time
pressure is to loosen the rule instead of fixing the fixture.** A-P17 is correct and must not be
relaxed to make a stale fixture pass. Recorded here as a Gate 2 criterion precisely because the
wrong repair would be locally reasonable and globally wrong.

### Gate 2 acceptance criteria — CONSOLIDATED (authoritative list)

Assembled from @paired-infrastructure-2, @dry-reviewer, @test, @operations, @security and
@code-reviewer. This is the list the Lead checks at Gate 2; scattered copies elsewhere in this
document are subordinate to it.

**Evidence integrity**
1. **Pre-B2 guard run captured verbatim** (file:line + rule_id) **before B2 lands**, with the
   post-B2 clean run beside it. Raised independently by @paired-infrastructure-2 and @test. Only
   non-circular evidence in the loop; destroyed the moment B2 removes the field. *Ordering is
   the risk, not effort.*
2. **Item-(a) fixture pair contains a retention site**, and the negative differs from the
   positive by the `password` field only. **A-P17 must not be relaxed to make a stale fixture
   pass.**
3. **Live `run-guards.sh` smoke** showing the guard discovered, executed, and failing on a real
   violation.
4. **One FP matrix measured over the retention gate** — after the F1 collapse this is the only
   empirical bound on a full-tree, no-bypass, always-run guard. Must include the Rule 2 surface
   (interface members, type-literal members, `$props()` exclusions).

**Wiring**
5. **`git ls-files -s` shows mode `100755`** on `scripts/guards/simple/ts/no-retained-credentials.sh`.
   Registration *is* `chmod +x`; `run-guards.sh` discovers by `find` and gates on `[[ -x ]]`, so
   a missing exec bit ships the entire (A) half as a no-op behind a green pipeline.
6. **`is_scan_exempt` called, not hand-rolled** (@dry-reviewer) — verified against the diff, not
   the plan. Hand-rolling flips this from tech-debt to a blocking true-duplication finding.

**Boundaries**
7. **Diff does not touch `PII_TOKENS_CATEGORY_A` membership.** If it does, security sign-off per
   the file header is back on. Checkable in one line rather than by judgment.
8. **ADR-0024 hunk confined to line ~49**, nothing in §6.2/§6.3/§6.4 (@code-reviewer verifies).
9. **GSA upgrade condition verified against the actual diff**, not the commit message
   (@code-reviewer): B1 must not change which token type or scope MC accepts.

**Record**
10. **Measured full-tree wall time** in the Implementation Summary. 118 files / ~470 KB is well
    inside budget, but this is the first full-tree guard in the TS set and the next author
    copying the pattern should find a baseline rather than an assumption.

### Gate 2 acceptance criteria carried forward (superseded by the consolidated list above)

1. **Pre-B2 guard run must precede B2 and be pasted verbatim** (file:line + rule_id), alongside
   the post-B2 clean run. Raised independently by @paired-infrastructure-2 and @test. It is the
   only non-circular evidence in the loop and is destroyed the moment B2 removes the field —
   ordering is the risk, not effort.
2. **`git ls-files -s` showing mode `100755`** on the new wrapper. `run-guards.sh` discovers by
   `find` and gates on `[[ -x ]]`, so a missing exec bit means the guard is silently never run
   and the entire (A) half ships as a no-op behind a green pipeline.
3. **`is_scan_exempt` called, not hand-rolled** (@dry-reviewer) — verified against the diff, not
   the plan. Hand-rolling the test-path half flips this from tech-debt to a blocking
   true-duplication finding.
4. **One FP matrix measured over the retention gate** — after the F1 collapse this is the only
   empirical bound on a full-tree, no-bypass, always-run guard.
5. **Measured scope correction**: 118 tracked `.ts`/`.tsx`/`.svelte` files, ~470 KB — not "a few
   hundred." Three orders of magnitude inside the 30s per-guard timeout; full-tree confirmed as
   the right call.

### F1 RESOLUTION — the Rule 1 / Rule 2 collapse (Lead-consolidated)

Three reviewers converged from different routes; recorded here because the reasoning is more
valuable than the outcome.

**The collision** (@code-reviewer): A-P6 removed the `guard:ignore` hatch, justifying it as
*"a **retained** type pairing a credential with a token is never correct."* But Rule 1 as
specified was retention-**blind** — so A-P6 defended a rule that was never specified. Concrete
consequence under A-P1's segment matcher: `{newPassword, resetToken}` is an ordinary,
*correct*, transient password-reset DTO; `newPassword` → `[new, password]` and `resetToken` →
`[reset, token]` both match; with no hatch it blocks every devloop in the repo, and the
documented remedy ("remove the field or stop retaining the type") does not parse for a
transient DTO.

**The trap in the obvious fix** (Lead): adding the retention predicate to Rule 1 makes it a
**strict subset of Rule 2** — Rule 2 pass-1 already collects every credential-carrying type
regardless of token, so every retention-gated Rule 1 hit is already a Rule 2 hit. Rule 1's
measured zero-FP matrix would then bound nothing independently, and A-P6's justification would
silently shift onto Rule 2's *unmeasured* heuristic surface.

**The resolution** (@dry-reviewer, superseding the Lead's weaker framing): the Lead called Rule
1 "a more specific finding message." @dry-reviewer identified it as a **DRY defect in the
guard's own internals** — two independent predicates implement "retention" twice with nothing
asserting the implementations agree. That is derived-state duplication: the exact failure class
this guard exists to detect, reproduced inside the guard.

**Binding shape**: one retention-gated detector, single pass, rule ID selected by
token-presence. The subset must hold **structurally** — emit from inside the retention branch —
**not** via a test asserting the two rules agree. A test can be deleted, skipped, or amended; a
structure cannot drift from itself. Same principle as the fixture-double-exemption and
union-equality-over-count-test rulings: prefer the shape where the failure mode is *impossible*
over the shape where it is merely *detected*.

**Follow-ons**: A-P6's no-hatch justification rewrites around the retention gate (a stronger
argument — it rests on semantics, not on a snapshot of today's tree); one FP matrix measured
over the retention gate becomes the *only* empirical bound on a full-tree, no-bypass, always-run
guard; and the A1 prose must name retention as the single gate with token-presence as a
refinement, or doc and guard diverge on day one (@dry-reviewer's original brief risk, arriving
as a concrete instance).

**Framing note**: the task statement always said "a **retained** auth-result/session type that
carries a `password` field alongside a token field." Retention was specified and dropped during
design. F1 is a drift-from-spec catch, not a new requirement.

(Roster is **nine** reviewers — the seven mandatory plus two additive paired seats. Earlier
Lead messages in this loop said "eight"; that was an arithmetic error, corrected here.)

### OWNER CONSTRAINT — SG-5 must NOT be applied to ADR-0024 §6.4

Raised unprompted by @code-reviewer, verified by the Lead, and issued to @implementer as an
owner constraint rather than left as a Gate 3 finding.

The SG-5 instruction ("remove the duplicate information and instead have pointers to the single
authoritative source") is phrased generally, and **§6.4's enumerated Guarded Shared Areas list
is the most conspicuous duplicated enumeration in ADR-0024** — mirrored in five locations, each
carrying a comment telling the reader to update all five together. A conscientious sweep would
land on it.

**It must not be dereferenced.** Verified mechanism: `crates/dt-guard/src/gsa_sync.rs` holds the
fully-expanded canonical list in `const CANON`, and runs a **count-check** asserting that the
number of backticked tokens in the ADR's §6.4 slice equals `CANON.len()`. So §6.4 satisfies
CLAUDE.md §Single source of truth via its *second* branch — "or add a guard that fails
validation on drift." Dereferencing it would delete a working control while technically
satisfying the instruction, and would break `gsa-sync` at Layer 3.

**Generalisable rule, worth carrying past this loop**: *"duplication" and "duplication with a
guard on it" are different categories.* The SG-5 instruction does not distinguish them. Before
dereferencing any enumeration, check whether something already asserts it.

**Gate 3 acceptance criterion** (checkable, not a judgment call): the four-check enumeration
occurs **exactly once** in ADR-0024 — line 49, §1 Team Composition table — confirmed by grep.
So the hunk touches line ~49 and **nothing** in §6.2/§6.3/§6.4. If it strays into §6,
@code-reviewer upgrades and it auto-routes to ESCALATE.

### SG-5 confirmation: is the Lead's owner-confirmation ceremonial?

Asked by the Lead against their own position; ruled by @code-reviewer. **Mostly no.** The Lead
is not writing these hunks — @implementer is — so this is an owner confirming someone else's
edit to the owner's file, which is the situation §6.3 exists for. The residue ("specify, then
accept") is what every owner does. The security-implements-security parallel does not hold,
since there the same party would have authored and reviewed.

**One carve-out — `docs/decisions/adr-0024-agent-teams-workflow.md` ONLY.** That file is the
ADR governing the confirmation procedure being used to confirm it, and that circularity is not
cured by care. `CLAUDE.md` and `.claude/skills/devloop/SKILL.md` need **no** co-sign.
(Correction per @code-reviewer: an earlier draft of this section implied the co-sign was
warranted because the change is an SSoT de-duplication, which would extend it to all three
governance rows. The de-duplication nature is why **@dry-reviewer** is the right co-signer —
ADR-0019 territory, their actual lens — but the *circularity* is what makes a co-sign necessary
at all, and that is specific to ADR-0024.)

**CO-SIGN GRANTED** by @dry-reviewer, on a verified boundary. They checked both Lead claims at
source rather than accepting them. Their findings sharpen the criterion:
- Line 49 sits under `## Decision` (line 29); §6.2/§6.3/§6.4 begin at lines 361/377/385. The
  hunk is ~300 lines clear of §6 — "hard to violate by accident," not merely checkable.
- **Line 49 already cites `scripts/guards/semantic/checks.md` by path, immediately before the
  parenthetical.** The pointer exists; the parenthetical is pure redundancy beside it. So the
  correct edit is *delete the parenthetical and change nothing else* — no restructuring of the
  row, no touching the blocking-policy column or the "distinct from code-reviewer's general
  lens" clause. An `e.g.`-marked example is equally acceptable per the owner's refinement; a
  bare list is not. **Restructuring the row would be straying.**

**Correction in the Lead's favour on the guard mechanism**: `gsa_sync` is stronger than the
Lead described. Beyond the count-check on §6.4's backticked tokens, it runs **per-entry
membership checks in both directions** for the YAML mirror — every `CANON` entry must appear as
a key, and every key must be in `CANON` or `INTERSECTION_SUBPATHS`. So a same-count swap of one
path for another is caught too, not just a length change. The owner constraint holds a fortiori.

### Roster note (environment deviation)

This session exposes no `TeamCreate`/`TeamDelete` tools — the `Agent` tool documents
`team_name` as "Deprecated; ignored. The session has a single implicit team." Teammates are
therefore spawned as named `Agent` invocations and address each other by name via
`SendMessage`. Step 8.5's `TeamDelete` is a no-op; teammates are shut down individually.

Second deviation: the specialist `subagent_type`s (`security`, `client`, `test`, …) are NOT
registered as agent types in this session — the available types are the generic set
(`general-purpose`, `Explore`, `Plan`, …). Teammates are therefore spawned as
`general-purpose` with an explicit Step 0 instruction to read their own
`.claude/agents/{name}.md` identity file and `docs/specialist-knowledge/{name}/INDEX.md`
navigation map before doing anything else. Identity content is identical; the loading is
explicit rather than automatic. Reviewers likewise read
`.claude/skills/devloop/review-protocol.md` themselves rather than receiving it inlined.

Because `--specialist=security` makes security the *implementer*, the security **reviewer**
slot is filled by a separate `security`-identity agent so the Gate 3 security lens is not
self-review. `paired-client` and `paired-infrastructure` are additive slots (neither
coincides with a mandatory reviewer).

---

## Task Overview

### Objective

Two halves:

- **(A) Guard extension** — client code currently has *no* semantic guard lens. The semantic
  credential-leak check is Rust-only and scoped to exfiltration-via-logs, not credential
  *lifetime*; the client's only guards are string-scans that catch hardcoded literals, not
  retained runtime state. Nothing owns "don't keep the password once you hold a token."
  - **A1**: extend `scripts/guards/semantic/checks.md` with a client-scoped credential-lifetime
    lens covering `.ts`/`.svelte`.
  - **A2**: add a deterministic mechanical guard as a `dt-guard` subcommand (ADR-0034) that
    flags a retained auth-result/session type carrying a `password` field alongside a token
    field, with pass/fail fixtures.

- **(B) The fix it forces** — credential minimization.
  - **B1**: `sdk-core` — `JoinCredentials` gains a token variant; `MeetingSession.#authenticate`
    uses the supplied bearer token instead of calling AC register/login.
  - **B2**: `web-app` — drop `password` from `AuthResult`; `JoinMeeting` passes `auth.userToken`;
    remove the `mode:'login'` band-aid.
  - **B3**: don't render Sign-up/Sign-in nav once authenticated, covered by a component test.

### Scope
- **Service(s)**: none (client packages + guard tooling only)
- **Schema**: No
- **Cross-cutting**: Yes — guard pipeline (infrastructure/ADR-0034) + client packages

### Debate Decision
NOT NEEDED — the design is fixed by the task statement and ADR-0034 already governs the
guard-as-dt-guard-subcommand shape.

---

## Cross-Boundary Classification

<!-- Filled by implementer at planning; reviewers confirm/upgrade at Gate 1. -->

Implementer is **security**. No path below is in the ADR-0024 §6.4 GSA enumerated list, and
none meets the §6.4 criterion (no wire-format runtime coupling, no auth-*routing* policy, no
detection/forensics contract, no schema evolution). `scripts/guards/semantic/checks.md` is a
detection *policy* document but not a forensics/audit contract in the §6.4 sense — flagged
here so reviewers can upgrade if they read it otherwise.

Both owners are paired on this loop, so Domain-judgment rows are satisfied by
`--paired-with` (SKILL.md §Owner Involvement) rather than by re-routing to a separate devloop.

| Path | Change pattern | Classification | Owner (if not mine) |
|------|----------------|----------------|---------------------|
| `crates/dt-guard/src/ts_retained_credentials.rs` (new) | new policy module (kernel + `run()`) | Not mine, Domain-judgment | infrastructure |
| `crates/dt-guard/src/lib.rs` | one `pub mod` line | Not mine, Mechanical | infrastructure |
| `crates/dt-guard/src/main.rs` | clap variant + dispatch arm | Not mine, Mechanical | infrastructure |
| `crates/dt-guard/src/common/pii_vocabulary.rs` | total 3-way partition + union test | Not mine, Minor-judgment | infrastructure |
| `crates/dt-guard/tests/ts_retained_credentials_fixtures.rs` (new) | fixture-driven kernel tests | Not mine, Domain-judgment | infrastructure |
| `crates/dt-guard/tests/fixtures/ts_retained_credentials/neg_transient_login_params.ts` (new) | fixture | Not mine, Domain-judgment | infrastructure |
| `crates/dt-guard/tests/fixtures/ts_retained_credentials/pos_nested_credential.tsx` (new) | fixture (`.tsx` + nesting) | Not mine, Domain-judgment | infrastructure |
| `scripts/guards/simple/ts/no-retained-credentials.sh` (new, mode 100755) | 4-line wrapper | Not mine, Minor-judgment (upgraded @code-reviewer: no guard covers wrapper-registration) | infrastructure |
| `scripts/guards/semantic/checks.md` | new `## Check:` section | Not mine, Domain-judgment | infrastructure **+ semantic-guard** (co-sign, SG-8) |
| `.claude/agents/semantic-guard.md` | dereference enumeration + fixture carve-out | Not mine, Minor-judgment | semantic-guard |
| `docs/observability/metrics/client.md` | `credential_invalid` value + emission-honesty bullet | Not mine, Domain-judgment | observability |
| `CLAUDE.md` | dereference check enumeration (SG-5, **APPROVED**) | Not mine, Minor-judgment | team-lead |
| `.claude/skills/devloop/SKILL.md` | dereference check enumeration (SG-5, **APPROVED**) | Not mine, Minor-judgment | team-lead |
| `docs/decisions/adr-0024-agent-teams-workflow.md` | dereference check enumeration (SG-5, **APPROVED**) | Not mine, Minor-judgment | team-lead |
| `docs/runbooks/devloop-validation.md` | §6.3 REASON row + §8 row + TS ignore-marker correction | Not mine, Minor-judgment | operations |
| `packages/sdk-core/src/session/events.ts` | new union variant (public API) | Not mine, Domain-judgment | client |
| `packages/sdk-core/src/session/MeetingSession.ts` | exhaustive `switch` + token path | Not mine, Domain-judgment | client |
| `packages/sdk-core/src/validation/limits.ts` | add `validateUserToken` | Not mine, Minor-judgment | client |
| `packages/sdk-core/src/session/__tests__/meeting-session.test.ts` | additive token + control cases | Not mine, Minor-judgment | client |
| `packages/sdk-core/src/validation/__tests__/limits.test.ts` | `validateUserToken` cases incl. CRLF | Not mine, Minor-judgment | client |
| `packages/web-app/src/lib/types.ts` | `AuthResult` → `AuthSession`, field removal | Not mine, Domain-judgment | client |
| `packages/web-app/src/App.svelte` | total shell state machine | Not mine, Domain-judgment | client |
| `packages/web-app/src/views/JoinMeeting.svelte` | token credentials, band-aid removal | Not mine, Domain-judgment | client |
| `packages/web-app/src/views/SignUp.svelte` | `onAuthed` shape + password zeroing | Not mine, Minor-judgment | client |
| `packages/web-app/src/views/SignIn.svelte` | `onAuthed` shape + password zeroing | Not mine, Minor-judgment | client |
| `packages/web-app/src/__tests__/appShell.test.ts` (new) | shell nav/state tests | Not mine, Minor-judgment | client |
| `packages/web-app/src/__tests__/joinMeeting.test.ts` | fixture literal + type rename | Not mine, Mechanical | client |
| `packages/web-app/src/__tests__/createMeeting.test.ts` | fixture literal + type rename | Not mine, Mechanical | client |
| `packages/web-app/src/__tests__/authViews.test.ts` | replace `mode` assertion | Not mine, Minor-judgment | client |
| `crates/env-tests/src/fixtures/auth_client.rs` | add `call_count()` (Gate-3 T1: observe AC requests, not token equality) | Not mine, Minor-judgment | test |
| `crates/env-tests/tests/24_join_flow.rs` | pin token-only property (C2) | Not mine, Minor-judgment | test |
| `docs/user-stories/2026-05-02-browser-client-join.md` | (c) re-scope + #18 note (C1) | Not mine, Minor-judgment | test |
| `packages/web-app/src/views/CreateMeeting.svelte` | `AuthResult`→`AuthSession` type-name rename only | Not mine, Mechanical | client |
| `crates/dt-guard/src/secret_patterns.rs` | union-equality test closing the `source_scan_patterns()` silent-drop | Not mine, Minor-judgment | infrastructure |
| `crates/dt-guard/tests/fixtures/ts_retained_credentials/neg_bare_string_form_state.svelte` (new) | fixture (pins the documented blind spot does NOT fire) | Not mine, Domain-judgment | infrastructure |
| `packages/sdk-core/src/index.ts` | add `TokenCredentials` to the re-export list | Not mine, Mechanical | client |
| `docs/specialist-knowledge/infrastructure/INDEX.md` | additive navigation pointer to the new guard | Not mine, Mechanical | infrastructure |
| `docs/TODO.md` | additive tech-debt/deferral entries only | Not mine, Mechanical | team-lead |
| `crates/dt-guard/tests/fixtures/ts_retained_credentials/pos_xfile_decl.ts` (new) | fixture (F11 cross-file decl half) | Not mine, Domain-judgment | infrastructure |
| `crates/dt-guard/tests/fixtures/ts_retained_credentials/pos_xfile_retention.svelte` (new) | fixture (F11 cross-file retention half) | Not mine, Domain-judgment | infrastructure |
| `crates/dt-guard/tests/fixtures/ts_retained_credentials/pos_xfile_decl.ts` (new) | fixture: cross-file decl half (silent alone) | Not mine, Domain-judgment | infrastructure |
| `crates/dt-guard/tests/fixtures/ts_retained_credentials/pos_xfile_retention.svelte` (new) | fixture: cross-file retention half (fires only as a set) | Not mine, Domain-judgment | infrastructure |
| `crates/dt-guard/tests/fixtures/ts_retained_credentials/neg_xfile_decl.ts` (new) | fixture: item-(a) controlled negative, credential field removed | Not mine, Domain-judgment | infrastructure |
| `crates/dt-guard/tests/fixtures/ts_retained_credentials/neg_xfile_retention.svelte` (new) | fixture: retention site INTACT (isolates the one variable) | Not mine, Domain-judgment | infrastructure |
| `crates/dt-guard/tests/fixtures/ts_retained_credentials/pos_union_decl.ts` (new) | fixture: union-alias decl half (F-SEC-1 closure) | Not mine, Domain-judgment | infrastructure |
| `crates/dt-guard/tests/fixtures/ts_retained_credentials/pos_retained_union_alias.svelte` (new) | fixture: union-alias retention half | Not mine, Domain-judgment | infrastructure |
| `crates/dt-guard/tests/fixtures/ts_retained_credentials/pos_nested_credential.tsx` (new) | fixture: nested literal + `.tsx` coverage | Not mine, Domain-judgment | infrastructure |
| `crates/dt-guard/tests/fixtures/ts_retained_credentials/neg_transient_login_params.ts` (new) | fixture: call-scoped parameter (lens-4 case) | Not mine, Domain-judgment | infrastructure |
| `crates/dt-guard/tests/fixtures/ts_retained_credentials/neg_bare_string_form_state.svelte` (new) | fixture: documented blind spot, pinned not-firing | Not mine, Domain-judgment | infrastructure |
| `crates/dt-guard/tests/fixtures/ts_retained_credentials/neg_exclusions_real_shapes.ts` (new) | fixture: interface-member + multi-line-param exclusions | Not mine, Domain-judgment | infrastructure |
| `crates/dt-guard/tests/fixtures/ts_retained_credentials/neg_props_type_literal.svelte` (new) | fixture: `$props()` exclusion | Not mine, Domain-judgment | infrastructure |
| `crates/dt-guard/tests/fixtures/ts_retained_credentials/neg_recursive_types.ts` (new) | fixture: cycle termination | Not mine, Domain-judgment | infrastructure |
| `crates/dt-guard/tests/fixtures/ts_retained_credentials/lens/pos_resend_credentials_with_token_held.svelte` (new) | LENS fixture (sub-check ii); guard silent by design | Not mine, Domain-judgment | infrastructure |
| `crates/dt-guard/tests/fixtures/ts_retained_credentials/lens/pos_resend_no_retention.svelte` (new) | LENS fixture: isolates (ii) from (i) | Not mine, Domain-judgment | infrastructure |
| `crates/dt-guard/tests/fixtures/ts_retained_credentials/lens/neg_form_input_binding.svelte` (new) | LENS fixture: FP trap for SG-3 | Not mine, Domain-judgment | infrastructure |
| `packages/web-app/src/lib/errorText.ts` | add `isSessionRejection` — defines which server responses prove a credential is dead (401 only) | Not mine, **Domain-judgment** | client |
| `.gitignore` | ignore Vitest browser FAILURE artifacts (screenshots/attachments) | Not mine, Mechanical | infrastructure |
| `docs/specialist-knowledge/security/INDEX.md` | pointer to new guard | Mine | — |
| `docs/devloop-outputs/2026-07-29-client-credential-lifetime-guard-task58/main.md` | this document | Mine | — |

**Explicitly NOT Mechanical** (pre-empting the @security lens-8 challenge): the
`JoinCredentials` union change in `packages/sdk-core/src/session/events.ts` is a public SDK
API-surface change, not value-neutral or structure-preserving — Domain-judgment, owner
client.

**~~Not touched~~ — STRUCK 2026-07-30** (@code-reviewer F3, @semantic-guard SG-10). This
paragraph originally said `pii_vocabulary.rs` gains nothing and that I would re-open the plan
rather than edit it silently. That commitment was **re-opened in the open** and renegotiated at
Gate 1: @dry-reviewer's Call 1 ruled the credential/token partition belongs co-located with the
catalog, and D2 requires a comment correction there. The table row governs. Struck rather than
deleted so the record shows the commitment was renegotiated, not quietly broken.

---

## Planning

### Mechanism restatement (SKILL.md step 1)

Instance-language (as the task states it): "the web-app keeps the password in `AuthResult`
and replays it at join."

Mechanism-language: **the guard pipeline's credential lens is scoped to *exfiltration events*
(a secret reaching a log sink / a literal in source), not to *credential lifetime* (a secret
still being reachable after the exchange that made it unnecessary). Client code additionally
has no semantic lens at all.** The defect is what that blind spot admits.

Restating this way widens the class in one useful direction and I am surfacing it rather than
silently expanding scope: the mechanism says the predicate is **retention**, not
"password-next-to-token". The task's named predicate ("a retained auth-result/session type
that carries a `password` field alongside a token field") describes *this* instance. A
retained type carrying a password with *no* token field is the same defect and would pass
that predicate. This is also @security's lens 1. My answer is in A2 below: **two rules**, one
of which is retention-keyed and name-agnostic.

The class does not widen across owners — every sibling is in `packages/**` (client) or the
guard tree (infrastructure), both paired here. No wider sweep is proposed.

### (A2) The mechanical guard — `dt-guard ts-no-retained-credentials`

New policy module `crates/dt-guard/src/ts_retained_credentials.rs`; wrapper
`scripts/guards/simple/ts/no-retained-credentials.sh` (4 lines, sourcing `_dt_guard_wrapper.sh`
like its six siblings). Scope: tracked `.ts` / `.tsx` / `.svelte` under `packages/**`, via
`git ls-files` (`common::git_changes::get_tracked_files`), minus
`common::test_code_filter::is_scan_exempt`.

**Full-tree, not diff-scoped** — deliberate. A diff-scoped guard would go quiet the moment
the offending declaration stops being touched, which is exactly how the current defect
survived. Full-tree over `git ls-files packages/**` is a few hundred files and never sees
`node_modules/` (gitignored).

**Vocabulary comes from the existing SoT** (@security lens 2): the credential set and the
token set are both derived from `crates/dt-guard/src/common/pii_vocabulary.rs::PII_TOKENS_CATEGORY_A`
(which already carries `password`, `passwd`, `pwd`, `cred`, `secret`, `token`,
`access_token`, `accessToken`, `jwt`, `bearer`, …), partitioned into a credential subset and
a token subset inside `pii_vocabulary.rs`'s consumer module. No fresh `"password"` literal.
Field-name matching is camelCase-and-snake_case aware and honours `CATEGORY_A_ALLOWLIST`
(`token_type`).

Two rules:

- **Rule 1 — `auth_state_password_with_token`** (the task's named predicate). A TS/Svelte
  type declaration (`interface X { … }` / `type X = { … }`) whose field set contains **both**
  a credential-vocabulary name and a token-vocabulary name. Zero-FP by construction: a
  request DTO carries one or the other, never both. This is what the conjunct buys.
- **Rule 2 — `retained_credential_binding`** (closes lens 1's false-negative class and
  lens 4's bypass). Pass 1 collects every in-repo type declaration carrying a
  credential-vocabulary field — *regardless of whether it also has a token*. Pass 2 flags any
  **retention site** annotated with one of those type names. "Retention site" is a small,
  enumerated, in-repo-idiom set:
  - Svelte 5 runes: `$state<T…>`, `$derived<T…>`
  - class property declarations, incl. private fields: `#f: T`, `readonly #f: T`, `f: T =`
  - module-scope `let` / `const` with a type annotation
  - Svelte stores: `writable<T…>` / `readable<T…>`

  Naming is irrelevant to Rule 2 — calling the type `JoinParams` does not help, because the
  finding is on the *storage*, not the name. That is the answer to @security lens 4: the
  discrimination is retained-vs-transient, and a function-parameter annotation
  (`function f(c: LoginCredentials)`) is not a retention site, so `LoginCredentials` /
  `RegisterCredentials` legitimately survive in the `JoinCredentials` union.

  **Stated limitation, not implied**: the retention-idiom set is enumerated, not exhaustive
  — a novel storage idiom (e.g. stuffing the value into a `Map`, or an untyped object
  literal with no annotation) is a Rule-2 false negative by construction. That gap is
  deliberately A1's job (the semantic lens reads intent, the mechanical guard reads syntax),
  and the module doc will say so rather than implying completeness. **Nested credential
  objects** (`readonly creds: { password: string }`) are IN scope for the field scan — the
  block scanner walks nested braces within a declaration and attributes nested field names to
  the enclosing declaration (@security lens 2, second half).

**Evidence the guard is not fitted to a fixture**: run against the *current* tree (pre-B2),
both rules fire on the real defect — Rule 1 on `AuthResult` (`password` + `userToken`),
Rule 2 on `App.svelte`'s `let auth = $state<AuthResult | undefined>(…)`. After B2 both go
quiet. That before/after on production code is the primary proof; the committed fixtures are
the regression lock.

**Registration** (@security lens 3): the wrapper lands in `scripts/guards/simple/ts/`, which
`run-guards.sh` auto-discovers (`find simple/**/*.sh -type f`, `[[ -x ]]`), and Layer 3
always runs `run-guards.sh`. Verification step below asserts the guard actually appears in
Layer 3 output by name — existence without execution is a masked failure. Rust unit tests in
the module + an integration test drive the fixtures; both run under Layer 4 `cargo test`.

Fixtures land at `crates/dt-guard/tests/fixtures/ts_retained_credentials/` (mirroring the
existing `fixtures/cite_extract/` precedent; doubly excluded from every other TS guard by
`is_guard_internal_path` + `is_test_path`, and pruned by `run-guards.sh`'s `-not -path
'*/fixtures/*'`):
`pos_auth_result_with_password.ts` (Rule 1 fires) · `neg_auth_result_token_only.ts` (same
file minus the field — Rule 1 silent) · `pos_retained_credential_state.svelte` (password-only
type stored in `$state<…>` — Rule 2 fires, Rule 1 does not: the lens-1 case) ·
`neg_transient_login_params.ts` (same type used only as a function parameter — both silent:
the lens-4 case).

### (A1) The semantic lens — `checks.md`

New `## Check: Client Credential Lifetime` section, scoped to `.ts` / `.tsx` / `.svelte`
production code, with the two sub-checks the task names — (i) a raw password/secret still
reachable from session/auth state after the token exchange, (ii) credentials re-sent on a
request when a bearer token for the same principal is already held — each with UNSAFE and
SAFE example pairs in the existing house style, and an explicit note that it owns the
retention idioms Rule 2 cannot see.

**SSoT fix (@dry-reviewer, please rule):** `.claude/agents/semantic-guard.md` hard-codes the
four check names in four places (role sentence + three output templates). Adding a fifth
check means five edits that will drift. I propose replacing the enumerations with "the checks
enumerated in `scripts/guards/semantic/checks.md`" and having the `Checked:` line list the
`## Check:` headings actually read. That makes `checks.md` the single source of truth and
makes the next check a one-file change. This is inside the "fix, don't defer" line (small,
in-tree), but it touches an agent definition, so I want @semantic-guard + @dry-reviewer to
confirm.

### (B1) sdk-core — token join path

`events.ts`: add

```ts
export interface TokenCredentials {
  readonly mode: 'token';
  readonly userToken: string;
  readonly displayName?: string;
}
export type JoinCredentials = TokenCredentials | LoginCredentials | RegisterCredentials;
```

`TokenCredentials` is the path the demo uses. `LoginCredentials` / `RegisterCredentials`
survive **only** for standalone join (an SDK embedder with no prior auth step who wants
`join()` to do the whole thing). Explicit criterion, per @security lens 4: *the password path
is legal only where the credential is a call-scoped argument that no caller retains* — which
is why it is a parameter type reached through `join(options)` and never a field of
`MeetingSession`. Their doc comments will state this, and Rule 2 enforces it mechanically the
moment someone stores one.

`MeetingSession.#authenticate` gains a leading `if (c.mode === 'token') return c.userToken;`
— no AC call on that path.

**New trust boundary** (@security lens 5 — accepted, doing it): a caller-supplied token now
reaches an `Authorization: Bearer` header that the SDK did not mint. I will add
`validateUserToken` to `packages/sdk-core/src/validation/limits.ts` (alongside the existing
`validatePassword` / `validateSubdomain` / `validateMeetingCode` family) rejecting empty,
over-length, and any character outside the RFC-7235 `token68` set — which rejects CR, LF, and
every other control character, so header splitting is structurally impossible. Called at the
top of the `mode === 'token'` branch, before the token is handed to `MeetingApiClient`. Unit
tests in `validation/__tests__/limits.test.ts` incl. an explicit CRLF-injection case.

**Observability note for @observability**: on the token path, `stage` is set to
`FailureStage.Signup` and then immediately overwritten by `FailureStage.GcJoin` without any
awaitable in between, so `signup` can no longer be emitted for token joins. That is honest
(the client cannot fail at a stage it does not execute) and no catalog change follows, but
I'd like it confirmed rather than assumed. No new metric, no label-value addition.

### (B2/B3) web-app — hold a session, not a credential

`lib/types.ts`: `AuthResult` → **`AuthSession`**, fields `{ subdomain, displayName?, userToken }`.
`password` goes (the defect). `mode` goes — the story text for #58 says so explicitly
("`mode` is gone and the register-vs-login class is structurally impossible"), and nothing
consumes it after B1. `email` goes — its *only* consumer today is `JoinMeeting.credentials()`,
which this task deletes; keeping unused identity state in a retained object is the same
minimization mistake one field over. The rename is the "missing session-state concept" the
task names; if reviewers prefer to keep the `AuthResult` name I'll keep it, but the field set
is not negotiable.

`JoinMeeting.svelte`: `credentials()` collapses to `{ mode: 'token', userToken: auth.userToken, …displayName }`;
the band-aid comment block goes.

`App.svelte` (B3, treated as a **security control**, @security lens 7): the Sign-up / Sign-in
nav buttons are **removed from the DOM**, not disabled or hidden — `{#if !auth}` around them
— and the `{#if view === 'signup'}` / `signin` branches are likewise gated on `!auth`, so
`auth` cannot be replaced by re-entering an auth view even if `view` were forced. The demo's
"switch account" affordance would be a sign-out that clears the session, not a re-entry; none
exists today and I am not adding one.

**Credential memory paths** (@security lens 6, enumerated as asked):
- `SignUp.svelte` / `SignIn.svelte` hold `let password = $state('')`. `App.svelte` renders
  exactly one view (`{#if}`/`{:else if}` chain), so the transition to `create` unmounts the
  component and the rune state becomes unreachable. I will **confirm this by test**, not by
  reading — and additionally set `password = ''` immediately after a successful token
  exchange (one line per view) as defense in depth, since JS strings are immutable and
  dropping the reference is the only available "zeroing".
- `AuthApiClient.register/login` build the request body inline in the `JSON.stringify(...)`
  argument and keep no field on the instance — verified by read; I will re-verify at
  implementation and say so plainly if wrong.
- `AuthError` / `SdkError` / `errorText`: to be re-verified that no error path captures the
  request body or `cause` chain carrying it. If one does, that is a finding I fix here.

### Tests / verification map

| Item | Deliverable | Layer |
|------|-------------|-------|
| (a) | `ts_retained_credentials_fixtures.rs` — pos fixture FAILS, neg fixture PASSES, for both rules | 4 (`cargo test`) |
| (a′) | Guard run against pre-B2 tree fires on the REAL defect; against post-B2 tree is clean | 3 |
| (b) | Semantic lens exercised by @semantic-guard against the fixture `.svelte`/`.ts` pair above | Gate 3 |
| (c) | See "the (c) question" below | 7 |
| (d) | `git grep` for a `password` field on any retained client type — 0 hits, asserted by the Rule-2 guard itself, which is strictly stronger than grep | 3 |
| (e) | `meeting-session.test.ts`: join with `{mode:'token'}` against a `fetchImpl` spy asserts **zero** requests to `/api/v1/auth/register` and `/api/v1/auth/user/token`, and that the supplied token is what reaches GC | 4 |
| (f) | `packages/web-app/src/__tests__/appShell.test.ts` (new): render `App`, complete sign-up against a stubbed `fetch`, assert `nav-signup` / `nav-signin` absent and `nav-create` / `nav-join` present | 4 (browser mode) |

### The (c) question — needs a ruling from @test + @operations before I build it

(c) as written is "`MeetingSession.join` succeeds against the live Kind cluster using ONLY a
userToken". Stating the situation plainly rather than quietly redefining it:

- The **server half is already covered** (on any local/container devloop with a live helper; Layer 7 cleanly skips in CI as `SKIPPED-NO-CLUSTER`):
  `crates/env-tests/tests/24_join_flow.rs::test_mc_webtransport_connect_and_join` registers a
  user once, then uses that single `user_token` for create → GC join → MC WebTransport join,
  with no second AC call. That is exactly the token-only contract B1 depends on, on the live
  cluster, at Layer 7.
- The **client half** — driving the actual TS `MeetingSession.join` against the cluster —
  needs a browser that can open WebTransport to MC with dev-cert fingerprints. That harness
  (`global-setup.ts` reading `MC_CERT_SHA256_B64` / `MH_CERT_SHA256_B64`, `bootstrapMeeting`,
  `joinAsUser`) **is task #18**, which the story deliberately sequenced *after* #58 so that
  #58 could define the contract first ("writing #18 first would force #58 to rewrite it").

So my proposal is: (c) = the existing Layer-7 env-test (which I will run and cite, and extend
with an explicit assertion if it turns out the token reuse is incidental rather than pinned)
+ the deterministic (e) proof that the SDK makes no AC call. Building the browser harness
here would be implementing #18 inside #58 and inverting the story's own ordering.

I am flagging this as a **scope boundary, not a deferral** — if @test or @operations judge
that (c) requires the browser-driven path in this loop, say so now and I will build it; I'd
rather hear that at planning than at Gate 3.

### Gate 1 Amendments (implementer response — supersedes the sections above where they conflict)

Every reviewer finding is ACCEPTED unless marked otherwise. Renames from the original plan:
subcommand `ts-no-retained-credentials`, module `crates/dt-guard/src/ts_retained_credentials.rs`,
wrapper `scripts/guards/simple/ts/no-retained-credentials.sh`, fixtures under
`crates/dt-guard/tests/fixtures/ts_retained_credentials/` (@paired-infrastructure-2 item 5,
@code-reviewer naming: name the concern, `ts-` prefixed).

**A-P1 — the matcher (closes @dry-reviewer P1). VERIFIED EMPIRICALLY, NOT REASONED.**

@dry-reviewer was right and my original claim was false. Confirmed by running the precedent
matcher: `\b(…|token|…)\b` returns **NO MATCH** on `userToken` (no word boundary between `r`
and `T`), so Rule 1 as originally specified was silent on the pre-fix `AuthResult` — the guard
would have shipped green against its own motivating case.

Mechanism adopted — @dry-reviewer's option (b) direction, refined one step further after
FP-testing their exact recommendation:

1. Normalize a field name into **segments** (split on `_`/non-word and at camelCase
   boundaries), lowercased. `userToken` → `[user, token]`; `access_token` → `[access, token]`.
2. A field matches a subset if any **contiguous segment run** equals the term's segments.
   Multi-word terms (`access_token`) therefore match `accessToken` with no separate entry.
3. `CATEGORY_A_ALLOWLIST` is honoured on the normalized join — so `token_type` **and** its
   camelCase spelling `tokenType` are both excluded by the one existing entry.
4. A declared `STEM_TERMS` set (currently exactly `cred`) additionally prefix-matches within a
   segment, so `credential` / `credentials` / `creds` match. Declared and tested, not implicit.

Why segment-equality rather than the plain suffix/`ends_with` I was handed: `ends_with` misses
`credential`/`credentials` (they don't end in `cred`), and the prefix variant that fixes them
false-positives on `tokenizer`. Segment matching gets both right. Verified results:

| field | cred | token | | field | cred | token |
|---|---|---|---|---|---|---|
| `password` | ✓ | — | | `token_type` | — | — (allowlist) |
| `userToken` | — | ✓ | | `tokenType` | — | — (allowlist) |
| `meetingToken` / `joinToken` / `bindingToken` | — | ✓ | | `tokenizer` | — | — |
| `accessToken` / `refreshToken` / `access_token` | — | ✓ | | `meetingCode` / `name` | — | — |
| `credential` / `credentials` / `creds` | ✓ | — | | `displayName` / `subdomain` | — | — |
| `clientSecret` / `passwd` / `pwd` | ✓ | — | | `jwt` | — | ✓ |

Rule 1 verdicts on every real type in `packages/**` — **pre-fix `AuthResult` fires; nothing
else does**: `AuthSession` (post-fix) ✗ · `LoginCredentials` ✗ · `RegisterCredentials` ✗ ·
`TokenCredentials` ✗ · `AuthTokenResponse` ✗ · `UserTokenCredentials` ✗ · `RegisterInput` ✗.
The zero-FP claim is now measured rather than asserted. These become table-driven unit tests
in the module so the matrix is a regression lock.

**A-P2 — fixture double-exemption (closes the Lead's blocking amendment + @paired-infrastructure-2
item 3 + @test F1).** The rules live in a pure kernel `scan_source(path: &Path, content: &str)
-> Vec<Hit>` that `run()` also calls, per the `cite_extract_e2e.rs` precedent. Fixture tests
drive the kernel directly, bypassing the exemption filter that would otherwise skip them
twice over. **Plus** a live `run-guards.sh` smoke at Gate 2 proving the wrapper is wired and
actually fails, since the kernel test cannot cover collection/filtering/exit-code. My original
plan cited the double exemption as a *safety* property; it is simultaneously the bug, and I
missed that.

**A-P3 — vocabulary partition (@dry-reviewer Call 1 + strengthened drift control).** Three
`pub(crate)` lists in `pii_vocabulary.rs` — credential / token / neither — with a unit test
asserting their union is **set-equal** to `PII_TOKENS_CATEGORY_A`, no fallthrough bucket.
Union-equality not member-count: a count test misses renames, and `contains()` fails silently
on one. Co-located with the catalog so a security reviewer adding a term is confronted with
the classification obligation at the point of edit.

**A-P4 — scanner hardening (@dry-reviewer Call 3).** Module-local declaration-block scanner,
inheriting the two hard-won properties of the existing depth-walkers: an **iteration/line cap**
that emits `common::scan::warn_skip` on trip rather than silently returning a truncated field
set, and **string/comment awareness** before depth counting. For `.svelte`, the scan is scoped
to the `<script>` block — markup `{}` template expressions would otherwise corrupt depth on
every component.

**A-P5 — filter shape (@dry-reviewer Item 2 condition + @operations F-OPS/budget).** Calls
`is_scan_exempt` FIRST, then layers only the genuinely-additive build-artifact entries.
`is_ts_build_artifact` extraction is TECH_DEBT → `docs/TODO.md`, not built here (Lead ruling).
Corrected measurement for the doc: **118** tracked `.ts`/`.svelte` under `packages/`, not "a
few hundred" (@operations).

**A-P6 — no bypass marker, by design (@operations F-OPS-2, taking their option (b)).** There is
no `guard:ignore` hatch for `.ts`/`.svelte` and I am not adding one. A suppressible
credential-retention guard is worse than a noisy one: the suppression would be applied by
whoever is inconvenienced, at the moment of inconvenience, with no security review. Rule 1 has
no legitimate exception (a retained type pairing a credential with a token is never correct)
and the measured zero-FP matrix above is what earns the right to have no hatch. Recorded in
the module doc AND in the runbook row as "the fix is to remove the field or stop retaining the
type; there is no bypass." If a genuine exception ever appears it gets a plan-level decision.
The runbook's generic "add `# guard:ignore(<reason>)`" line is **wrong for `.ts`/`.svelte`
today** for every existing TS guard too — corrected in the same edit.

**A-P7 — observability (@observability F1/F2/F3).** Accepted in full; this reverses my
"no catalog change" position, which was wrong.
- New bounded `failure_stage` value **`credential_invalid`** — not `signup` (a semantic lie)
  and not `internal` (which is the SDK-fault bucket; routing caller-input errors there makes
  it non-actionable).
- New `gcJoinFailureStage(err)` helper mapping `MeetingError` 401/403 → `credential_invalid`,
  mirroring the existing `signalingFailureStage` pattern in the same file. F3 is right that
  B1 materially shifts `gc_join`'s failure-mode mix: a caller-supplied token of arbitrary age
  makes credential rejection a routine `gc_join` outcome, where before the token was seconds
  old and SDK-minted. One new value covers both the local shape-reject and the GC reject.
- `docs/observability/metrics/client.md` gains the catalog bullet naming where token-mode
  credential failures land. Added to the classification table, owner observability.

**A-P8 — A1 check text (@semantic-guard SG-1..SG-4, @observability F4, @paired-client F6).**
New top-level `## Check: Client Credential Lifetime` (not a sub-item of Credential Leak — the
existing check is exfiltration-keyed and Rust-macro-shaped; a separate heading is what makes
the scope statement unambiguous), with an explicit cross-reference in both directions. It will
carry: a **procedure step** requiring cross-file resolution when a credential-bearing object is
passed to a callback/prop/store setter, judging the *destination's* lifetime (SG-1 — without
this the lens misses the actual defect, which is invisible on any single line); sub-check (ii)
reformulated as **scope-reachability** ("a credential included in an outbound request at a call
site where a token is reachable in lexical scope") rather than unevaluable principal identity
(SG-2); a **SAFE list** including the form-binding carve-out, call-scoped parameters,
credential-bearing parameter *type declarations* nobody stores, and request DTOs (SG-3 — and
explicitly written so the *absence* of `password = ''` is not itself a finding); **no fresh
word list**, naming `PII_TOKENS_CATEGORY_A` as the mechanical vocabulary and stating the lens
is deliberately not word-list-bound, plus **precedence** — the mechanical guard is a floor, the
lens is additive, the lens never green-lights what the guard fails (SG-4); the client's real
leak surfaces, which have no Rust analogue — metric labels, span attributes, `console.*`,
`Error.message`, `JSON.stringify`, and the DOM render path (@observability F4b); a statement of
whether the four existing checks apply to `.ts`/`.svelte` (@observability F4a); and the
bare-string form-state idiom both mechanical rules are structurally blind to
(@paired-client F6).

**A-P9 — semantic fixtures (@semantic-guard SG-6/SG-7).** Two added: `pos_resend_credentials_with_token_held.svelte`
(the pre-fix `credentials()` shape — the only fixture exercising sub-check (ii); SG-6 is right
that nothing I originally offered did) and `neg_form_input_binding.svelte` (the `SignIn.svelte`
shape — the FP trap for SG-3, provable rather than argued). SG-7: `.claude/agents/semantic-guard.md`
gets a carve-out making fixture files in scope when the Lead directs a fixture-verification
run — otherwise a Gate-3 "no findings" would be procedurally correct and vacuous.

**A-P10 — SSoT dereference (@semantic-guard SG-2 ruling + SG-5, @dry-reviewer Call 2).**
`.claude/agents/semantic-guard.md`: delete the line-7 parenthetical, keep `Checked:` as
concrete output listing the headings actually read. SG-5 found **7** live enumeration sites,
not 4. `CLAUDE.md`, `.claude/skills/devloop/SKILL.md` and `docs/decisions/adr-0024-agent-teams-workflow.md`
are the other three, which I routed to @team-lead rather than folding in.

**RULED 2026-07-30 by the project owner — dereference ALL SEVEN, with a refinement neither
option offered**: an illustrative list may REMAIN where it helps the reader, provided its
framing makes non-exhaustiveness explicit (`e.g.` / `such as` / `including`) rather than
reading as the definitive set. The failure mode being killed is a reader treating a stale
local copy as authoritative; an openly-partial list does not create it, a bare parenthetical
enumeration does. So this is applied **per site by judgment, not as a mechanical delete** —
some parentheticals are load-bearing for someone skimming the specialist table, and stripping
them to a bare pointer would cost more than it saves. Per site:

| Site | Treatment |
|------|-----------|
| `.claude/agents/semantic-guard.md:7` | delete the parenthetical — the sentence already names `checks.md` (SG-2 ruling) |
| `.claude/agents/semantic-guard.md` ×3 `Checked:` templates | keep as **concrete output** — the headings actually read this run, verbatim; NOT a pointer (SG-2: a failed/stale read must be visible in the verdict, not hidden) |
| `CLAUDE.md:50` specialist table | `e.g.`-framed short example + pointer — the row is skim-surface for someone choosing a specialist |
| `.claude/skills/devloop/SKILL.md:51` Team Composition row | `e.g.`-framed short example + pointer (the Lead's own suggested shape) |
| `docs/decisions/adr-0024-agent-teams-workflow.md:49` reviewer table | pointer — an ADR should not carry a mutable operational list |

This takes CLAUDE.md §Single source of truth's *derive one from the other* branch. @semantic-guard's
option (b) drift guard is NOT built here (a second new subcommand in a loop that already adds
one, and lower-value once only one list remains) — `docs/TODO.md` at Step 9, credited to
@semantic-guard.

**A-P11 — client findings (@paired-client F1-F7, @test F5/F6/F7/F10).** All accepted:
exhaustive `switch` + `never` in `#authenticate` (F1); (e) additionally pins `participantName`
on the decoded MC `JoinRequest` (F2 — the un-narrowed union access means a later
`displayName` omission ships nameless participants while (e) stays green); class-level
`@example` switches to the token path so the advertised default is the minimizing one (F3);
`validateUserToken`'s doc states it is a header-injection charset guard and explicitly **not**
JWT verification (F4); `App.svelte` derives the effective view so a forced `view` lands on
`create` rather than a blank `<main>` (F5, and @paired-client's derived-view form is better
than the total-`{:else}` shape I was going to write); `authViews.test.ts` re-points to
`userToken`, which has a live consumer (F7). Existing login/register unit cases stay — (e) is
additive, not a replacement, so the retained standalone path is not untested public API.
`exactOptionalPropertyTypes: true` means the conditional-spread idiom stays.
From @test: spy-wiring control test proving `mode:'register'` DOES record an AC request (F5);
(f) asserts nav **present** pre-auth then absent post-auth (F6); (e)'s negative is an
**allowlist** — no request to the AC origin at all, asserted on the recorded URL list, not an
enumerated per-endpoint denylist (F10); `afterEach(vi.unstubAllGlobals)` in `appShell.test.ts`.
`fetchImpl` spy over MSW stands, but on @test's reasons not mine — MSW *is* a dependency
(`msw@2.7.0`), the real arguments are tier consistency and that a recording spy directly
proves a negative where a handler-based mock cannot.

On @test F7 (is the B3 view-gate control tested?): the component tier cannot force `view` —
it is internal `$state`, and @paired-client is explicit that adding an `auth` or `view` prop
for testability would be production API for test convenience. What IS assertable, and what I
will assert: with `auth` set, **no password or email input exists anywhere in the shell**.
That tests the control's effect rather than its implementation. The stronger claim ("unreachable
even if `view` were forced") is review-verified only, and I hand that lens to @security rather
than implying test coverage.

**A-P12 — (c) (@test ruling + @operations ruling, both APPROVED the re-scope).** Conditions
accepted as REQUIRED, not conditional: `24_join_flow.rs` gets a doc-comment naming the
token-only property as load-bearing for #58 plus an explicit assertion (both reviewers
independently confirmed the property is currently **incidental** — the test asserts nothing
about not re-authenticating, so a future edit could break it green); the story is amended in
this commit to record the (c) decomposition, that #18 inherits the client half, and that
between #58 and #18 the browser join path has **no live-cluster coverage** — a real temporary
hole, recorded rather than discovered. No duplicate assertion against #18's existing
"no credentials at join time" check. Correction accepted: the env-test is **not** "always-run"
— Layer 7 cleanly skips in CI (`SKIPPED-NO-CLUSTER`); it runs on any local/container devloop
with a live helper.

**A-P13 — runbook (@operations F-OPS-1/F-OPS-4, @observability F5).** `docs/runbooks/devloop-validation.md`:
§6.3 row keyed on the strings an operator actually sees (`guards-failed` at the layer STATUS
line, discriminated by `FAILED: no-retained-credentials` + the `VIOLATION:` lines) rather than
a REASON token that never reaches the layer line; §8 row keyed on the two greppable rule IDs;
the full-tree semantics in oncall's terms ("this guard can fail your devloop for a violation
your diff did not introduce — standing invariant, not a diff lint"); the no-bypass statement
per A-P6; and §6.3.1's stale "Eight of the simple guards" count corrected (already wrong today
— the six `simple/ts/*` wrappers aren't in it).

**A-P14 — rollback order (@operations F-OPS-3).** Two separable commits: **(1) B1/B2/B3 client
fix, then (2) A1/A2 guard + fixtures + runbook.** Documented revert order: **A2 must be
reverted before or together with B2** — reverting the fix while the guard stands leaves the
tree uncommittable repo-wide. Added to §Rollback Procedure.

**A-P15 — @code-reviewer's upgrade condition, answered explicitly.** B1 does **not** change
which token type or scope MC accepts. MC's accepted credential is the GC-minted meeting token
(`joinResp.token`), untouched. GC's `joinMeeting` still requires a user JWT via `require_user_auth`,
untouched. B1 changes only **where the client obtains the user token it presents to GC** —
mint-a-fresh-one versus reuse-the-one-already-held. No server-side decision function, token
type, or scope is modified anywhere in the diff. I will state this in the commit message so the
condition is checkable without inference. `Approved-Cross-Boundary:` trailers to be collected
from @paired-client (packages/**), @operations (runbook), @semantic-guard (agent def).

**A-P16 — accepted, no argument.** `.tsx` claim now backed by a fixture (@test F9). (d) runs the
plain `git grep` as an independent cross-check alongside Rule 2, both recorded — Rule 2 alone
is circular for a one-time check (@test F4). Pre/post-B2 guard output pasted into the
Implementation Summary rather than asserted (@test F3). Every fixture asserted against **both**
rules — 2×6 matrix — with positives pinning rule_id + line and negatives pinning zero findings
(@test F2). Verification rows added for the password-unmount/zeroing test and the
`validateUserToken` CRLF case (@test F8). Module doc states the no-allowlist-needed property of
Rule 2 explicitly (@paired-client decision 2). Storage check: `localStorage`/`sessionStorage`
grep-verified absent from `packages/web-app`, recorded in the Implementation Summary
(@observability).

**Noted, not actioned in this loop**: `source_scan_patterns()` has three subset members pinned
by no test today (@dry-reviewer) — their TODO to route, not my changeset. ADR-0034's
subcommand-count re-debate trigger (Lead: TODO, do not address). `SignIn.svelte` cannot populate
`displayName` because AC's token response carries no identity fields — pre-existing, unchanged
by this work, follow-up (@paired-client).

### Gate 1 Amendments, round 2 (2026-07-30)

**A-P17 — Rule 1 gains the retention predicate (@code-reviewer F1, option (a)). VERIFIED.**
"Zero-FP by construction" was overclaimed and @code-reviewer is right that the difference from
"zero-FP measured today" is what a future author runs into. Their counter-examples all fire
under the retention-blind rule — confirmed by running them: `{resetToken, newPassword}` ✗,
`{currentPassword, csrfToken}` ✗, `{password, mfaToken}` ✗, `{clientSecret, accessToken}` ✗.
All four are legitimate transient DTOs; all four would have blocked the repo, with no bypass
per A-P6.

Rule 1 now requires **retained ∧ credential ∧ token**. This matches the task's own wording ("a
**retained** auth-result/session type carrying a `password` field alongside a token field"),
which the original rule had silently dropped. It still fires on the real defect — `AuthResult`
is retained at `App.svelte:14`.

Answering their follow-up rather than leaving two rules that look independent: **retained ∧
credential ∧ token is a strict subset of retained ∧ credential, so Rule 1 is now a message
refinement on a Rule 2 finding, not a second detector.** One detection pass; a finding that
also carries a token reports under the more specific rule ID and message. That will be stated
in the module doc.

Cost, stated rather than hidden: the retention-blind version caught a credential+token
declaration regardless of *how* it was stored. Retention-gating makes both rules depend on the
enumerated retention-idiom set, so a password+token type stored via a novel idiom now escapes
both. That residue joins the A1 lens's remit, and the module doc says so.

**A-P18 — Rule 2 alias/inheritance resolution (@security F-SEC-1). ACCEPTED — best catch in
round 2, and it lands exactly on the type this task introduces.** `JoinCredentials` is a union
alias with no fields of its own, so pass 1 would not have collected it, and
`$state<JoinCredentials>` — an *enumerated* retention idiom, not a novel one — would have
produced no finding while retaining an object carrying a password. @security is right that
this is not the residue I assigned to A1: it is squarely inside Rule 2's remit.

Fix: pass 1 resolves in-repo indirection to fixpoint — a type alias to a union/intersection
inherits credential-bearing-ness from any member; `interface X extends Y` inherits from `Y`.
Fixture `pos_retained_union_alias.ts`. Without this, the lens-4 criterion ("legal only where
the credential is a call-scoped argument no caller retains") was enforced by doc comment
rather than by the guard, which was the entire point of Rule 2.

**A-P19 — Rule 2 FP surface MEASURED (@operations item 1, @security F-SEC-2 condition).** Both
asked for this and both were right that my matrix only probed the token limb, which Rule 2
doesn't use. Measured across all 62 in-scope tracked files: exactly **three** sites annotate a
credential-carrying type, and the measurement produced a precision requirement I had not
written down.

| Site | Verdict | Why |
|---|---|---|
| `App.svelte:14` `let auth = $state<AuthResult\|undefined>` | **TRUE POSITIVE** — the real defect | goes quiet after B2 |
| `events.ts:59` `readonly credentials: JoinCredentials` | must NOT fire | member of the `JoinOptions` **interface** — a parameter type, not storage |
| `CreateMeeting.svelte:16` `auth: AuthResult` | must NOT fire | inside a `$props()` type annotation — props are transient |

So Rule 2 must fire on **1 site today, 0 after B2** — but only if the retention-site definition
**excludes interface members and type-literal members, including `$props()` annotations**. That
constraint was implicit in my head and absent from the plan; without it the guard FPs on two
sites immediately, and A-P18's alias resolution makes `events.ts:59` *more* likely to fire, not
less. Both exclusions become table-driven tests alongside the Rule 1 matrix.

**A-P20 — @dry-reviewer D1/D2 (Lead-verified).**
- **D1**: the open `STEM_TERMS` prefix reintroduced the failure I rejected `starts_with` for.
  `credit_card` is in CATEGORY_B, so `creditCard` → `[credit, card]` → `credit`.starts_with(`cred`)
  → fired as a credential, producing a BLOCKING finding whose rule ID misdescribes what it
  found. Replaced with a **declared stem→expansion set** (`cred` → `{cred, creds, credential,
  credentials}`), asserted by test. Verified: `creditCard` / `credit_card` / `creditLimit` /
  `credited` all silent; `cred` / `creds` / `credential` / `credentials` all fire; nothing else
  in the matrix moves. `creditCard` and `credit_card` added as explicit negative cases — the
  matrix is only as good as its adversarial inputs, and that one was missing.
- **D2**: comment-only correction at `pii_vocabulary.rs:72-76`. The `accessToken` entry's
  justification names `ts_pii` as its consumer, but `ts_pii` reads CATEGORY_**B**; CATEGORY_A
  has exactly three consumers and the entry is unreachable in all of them. Corrected to name
  the actual consumer (this guard, the first TS-side CATEGORY_A consumer) and note that under
  segment matching it is redundant with `access_token` but retained because the word-boundary
  consumers cannot normalize. **No vocabulary mutation** — CATEGORY_A changes need security
  sign-off and removing a live entry is bigger than this loop.

**A-P21 — `secret_patterns.rs` union-equality test (Lead ruling).** ~8 lines closing the
`source_scan_patterns()` silent-drop: `HYGIENE_SOURCE_SCAN_SUBSET.contains(name)` fails silently
on a rename, and three of its five members (`OpenAI/Stripe-style key`, `GitHub PAT`,
`Slack token`) are pinned by no test today. Same pattern I'm writing two files over. Table row
added, infrastructure-owned, Minor-judgment, @paired-infrastructure-2 ack requested.

**A-P22 — @security F-SEC-2 / F-SEC-3.**
- **F-SEC-2 (honesty)**: the claim is stated as **measured across the current tree**, not "by
  construction", in the module doc and the runbook. An overclaimed invariant is how the next
  person concludes a real FP must be a real finding. Note A-P17 makes the claim materially
  stronger anyway — the retention predicate collapses the DTO class rather than relying on
  today's file set.
- **F-SEC-3 (recovery path)**: ACCEPTED, ~10 LoC. B1 makes the retained `userToken` the sole
  join credential and B3 removes the auth nav once `auth` is set, so an expired token leaves
  the user with no affordance to re-authenticate and a known-dead bearer token held
  indefinitely. A-P7 is the evidence it is routine, not a corner. Fix: `JoinMeeting` surfaces
  an `onSessionExpired` callback on a `credential_invalid` / 401 join failure; `App.svelte`
  clears `auth`. The auth nav then returns automatically via the existing `{#if !auth}` gate —
  the UX recovery and the security-correct action (drop the dead credential from retained
  state) are the same edit. `CreateMeeting` is not dragged in: it surfaces its own error today
  and the session-clear path is join-scoped.
- **`validateUserToken` length bound**: a named constant sized for a real multi-KB JWT, NOT
  128 by analogy with `MAX_PASSWORD_LENGTH`. Per A-P6 there would be no way around a bad bound.
- **Zeroing claim**: held at "narrows the window", labelled defense-in-depth. It will not drift
  up to "removes the password from memory" — `password = ''` drops one reference; the original
  string survives until GC and may persist in the DOM input value and the `JSON.stringify`
  intermediate.

**A-P23 — @semantic-guard SG-9: the sink list moves out of the lifetime check.** They are
right, and the argument that decides it is theirs: their findings carry a `[check-name]` tag
that routes attribution, and tagging `console.log(session)` as `[client-credential-lifetime]`
would be simply wrong — nothing about lifetime was violated. Metric labels, span attributes,
`console.*`, `Error.message`, `JSON.stringify` and the DOM render path are **sinks**, i.e. the
client analogue of the existing `## Check: Credential Leak`, not of lifetime. Absorbing them
into the new heading spends the scope clarity that heading exists to buy — my own A-P8
argument, turned against my own structure.

So: the client sink surfaces extend **`## Check: Credential Leak`** as items 5+ (its 1-4 are
Rust-shaped; the heading name is already language-neutral). `## Check: Client Credential
Lifetime` stays retention + re-send only. Two names, two tags, clean attribution. @observability's
F4a document-scope statement moves to the `checks.md` **preamble**, next to the existing
"excluding test files" sentence — buried inside check five it is invisible to anyone reading
checks one through four, which is where the question arises. Their operability-budget point is
taken as a constraint, not a nice-to-have: a section I skim is worse than a shorter one I apply
correctly every time, and SG-1's cross-file procedure step plus SG-3's form carve-out are what
get lost first.

**A-P24 — @code-reviewer F2/F3/F4/F5 + upgrade, and @semantic-guard SG-10.**
- **F2**: the bare-string form-state blind spot (`let password = $state('')`, present twice
  in-tree) is named specifically in the module doc, plus a `neg_bare_string_form_state.svelte`
  fixture pinning that it does **not** fire — so a later author doesn't "fix" the gap and turn
  the guard into an FP generator on every login form. Good call: documenting the gap without
  pinning it invites exactly that.
- **F3/SG-10**: the "Not touched" paragraph is struck in place, with a note that the commitment
  was renegotiated in the open at Gate 1 rather than silently broken.
- **F4**: the seven stale `credential_lifetime` / `credential-lifetime` names are corrected
  throughout §Planning.
- **F5**: `CreateMeeting.svelte` added — Mechanical, owner client. It annotates `auth: AuthResult`
  but reads only `auth.userToken`, so it is a pure type-name rename that `tsc` fully covers.
  Layer A would have flagged it as unplanned; I missed it.
- **Upgrade ACCEPTED, no escalation**: the wrapper row goes Mechanical → Minor-judgment. §6.2
  conditions Mechanical on guard coverage of the change-pattern, and nothing in the pipeline
  verifies a wrapper is registered *and executable* — `run-guards.sh` gates on `[[ -x ]]`, so a
  missing exec bit is invisible. My `(new, mode 100755)` annotation was awareness, not coverage,
  and they are right that §6.2 asks for coverage.
- Their two deliberate non-upgrades (`lib.rs`, `main.rs` stay Mechanical — the compiler is a
  complete guard for a match-arm addition) are recorded so they aren't reopened.

**A-P25 — @test's snake_case corroboration.** They independently re-derived the matcher bug and
found it is worse than the case I reported: `\btoken\b` also fails on **`user_token`** (`_` is a
word character, so there is no boundary there either). Segment matching handles both — verified,
`user_token` → `[user, token]` ✓. Their ask is taken: the Implementation Summary will say
plainly that **the matcher was broken and reasoning had certified it; the pre-B2 run against the
real tree is what caught it.** That is the transferable lesson, and it is the argument that
keeps the (a′) discipline alive next loop.

**A-P26 — @operations' documented escape path.** "No bypass" is right for the in-code hatch, but
a dead end is not a procedure. The runbook row will say: there is no in-code bypass — the fix is
to remove the field or stop retaining the type; **if the guard itself is wrong, revert the guard
commit**, which is separable by design (A-P14) and ordered second precisely so this works. One
sentence, and it converts a dead end into a documented procedure.

### Gate 1 Amendments, round 3 (2026-07-30) — F1 resolution and closure

**A-P27 — F1 RESOLVED: option (b), one retention-gated detector, structural subset.** Taking
@dry-reviewer's framing over the Lead's original one, per the Lead's consolidation: two
independent predicates would implement "retention" **twice**, with nothing asserting the two
implementations agree — derived-state duplication, i.e. the exact failure class this guard
exists to detect, reproduced inside the guard.

Structural, not asserted-by-test (binding, per @dry-reviewer + Lead):

```
if retained { emit(if has_token { RULE_1_ID } else { RULE_2_ID }) }
```

The subset relationship holds **by construction** — it is impossible to emit a Rule 1 finding
for a declaration that did not satisfy retention. A test asserting the same thing could be
deleted, skipped, or amended; a structure cannot drift from itself. Same principle as the
fixture double-exemption fix and union-equality-over-count-test.

**F1 is a drift-from-spec catch, not a new requirement.** The task statement always said "a
**retained** auth-result/session type that carries a `password` field alongside a token field."
Retention was specified and dropped during design.

**A-P6 rewritten around the retention gate.** After the collapse there is one predicate and one
FP surface. The `{newPassword, resetToken}` / `{currentPassword, csrfToken}` /
`{password, mfaToken}` / `{clientSecret, accessToken}` class drops out **because it is not
retained** — semantics, not a snapshot of today's 118 files. That is a materially better
argument for no-hatch than the one A-P6 originally made.

**Declining @paired-client's token-limb narrowing, with reasons** (@code-reviewer's option 2;
they said any of their three closes F1). The semantic argument is genuinely good — a reset
token does not make a password unnecessary, it is the capability to *set* one — but under the
retention gate the transient-DTO class is already gone, so narrowing buys only the retained
password-reset *form state* case while adding a second matching mechanism and walking into the
`userToken` trap @code-reviewer identified: `userToken` matches **only via bare `token`**
(there is no `userToken`/`user_token` entry in CATEGORY_A), so dropping bare `token` from the
limb would make Rule 1 go silent on the motivating defect *again*. Catching the A-P1 failure
twice in one guard is not a risk worth taking for a shape this codebase does not use (it uses
separate primitives for form fields). Recorded as @code-reviewer's Gate-3 watch item: if a
retained `ResetForm { newPassword, resetToken }` ever appears, A-P6's plan-level-decision
path plus the commit-revert path (A-P26) are the known routes, not a surprise.

**A-P28 — @security's closure question: member-of IS included, and re-measuring changed the
answer.** Their preference, and it is right: handling the anonymous inline case
(`readonly creds: { password: string }`, already committed in A-P2) but not the named one
(`readonly creds: LoginCredentials`) would be an inconsistency whose workaround is a one-word
edit — extract the inline type to a name and the guard goes quiet.

They also predicted the consequence, and it landed: **the FP re-measurement post-closure
surfaced a fourth site and a new exclusion requirement.** Closed set becomes
`AuthResult, LoginCredentials, RegisterCredentials, RegisterInput, LoginInput` (seed) +
`JoinCredentials` (union alias) + `JoinOptions` (member-of). Re-measured:

| Site | Verdict | Why |
|---|---|---|
| `App.svelte:14` `$state<AuthResult\|undefined>` | **TRUE POSITIVE** | goes quiet after B2 |
| `events.ts:59` `readonly credentials: JoinCredentials` | must NOT fire | interface member |
| `CreateMeeting.svelte:16` `auth: AuthResult` | must NOT fire | `$props()` type literal |
| **`MeetingSession.ts:335` `options: JoinOptions,`** | **must NOT fire — NEW** | **function parameter in a multi-line signature** |

The fourth is new and only exists because of the closure. It means the exclusion set must
cover **function-parameter declarations in multi-line signatures**, not just interface and
type-literal members — a parameter spread across lines is lexically indistinguishable from a
field declaration without tracking the enclosing construct. Had the closure landed without
re-measuring, the guard would have red-lined `MeetingSession.join`'s own signature. All four
rows become table-driven tests; per @operations, the three exclusions are anchored on **these
real call sites**, not synthetic equivalents, because a future refactor breaks real sites.

**A-P29 — stem expansion converged (@paired-infrastructure-2 Item 1 ≡ @dry-reviewer D1).** One
fix: `STEM_EXPANSIONS: &[(&str, &[&str])] = &[("cred", &["cred","creds","credential","credentials"])]`
in `pii_vocabulary.rs`, matched by the **same segment-equality primitive** as every other term.
No prefix path — two matching primitives collapse to one, and the matched set becomes finite
and auditable rather than open-ended over English.

Three constraints, all adopted:
- **Expansions stay OUT of the credential/token/neither partition lists** (@paired-infrastructure-2).
  A-P3's union-equality test asserts the partition equals `PII_TOKENS_CATEGORY_A`; putting the
  expansion spellings in a partition list would break set-equality unless they were also added
  to CATEGORY_A — reintroducing the blast radius across `instrument_skip_all` /
  `rust_log_secrets` / `metric_labels` that the stem mechanism exists to avoid. The two
  invariants collide if the expansion lives inside the partition.
- **Referential-integrity test**: every KEY in `STEM_EXPANSIONS` must be a member of
  `PII_TOKENS_CATEGORY_A` (@paired-infrastructure-2, @dry-reviewer). Without it a rename of
  `cred` silently orphans the expansion — key matches nothing, `credential`/`credentials`
  quietly stop being detected, all tests still pass.
- **No CATEGORY_A mutation, so no security sign-off needed** — @paired-infrastructure-2
  withdrew their own additions proposal on realising the blast radius; the Lead has retracted
  the sign-off instruction.

**@dry-reviewer's correction, recorded so the convergence does not paper over it**: D1 and the
vocabulary-completeness finding are **not** the same defect. D1 is about *this guard's matcher*
over-matching `creditCard`; theirs is about *the catalog* — `\bcred\b` never matches
`credential` in the three existing **Rust** consumers, which consume CATEGORY_A directly and
will never see `STEM_EXPANSIONS`. Converging the fix closes mine and leaves theirs open. It is
routed as a follow-up with real blast radius (promoting `credential`/`credentials`/`creds` to
full CATEGORY_A entries changes detection in three shipped guards and needs security sign-off)
— **not** closed by this loop, and the amendment says so.

**A-P30 — SG-11: one rule with a boundary, not two bullets.** @semantic-guard is right that
@paired-client's F6 ("the lens owns the bare-string form-state idiom") and SG-3 ("the form
binding is SAFE") read as opposite verdicts on the same three tokens, and that a reader
resolving them wrongly fails in *both* directions — flag `SignIn.svelte` and trip
`neg_form_input_binding.svelte` (the FP-generator outcome the fixture exists to prevent), or
skip bare strings as a class and miss a password propagated upward through `onAuthed` (the
original defect, one type annotation away).

Reconciliation, stated as a single rule: **a bare-string credential in the collecting view is
SAFE; it becomes a finding at the point it crosses out of that view (callback, prop, store,
parent state, persistence) or is read after a token exists.** F6 hands the lens the *dangerous
half*, not the whole idiom. This reuses SG-1's cross-file boundary rather than introducing a
concept, so it costs no length — and one rule with a boundary is shorter than two bullets that
contradict, which serves the operability budget.

**A-P31 — remaining table rows** (@paired-client Row 2 via @code-reviewer, @paired-infrastructure-2,
Lead ruling): `packages/sdk-core/src/index.ts` (add `TokenCredentials` to the re-export list —
Mechanical, client; noting @code-reviewer's point that **no guard covers this change-pattern**:
`ts-exports-map-closed` validates `package.json` maps, not `index.ts` re-export completeness,
and `tsc` won't catch the omission because in-repo code reaches the type by direct path — which
is exactly why the row must exist rather than be assumed);
`docs/specialist-knowledge/infrastructure/INDEX.md` (Mechanical, infrastructure — a guard not
in the INDEX does not exist to the next infrastructure agent); `docs/TODO.md` (Mechanical,
owner team-lead — additive entries only; any modification or closure of an existing entry
stops being Mechanical and returns to the Lead).

**A-P32 — the loop's own pattern, for Lessons Learned.** @semantic-guard counted **five**
instances in this loop of *coverage that reads as real and isn't*: the `\btoken\b` non-match,
fixtures exercising the wrong sub-check, fixtures inside the exempted directory, the fixture
double-exemption, and the union-alias blindness. Their generalisation is the transferable
output: every one is a **negative** claim ("nothing was retained", "no credential leaked", "no
AC call was made"), and negative claims fail silently by construction — a broken detector and a
clean tree are indistinguishable from the outside. @operations, @test and @paired-client
independently arrived at the same control: **run it against the real tree**; on three separate
occasions here reasoning had already certified the code as correct. @dry-reviewer adds the
sharpest corollary — a probe matrix generated *from* the vocabulary can only confirm the terms
match; it cannot find the words you never thought to include, so probes must be generated
*against* the matcher adversarially. All four go in §Lessons Learned as guard-authoring
practice, not as an incident report.

### Gate 1 Amendments, round 4 (2026-07-30)

**A-P33 — F11 (@test, confirmed by @paired-infrastructure-2): the kernel seam cannot express
Rule 2, and the fixture suite would have passed vacuously.** A-P2 specified
`scan_source(path, content)` — single file in — while A-P18's pass 1 is **repo-wide**. A
per-file kernel scanning `App.svelte` in isolation cannot know `AuthResult` carries a
`password`; that fact lives in `types.ts`. Driving the fixtures through it hands Rule 2 an
empty declaration index, so **every Rule 2 assertion returns silent — which is exactly what a
negative fixture asserts.** The 2×N matrix goes fully green with Rule 2 structurally disabled.
Worse than the double-exemption bug it descends from: that produced a scan of zero files; this
produces a *passing matrix that reads as coverage*.

Seam becomes two-phase, `run()` composing them:
`collect_credential_types(&[(PathBuf, String)]) -> DeclIndex` (pass 1, incl. the A-P18 fixpoint)
+ `scan_retention(path, content, &DeclIndex) -> Vec<Hit>` (pass 2). Fixture tests drive the pair
over the fixture set **as a set**. A-P2's actual property — fixtures reaching the kernel without
passing through `is_scan_exempt` — is fully preserved; it only ever needed a set-shaped entry
point. @paired-infrastructure-2 owns the correction: they mapped the `cite_extract_e2e.rs`
precedent (genuinely per-file) onto a rule that was repo-wide in the original plan text.

**A-P34 — verification item (a)'s fixture was invalidated by A-P17, and the cross-file pair
fixes both.** @paired-infrastructure-2 caught that `pos_auth_result_with_password.ts` was
written when Rule 1 was retention-**blind**; post-A-P17 a bare `interface` has no retention
site (A-P19 excludes interface members), so **it fires nothing — the "positive" fixture is a
negative.** That is literally verification item (a), the Lead's blocking amendment and the
load-bearing evidence for the whole (A) half.

Both reviewers converged on the same fix from different directions, and @paired-infrastructure-2
explicitly preferred @test's over their own patch: a genuinely **cross-file pair** —
`pos_xfile_decl.ts` (declaration only; asserts **silent on its own**) + `pos_xfile_retention.svelte`
(retention site annotated with that type; asserts Rule 2 fires **only when scanned as a set**).
That reproduces the shipped defect's actual shape — declared in `types.ts`, retained in
`App.svelte` — which no self-contained fixture covers, and the declaration-alone-is-silent
assertion is a free precision check. `neg_` counterpart is the same pair with `password`
removed and the retention site intact, so it stays a controlled experiment isolating the
credential field. `pos_retained_union_alias.ts` is likewise split, so it tests fixpoint
resolution *through* the cross-file path rather than beside it. `pos_nested_credential.tsx`
audited for the same defect.

Why this matters beyond tidiness: (a′) covers the cross-file case **once**, and B2 then deletes
the evidence. After that the fixtures are the only regression lock, and as specced the lock did
not cover the shipped defect's shape.

Also per @test: the matrix cell "Rule 1 fires, Rule 2 silent" is now **structurally impossible**
(A-P27's emit-from-inside-the-retention-branch), so it is asserted as impossible rather than
carried as a case that happens to be empty — a test asserting it cannot happen documents the
invariant; a missing case documents nothing.

**A-P35 — declining the token-limb narrowing STANDS, and the ground shifted under it twice.**
@code-reviewer corrected their own endorsement (Option A is a no-op: under segment matching
`resetToken` → `[reset, token]` reaches the limb by the same bare-`token` path as `userToken`,
so no rule keeps one and drops the other). @paired-client then withdrew Option B's supporting
argument after @code-reviewer found it rested on a false premise: **`ts_pii` reads CATEGORY_B
only** (`ts_pii.rs:19`), so adding `userToken` to CATEGORY_A would not make `ts_pii` detect it.
CATEGORY_A's consumers are three Rust-side guards where a camelCase TS identifier is inert.

So Option B now buys a security sign-off plus a widening that does nothing in any consumer that
can see it — and under the retention gate the transient-DTO class it targeted is already gone.
Both reviewers now agree the plan's current position (@code-reviewer's option 3) is defensible;
@security backed the decline independently on the grounds that re-introducing the A-P1 trap in
exchange for a shape this codebase does not have is a bad trade. The retained-`ResetForm` case
remains @code-reviewer's Gate-3 watch item with the plan-level and commit-revert paths named.

**A-P36 — @semantic-guard SG-12: TWO gates in A1, not one. The F1 collapse must not be mirrored
into the prose.** Retention-as-single-gate is right for A2 **because it is the mechanical
layer's ceiling**, not because retention is the true predicate. Sub-check (ii) is a
*transmission* event, not a retention finding: a view that collects a password into a bare
local, posts it while already holding a valid token, and stores it nowhere satisfies (ii)
completely with zero retention — and the task statement names that case explicitly. Copying
A2's gate into A1 would import the mechanical layer's blind spot as if it were a design
decision, inverting the whole A1/A2 split.

So the prose carries **two gates**: *retention* (a credential reachable after the exchange,
token-presence refining it) and *transmission-while-token-held* (SG-2's scope-reachability
formulation — the **whole** predicate of (ii), no mechanical counterpart). Stated plainly that
A2 implements the retention gate only, so prose and guard describe the same thing without
pretending the guard reaches as far as the lens.

Taking their optional fixture too, because their reasoning is this loop's own pattern applied
to the check text: `pos_resend_no_retention` — password in a bare local, posted while a token
is held, stored nowhere — isolates (ii) from (i). Without it, the only (ii) fixture is the real
pre-fix shape, which satisfies **both**, so (ii) would read as covered while being tested only
through (i). They offered to accept prose alone and report the caveat; a seventh fixture is
cheaper than an accurate-but-weak Gate-3 record.

**A-P37 — @paired-client's three `onSessionInvalid` rulings + Lead ruling.**
- **(a) callback prop — yes**, matching the established `onAuthed` / `onGoJoin` idiom; a
  store/context is new machinery for one call site. **Renamed `onSessionExpired` →
  `onSessionInvalid`**: a 401 covers revoked and malformed tokens, not just expiry, and
  @observability's stage for the same condition is `credential_invalid`. Two words for one
  condition is a vocabulary seam for whoever correlates the metric to the code.
- **(b) I was wrong; `CreateMeeting` is wired too.** Lead ruled and @paired-client is right:
  `CreateMeeting.svelte:37` passes the same `auth.userToken`, so the identical 401 lands there
  and B3 has removed the nav either way — a user who hits create first is equally stranded. The
  truth is **credential-shaped, not view-shaped: any 401 from a `userToken`-bearing call proves
  the retained credential is dead.** Scoping the clear to join means knowingly holding a
  credential already known invalid, which is this task's own subject. ~3 LoC in a file already
  in the changeset; "the task didn't ask for the other instance" is the framing-lock
  anti-pattern, not a deferral reason.
- **(c) BLOCKING, and they are right — my reading was wrong.** F5's derived view redirects one
  direction only (`signup`/`signin` → `create` when authed). Clearing `auth` while
  `view === 'join'` leaves `effectiveView === 'join'`, the join branch requires `auth`, every
  branch falls through → **empty `<main>`**: F5's own failure mode reintroduced through the
  recovery path I was adding. Fix is bidirectional:

```ts
const effectiveView = $derived(
  auth
    ? (view === 'signup' || view === 'signin' ? 'create' : view)
    : (view === 'create' || view === 'join' ? 'signin' : view)
);
```

  `&& auth` guards stay on the create/join branches — unreachable but necessary for TS
  narrowing of the `{auth}` prop.

**Test rigor (@test + Lead)**: the assertion must drive a **real 401 through the stubbed
`fetch` at join time** — calling `onSessionInvalid` directly would assert that a callback
clears state, not that a 401 invokes it, and wiring the callback to nothing would still pass.
Both triggers (401 and `credential_invalid`) covered, or one pinned and the other named as
unverified. Assert the join failure surfaced, so a test that never reached the join cannot read
as a passing recovery test. And per @paired-client + Lead: assert the Sign-in **view** renders
(`signin-button` / `email` testids), not just the nav button — "the nav returns" passes with a
blank `<main>`, which is the same false-green shape as their F2.

**Scope growth recorded, not naturalised** (@test): `onSessionInvalid` + the `auth` clear is
new production behaviour added after plan confirmation, driven by @security's F-SEC-3. The
Implementation Summary says so with the driver named, so a later reader sees that **B3 as
originally specced was incomplete** rather than assuming this was always planned.

**A-P38 — @operations: the FP matrix is a standing obligation, not a landing measurement.**
Their point is that the matrix decays and nothing notices: the fourth site appeared because a
closure change moved the FP surface underneath a count that had been correct an hour earlier,
and it was caught only because I re-measured rather than trusting it. The surface moves without
this guard's code changing — extending `PII_TOKENS_CATEGORY_A` (shared, other owners), adding a
retention idiom, or any change to alias closure. So: one line in the module doc and one in the
runbook row requiring the matrix be **re-measured against the real tree whenever the vocabulary,
the retention-idiom set, or the alias closure changes**, anchored on the four real sites rather
than synthetic equivalents. That converts a control I arrived at empirically into one the next
author inherits.

**A-P39 — the systemic finding is THREE modes, not one root cause.** @dry-reviewer corrected
their own "three instances, one root cause" framing, and @paired-infrastructure-2 added a third.
Recording all three, because a follow-up scoped from the one-root-cause version closes Mode A,
leaves B and C open, and **looks closed** — this loop's own failure shape appearing in the
description of a fix rather than in a fix:
- **Mode A — referential integrity under rename.** A structure references a string that must
  exist elsewhere; rename the target and the reference silently orphans. Instances:
  `HYGIENE_SOURCE_SCAN_SUBSET`, the new `STEM_EXPANSIONS` keys. Remedy: membership test (this
  loop closes both).
- **Mode B — matcher-capability mismatch.** Entry well-formed and correctly referenced, but the
  consumer's matcher cannot reach it or what it was meant to reach. Remedy: per-consumer
  **self-match reachability** test — a membership test passes and reveals nothing. Not this loop.
- **Mode C — absence masked by naming** (@paired-infrastructure-2). The term is not in the
  catalog at all while the vocabulary's naming implies it is: `cred` reads as covering
  credential-ish identifiers and covers exactly one spelling. Neither orphaning nor
  unreachability.

**Two corrections inside Mode B, both reviewers correcting themselves**: `accessToken` is dead
**only in `metric_labels`** (which lowercases first, so it cannot see *any* camelCase catalog
entry — a consumer defect, and the next camelCase entry dies there on arrival too); it is
reachable in `rust_log_secrets` and `instrument_skip_all`, whose `\b(alternation)\b` runs
against raw line text and so matches literals, doc comments and `#[serde(rename = "accessToken")]`.
So D2's framing is "one entry, three consumers, two matchers, reachable in two, dead in one."
The part that survives intact: **this guard is the first consumer that normalizes, which is why
several of these surfaced at once rather than as unrelated bugs.**

**A-P40 — follow-ups routed with evidence, not assertions** (@dry-reviewer owns routing):
- **CATEGORY_A promotion of `credential`/`credentials`/`creds`** — @paired-infrastructure-2
  measured it: the terms appear as identifiers in **15 Rust production files**, concentrated in
  `crates/ac-service/src/handlers/admin_handler.rs` (service-credential management — precisely
  where a credential-leak guard should have reach and has none). But `admin_handler.rs:754`
  logs `credential_id = %credential.credential_id`, and `rust_log_secrets`'s `[%?]\s*WORD\b`
  branch would match `%credential` (the `.` is a non-word char, so the boundary holds) — an
  immediate FP on a line emitting a credential's **identifier**, not its secret, in a file with
  many sibling call sites. So the follow-up is: add terms → work the FP surface (allowlist
  entries or a shape distinguishing `%credential.field` from `%credential`) → before/after runs
  on all three consumers → security sign-off. Verification surface and pipeline-blocking risk,
  not LoC — **genuinely task-sized, and the deferral clears the burden of proof on its merits
  rather than on scope framing.**
- **`pii_vocabulary.rs:68-73` comment** — has now misled **two** reviewers in one loop. Entry
  names the three possibilities (comment wrong / entry miscategorised / `ts_pii` meant to read
  both) so the next person doesn't re-derive them.
- **`ts_pii` has no secret limb at all** (@paired-client): it reads CATEGORY_B only, which is
  `email`/`phone`/`ssn`-shaped end to end — so `ts_pii` cannot detect **any** secret identifier
  in TypeScript. Combined with `no-secrets-in-ts` catching only literal *value shapes*, the
  uncovered case is `console.log('t', userToken)`: identifier unchecked (no secret vocabulary),
  value unchecked (runtime variable, not a literal). Same shape as this task's own thesis about
  client guard coverage. **Not a live leak** — R-23 hygiene is documented and there is an
  existing test asserting no join logs are emitted — but it is task-sized and belongs in the
  guard tree, not `packages/**`.

**A-P41 — Lessons Learned framing, per the Lead.** Not a personal note: **it is not only an
implementer failure mode, and the asymmetry is what makes it dangerous.** Reasoning produces a
confident claim at near-zero cost; execution produces a correct one at slightly-above-zero
cost. Everything in this loop that reasoning got wrong, running got right in under a minute.
Four instances across four roles: mine (three design-changing findings), the Lead's (recorded
the ADR-0034 sprawl trigger as unfiled without running the grep that showed it was tracked and
deliberate), @paired-infrastructure-2's (raised that premise; also mapped a per-file precedent
onto a repo-wide rule), @dry-reviewer's (cited a file relocation that never happened; also
over-broad on `accessToken` reachability). Plus @semantic-guard's mechanism — six-plus
instances, all **negative** claims, which fail silently by construction — and @dry-reviewer's
corollary that probes generated *from* a vocabulary can only confirm the terms match.


### Gate 1 Amendments, round 5 (2026-07-30) — post-approval, folded in before coding

**A-P42 — @dry-reviewer: pass 1 MUST use `common::git_changes::get_tracked_files`.** The
single-file seam used `get_all_changed_files`, which returns only *changed* files; the
repo-wide index needs `get_tracked_files(root, "packages/*", &[".ts",".tsx",".svelte"])`. A
direct `Command::new("git")` here is a `common/` helper reimplemented — **BLOCKER-class under
ADR-0019**, not an extraction opportunity. `WalkDir` is not cover: the 8+ modules using it all
walk a specific non-git directory, never repo-wide source. Also taken: **read each file once**
(phase 2 evaluates against the index rather than re-reading, so two read paths cannot disagree),
and **the exclusion filter applies to phase 1** — otherwise Rule 2 indexes fixtures and test
files Rule 1 correctly skips, and the two rules disagree about scope, which is the derived-state
divergence the structural subset exists to prevent.

**A-P43 — @operations: the fixpoint needs a visited set.** TS permits mutually recursive types
(`interface A { next: B }` / `interface B { next: A }`); a closure without cycle detection does
not terminate. No cycle exists today (the credential-adjacent graph is one hop), so this is
hardening. It is worth doing because the failure shape is nastier than an FP: no `VIOLATION:`
line, just `STATUS=FAIL REASON=guard-timeout-…` after 30s on every devloop repo-wide, presenting
as a *performance* problem so triage goes to file counts rather than to a recursive type — added
by someone with no connection to credentials, who gets no signal. Visited set + a
mutually-recursive fixture + a runbook line under the timeout row ("suspect a cyclic type alias
before scan volume").

**A-P44 — @semantic-guard SG-13: lens-only fixtures need the layer in the path.** A2 implements
the retention gate only, so `pos_resend_credentials_with_token_held.svelte` and
`pos_resend_no_retention` are fixtures the mechanical guard must be **silent** on — the second
by construction, since it is specified as *stored nowhere*. Under A-P16's `pos_`/`neg_` matrix
those either fail or need a harness exemption list, and an exemption list is the thing A-P6
refused to build for the guard itself (same reasoning: extended by whoever is inconvenienced).
Worse, it leaves an invitation to "fix" the guard so `pos_*` fires — which means retention-free
transmission detection in a syntax matcher, the A1/A2 inversion SG-12 just closed, arriving from
the other end with a green test as justification. Fix: lens fixtures live in
`fixtures/ts_retained_credentials/lens/`, the mechanical matrix is scoped to the mechanical set,
and the lens set carries **no `cargo test` assertion** — its executor is the Gate-3 report. The
property required: **a file's path says which layer is expected to fire**, so silence is never
ambiguous between "correctly quiet" and "detector disabled."

**A-P45 — @test F12/F13.** F12: `onSessionInvalid` is wired at **two** call sites (join and
create), so assert **both** entry points, or assert join and state plainly in the Implementation
Summary that create is wired-but-unasserted. Their point stands — the create path is the more
likely to rot because it is the less-travelled flow and nothing else exercises it. Asserting
both, since the stub and recovery assertion are already built. F13: the (f) states land as
**independent `test()` blocks**, each arranging its own precondition, not one sequential walk —
otherwise a failure at state 2 hides 3-5, the message names the file rather than the behaviour,
and the states become order-coupled.

**A-P46 — `pii_vocabulary.rs` edits are wider than the partition** (@paired-client + @code-reviewer's
verified consumer map). A2 becomes **CATEGORY_A's first TypeScript consumer, in this commit** —
every existing consumer is Rust-side (`rust_log_secrets`, `instrument_skip_all` = A only;
`rust_pii`, `ts_pii` = B only; `metric_labels` = **both**, the sole dual consumer). So:
- The module-doc consumer map (lines 6-15) goes stale in the same commit that repairs `:68-73`;
  `credential_lifetime` is added to the secret-identifier group, marked as the TS-side one.
- `:68-73` is repaired as neither "wrong" nor "dead": *added for camelCase detection under
  word-boundary matchers which cannot reach it — CATEGORY_A had no TS consumer until this
  module; under segment matching the entry is redundant, and it remains inert in the three
  Rust-side consumers.* **The entry stays** — it is redundant only under A2's matcher, and
  removing it would be right for the new consumer and wrong for the existing ones, which is the
  mirror of the mistake the comment records.
- @code-reviewer's correction to their own count: a `userToken` addition would have widened
  **three** Rust consumers, not two. Conclusion unchanged (all inert for a camelCase TS name).

**A-P47 — `docs/TODO.md` entry for the `ts_pii` secret limb**, taken verbatim from
@code-reviewer's specification, including the two load-bearing parts: the **severity line**
(coverage gap, not a live leak — without it someone triages this as a credential leak and
escalates) and @paired-client's **"fix the comment is the one resolution that is certainly
wrong"** diagnostic, which preserves a sign-off for a mechanism that never existed.

**A-P48 — the failure class, in @code-reviewer's stronger form.** Three instances in this loop
of *the check passes while proving nothing*: A-P1 (matcher silent on `userToken`), F11 (per-file
seam, Rule 2 structurally disabled, matrix green), and the stale positive fixture (retention
gate invalidated a fixture written against the retention-blind rule — verification item (a)
unmet). In each case **a fixture existed and passed**, so "add fixtures" is not the lesson. The
durable form: **for any new detector, assert it fires on the real pre-fix artifact, not only on
a fixture.** That single check would have caught all three. @code-reviewer owns instance 3 as a
downstream consequence of F1 and asked for a module-doc line so the next person amending a
predicate re-derives the fixtures rather than assuming they still bind. @semantic-guard's
sharper version of F11 specifically: the earlier failures produced *wrong output you could look
at*; F11 produced *no output while every assertion passed* — catchable only by asking what the
result would look like if the thing under test were disabled, and noticing it is the same
picture.

### Out of scope (restating the task's own list)
AC-side token-exchange redesign; backend Rust credential-leak checks; any broader
auth-session-management refactor beyond what B1/B2 require.

---

## SG-5 RULING — APPROVED by user (project owner), 2026-07-30

@semantic-guard found the semantic-check enumeration duplicated in **7** locations, three of
them governance files the implementer correctly refused to touch unilaterally (`CLAUDE.md`,
`.claude/skills/devloop/SKILL.md`, `docs/decisions/adr-0024-agent-teams-workflow.md`).

**Ruling (user's words): remove the duplicate semantic-guard information and replace it with
pointers to the single authoritative source. Examples are OK but must be clearly framed as
examples, not complete lists.**

This is option (a) — dereference all seven — plus a refinement neither option offered: an
illustrative list may REMAIN where it aids the reader, provided its framing makes
non-exhaustiveness explicit (`e.g.`, `such as`, `including`) rather than reading as the
definitive set. The failure mode being eliminated is a reader treating a stale local copy as
authoritative; a list openly marked as partial does not create that failure mode.

`scripts/guards/semantic/checks.md` is the single source of truth. Applies to all seven
locations, `.claude/agents/semantic-guard.md` included.

Rationale is CLAUDE.md §Working Conventions §Single source of truth ("When two places encode
the same value, they will drift. Derive one from the other, or add a guard that fails
validation on drift") — this ruling takes the *derive one from the other* branch.
@semantic-guard's option (b) drift guard is NOT built in this loop (it would be a second new
dt-guard subcommand in a loop that already adds one) and is recorded as a TODO follow-up
below — **rescoped, per their pushback, from a full parse-and-compare to a much narrower
guard**: assert the pointer path resolves, and assert every check name appearing in an `e.g.`
list matches a `## Check:` heading in `checks.md`. Existence check + subset check; no canonical
array, no five-location mirror comment.

**My original rationale for deferring was partly wrong and is corrected here.** I wrote that
after the dereference "there is only one list left to drift from." That understates the
residual, in two ways @semantic-guard identified:

1. **The risk changes shape rather than shrinking — content drift becomes path rot.** Every
   pointer names one path; if `checks.md` moves, all of them break at once and silently (the
   docs still read fine, they just point at nothing).
2. **Non-exhaustive framing closes under-inclusion, not over-inclusion.** `e.g.` protects
   against a list *missing* a check. It does nothing about a list *naming* a check that was
   since renamed or removed — that stays syntactically honest while pointing at nothing, and a
   reader cannot tell.

**Evidence check (Lead, verified rather than accepted).** @semantic-guard cited
`docs/devloop-outputs/2026-05-14-semantic-guard-relocation-task40/` as proof this file has been
relocated before. The devloop exists, but `git log --follow --diff-filter=AR --
scripts/guards/semantic/checks.md` returns only the commit that created it: task #40 relocated
the semantic-guard's *role* (Layer-3 shell guard → Gate-2 reviewer panel), not the file.
`checks.md` has never moved, so the precedent claim is withdrawn.

The conclusion survives on stronger, present-tense evidence found while checking: the *pointer*
surface is wider than the 7 *enumeration* sites SG-5 addresses, and includes sites outside the
changeset — `AI_DEVELOPMENT.md:173` (a markdown link, so a move breaks link and text together)
and `docs/specialist-knowledge/semantic-guard/INDEX.md:6` and `:7`. After this loop roughly ten
references name one path and none is asserted. That argument is checkable by grep today and
does not rest on history.

**Consequence for the classification table**: the three governance rows are now live at
Minor-judgment, owner `team-lead`. Per ADR-0024 §6.3 that requires owner confirmation at
Gate 1 and Gate 3 — I am the owner and this section IS the Gate 1 confirmation. I will
re-confirm at Gate 3 against the actual hunks.

---

## PRE-B2 EVIDENCE (captured 2026-07-30, BEFORE any `packages/**` edit)

This is the loop's only non-circular evidence and it is **destroyed the moment B2
removes the field**, so it is recorded verbatim here at capture time rather than
reconstructed later. Tree state: `HEAD = 72ca36c`, `git status --porcelain packages/`
= 0 files — the client tree is untouched.

```
$ ./scripts/guards/simple/ts/no-retained-credentials.sh
VIOLATION: packages/web-app/src/App.svelte:14 [auth_state_password_with_token] retained type `AuthResult` carries a credential field alongside a session token (the credential is provably unnecessary — you already hold a token). Fix: remove the field, or stop retaining the type. There is no bypass marker; if the guard itself is wrong, revert the guard commit.
ts-retained-credentials-violation-found-1-of-62-files
STATUS=FAIL REASON=ts-retained-credentials-violation-found-1-of-62-files
EXIT=1

$ dt-guard ts-no-retained-credentials --root /work --explain
EXPLAIN: packages/web-app/src/App.svelte:14:1 policy=ts-no-retained-credentials::auth_state_password_with_token pattern=auth_state_password_with_token src=crates/dt-guard/src/ts_retained_credentials.rs:727
STATUS=FAIL REASON=ts-retained-credentials-violation-found-1-of-62-files
EXIT=1
```

**EXACTLY ONE hit — not "at least one"** (@paired-client's requirement). The exclusions
are doing real work rather than being over-broad, and an over-broad parameter exclusion
would have shown as zero hits, which reads identically to "clean" under a lower-bound
assertion. All four measured sites behave as predicted in A-P19/A-P28:

| Site | Predicted | Actual |
|---|---|---|
| `App.svelte:14` `$state<AuthResult\|undefined>` | fires, refined rule | ✅ fires, `auth_state_password_with_token` |
| `events.ts:59` `readonly credentials: JoinCredentials` (interface member) | silent | ✅ silent |
| `CreateMeeting.svelte:16` (`$props()` type literal) | silent | ✅ silent |
| `MeetingSession.ts:335` `options: JoinOptions,` (multi-line param) | silent | ✅ silent |

The refined rule ID fired, which independently confirms the closure: `AuthResult` carries
`password` + `userToken`, and `userToken` is reachable ONLY through segment matching —
under the `\b(token)\b` matcher this guard was originally specified with, this run would
have printed nothing at all.

**Registration proof** (existence ≠ execution):

```
$ git ls-files -s scripts/guards/simple/ts/no-retained-credentials.sh
100755 93fdd664b87c4d7ebf11cff5903d4785a3d74058 0	scripts/guards/simple/ts/no-retained-credentials.sh
```

Mode `100755`; `find scripts/guards/simple -name '*.sh' -not -path '*/fixtures/*'`
discovers it and `run-guards.sh`'s `[[ -x ]]` gate passes.

**Measured cost — two-pass shape** (supersedes the single-pass estimate): **8-9 ms**
across 5 runs, against `GUARD_TIMEOUT_SECS=30` and the ADR-0033 §4 90s p95 always-run
budget. Recorded as a baseline for the next author copying the first full-tree pattern
in the TS set.

**File counts, with the exemption stage named** so no future reader thinks someone
miscounted: **118** raw from `git ls-files 'packages/*'` filtered to `.ts`/`.tsx`/`.svelte`
→ **62** after `is_scan_exempt` (tests/fixtures/`__tests__`/test-utils/`.test.*`/`.spec.*`)
plus the build-artifact layer (`node_modules`/`dist`/`build`/`.svelte-kit`/`coverage`/`.d.ts`).

---

### POST-B2 RUN (the other half of the (a') evidence)

```
$ ./scripts/guards/simple/ts/no-retained-credentials.sh
STATUS=OK REASON=ts-no-retained-credentials-clean-62-files
EXIT=0
```

The guard fired on the real defect before the fix and is silent after it, on the same
62 files. That before/after against production code — not against a fixture — is the
primary evidence that the guard is not fitted to its own test set.

### (d) INDEPENDENT CROSS-CHECK — plain `git grep`, run alongside Rule 2

Rule 2 asserting (d) on its own is circular for a one-time check: if the guard has a
bug, its silence proves nothing. Both were run and both agree.

```
$ git ls-files 'packages/*' | grep -E '\.(ts|tsx|svelte)$' \
    | grep -vE '(__tests__|/tests/|\.test\.|\.spec\.|test-utils)' \
    | xargs grep -nE '^\s*(readonly\s+)?password\s*\??\s*:'
packages/sdk-core/src/http/types.ts:26:  readonly password: string;      <- RegisterInput  (transient DTO)
packages/sdk-core/src/http/types.ts:34:  readonly password: string;      <- LoginInput     (transient DTO)
packages/sdk-core/src/session/events.ts:65:  readonly password: string;   <- LoginCredentials    (call-scoped param)
packages/sdk-core/src/session/events.ts:77:  readonly password: string;   <- RegisterCredentials (call-scoped param)
```

Four remaining `password` field declarations, **all on transient types** — request
DTOs and the standalone-join parameter types. **Zero on any retained type**, which is
what (d) asks. Note the grep and the guard agree *and disagree usefully*: the grep
cannot tell a retained type from a parameter type, which is exactly why the guard's
retention predicate is the durable net and the grep is the one-time cross-check.

`AuthResult` has no references left outside the historical note in `types.ts` that
explains why the type was renamed.

---

## Gate 1 Record (Lead-maintained — durable copy of reviewer rulings)

<!-- Written by the Lead because reviewer analysis otherwise lives only in agent
     transcripts. A mid-loop API-capacity event (widespread 500s then 529s) took
     every teammate down at least once; this section exists so no ruling has to be
     re-derived. Reviewers resume from their own transcripts and remain authoritative
     for their own verdicts — this is a record, not a substitute. -->

### BLOCKING — P1 (@dry-reviewer): A2 does not fire on its own motivating defect

The guard as specified **ships green against the exact defect it exists to catch.** Rule 1
requires both a credential-vocabulary field AND a token-vocabulary field. Pre-fix `AuthResult`
pairs `password` with **`userToken`** — the credential half matches CATEGORY_A, but `userToken`
matches nothing, because the precedent matcher (`crates/dt-guard/src/ts_pii.rs`) is
word-boundary anchored and there is no boundary inside `userToken`.

This directly falsifies the plan's primary evidence claim ("both rules fire on the real
pre-fix tree — Rule 1 on `AuthResult` (`password` + `userToken`)"). As written, Rule 1 stays
silent there.

@dry-reviewer recommends **normalize-then-suffix-match** over adding `userToken` to the
catalog, with supporting evidence: `accessToken` is already in CATEGORY_A as a task-#11 patch
for this identical blind spot, which is evidence the per-name patch does not scale. Awaiting
@implementer's choice.

### BLOCKING — fixture double-exemption (@paired-infrastructure-2)

`common::test_code_filter::is_scan_exempt` exempts BOTH `/fixtures/` AND `crates/dt-guard/**`.
Fixtures at `crates/dt-guard/tests/fixtures/ts_retained_credentials/` are therefore exempt twice
over; driven through `run()` they are skipped before any rule evaluates and the test passes
green having proven nothing. Verification item (a) is the load-bearing evidence for the whole
(A) half, so the naive implementation is silently vacuous.

Note the plan cites this same double-exclusion as a *feature* (why fixtures are safe from other
TS guards). The property cited as a feature is the bug.

Required shape (`cite_extract_e2e.rs` precedent): a pure `(path, content)` kernel that `run()`
also calls, fixture-driven from `tests/`, **plus** a live `run-guards.sh` smoke proving the
wrapper is wired and actually fails.

### ADR-0024 §6.4 criterion — NOT a Guarded Shared Area (three independent concurrences)

No enumerated GSA row matches any path in this task. On the criterion limbs:

- **`crates/dt-guard/**` + `scripts/guards/**` are not a detection/forensics contract.**
  @code-reviewer: §6.2 makes Mechanical classification *conditional on guard coverage*,
  deliberately creating a forcing function for guard expansion; making guards themselves GSA
  would gate the cheap path behind ceremony and invert what §6.1 exists to remove.
  @paired-infrastructure-2, independently: `is_guard_internal_path` already classifies these
  roots as tooling rather than contract surface; and answering "yes" would widen the GSA
  perimeter by assertion while bypassing the micro-debate + five-location sync §6.4 requires,
  leaving `validate-gsa-sync.sh` enforcing a list one devloop widened unilaterally. Audit-trail
  weakening is permanent and undetectable; guard weakening is bounded and tripwired by the
  guard's own fixtures.
- **`MeetingSession.ts` is not auth-routing policy.** @code-reviewer: §6.4's canonical instance
  is the `ServiceType`/scope/identity *definitions*; B1 changes which already-valid credential
  the client presents to an unchanged decision function. §6.4 uses "wherever referenced" exactly
  once, scoped to ADR-0027 primitives — extending it by analogy would make every consumer of an
  auth primitive GSA. @paired-client, independently and more generally: every enumerated GSA is
  a surface where the authority decision is *defined or enforced*; `packages/**` holds no
  authority-deciding code at all, since the browser is untrusted and its claims are re-validated
  server-side by definition. If client code *could* set auth-routing policy, that would itself
  be the vulnerability — so no `packages/**` path can meet that limb.
- **ADR-0027 path-independent limb checked and does not fire.** @paired-client found that
  `MeetingSession.ts` *does* reference an approved primitive (`crypto.subtle.digest('SHA-256',…)`),
  which is the trap — "the file contains a crypto call" is not the test. GSA is per-hunk (the
  premise of hunk-level §6.7 trailers); B1's hunks are `#authenticate` and the `events.ts` union,
  and `meetingIdHash` is untouched.

**Consequence**: `--paired-with` satisfies the Domain-judgment rows; this does NOT re-route to a
client-implemented devloop (§6.5 would have forced that under a GSA reading — the only
consequential difference).

**Conditions attached to the ruling** (binding, not decorative):
1. @code-reviewer will UPGRADE to GSA on the spot, with the ESCALATE that implies, if B1 turns
   out to change *which token type or scope MC accepts* rather than merely which already-valid
   credential the client presents.
2. @paired-infrastructure-2's "no" is conditional on the fixture pair being real — if the
   kernel-plus-smoke evidence does not materialize, the structural basis for declining GSA
   status goes with it.
3. @code-reviewer requires an `Approved-Cross-Boundary: client` trailer from @paired-client,
   hunk-level not blanket, covering the `packages/**` items — the proportionate substitute for
   the escalation.

No classification upgrades raised. @paired-client checked all eleven `packages/**` rows
including the two `Mechanical` test rows (fixture field deletions compelled by the type change;
pass the sed-test). Nothing auto-routes to ESCALATE at Gate 1.

### Resolved scope questions

- **`AuthResult` → `AuthSession { subdomain, displayName?, userToken }`, dropping `email` and
  `mode` as well as `password`** — CONFIRMED by @paired-client, who ran the analysis
  independently before the plan landed and reached an identical field set. `email`'s only
  consumer is the deleted `credentials()`; `mode`/`AuthMode`'s only consumers are the deleted
  band-aid comment and one test assertion on a field nothing reads. The honest post-change type
  is a session, not an auth result. So B3 is not patching a symptom without the concept — the
  concept lands in B2 and B3 becomes its rendering consequence.
- **Retained password path (`Login`/`RegisterCredentials`)** — RETAIN, per @paired-client, with
  conditions. Zero production callers after B2, but it is published SDK API surface and removal
  is a semver break deserving its own decision. The "permanent guard exception" worry is
  dissolved mechanically rather than by promise: Rule 2 keys on retention sites, a function
  parameter is not one, so these types pass **on the merits with no allowlist entry**, and Rule 2
  enforces the criterion the moment anyone stores one. Condition: the class-level `@example` must
  switch to the token path — the SDK's advertised default must be the minimizing one.
- **`is_ts_build_artifact()` extraction** — TECH_DEBT, not a blocker. @dry-reviewer measured
  rather than estimated (13 entries, 7 shadowed by `is_test_path`, 6 genuinely additive) and
  confirmed the 3rd copy is not *forced*: `is_scan_exempt` is `pub` and A2 can consume it while
  layering the 6 additive entries, as both siblings already do. → `docs/TODO.md` at verdict time.
  **Condition**: if `ts_retained_credentials.rs` hand-rolls the test-path half instead of calling
  `is_scan_exempt`, it bypasses code that DOES exist in `common` and flips to true duplication +
  a blocking finding. @dry-reviewer checks at Gate 2.

### Lead rulings issued

- **My briefing error, disclosed**: my spawn prompt to @dry-reviewer wrongly named
  `secret_patterns::HYGIENE_PATTERNS` as A2's vocabulary. That catalog matches value shapes; A2
  matches field *names*, so the SoT is `pii_vocabulary.rs::PII_TOKENS_CATEGORY_A`, and
  `pii_vocabulary.rs` explicitly forbids consolidating the two. The implementer had already
  chosen correctly; @dry-reviewer verified independently and concurred. No damage.
- **Drift control (adopted, then strengthened by @dry-reviewer)**: declare the credential/token
  partitions by name filtered from CATEGORY_A. A member-count test catches additions and
  removals but **not renames** — so @dry-reviewer required **union-equality** instead: three
  explicit lists (credential/token/neither) asserted set-equal to CATEGORY_A, no fallthrough
  bucket.
- **(c) scope — lean stated, ruling delegated** to @test + @operations jointly. Lean: (c) = the
  existing always-run Layer 7 env-test (`24_join_flow.rs::test_mc_webtransport_connect_and_join`)
  + the deterministic (e) proof, with the env-test's single-token reuse **pinned by an explicit
  assertion if it currently holds only incidentally** — an incidental property is not a contract.
  Building the `global-setup.ts`/`bootstrapMeeting`/`joinAsUser` harness here would implement
  task #18 inside #58 and invert the story's explicit ordering.
- **`.claude/agents/semantic-guard.md` SSoT tiebreak** (if @semantic-guard and @dry-reviewer
  split): prefer the reference to `checks.md`. The forcing-function argument for the hardcoded
  list is weak because it fires at edit time on a file nobody edits when adding a check, whereas
  the drift is silent.

### Follow-ups for `docs/TODO.md` at Step 9 (NOT pulled into this loop)

- ~~**ADR-0034 §When-to-Revisit trigger has already fired and been passed in silence**~~ —
  **RETRACTED. Do NOT file this at Step 9.** @paired-infrastructure-2 raised it, then corrected
  themselves after @code-reviewer challenged the premise; the Lead verified at source and the
  correction holds in every particular:
  - **Already tracked**: `docs/TODO.md:387` §Polyglot Pipeline Follow-ups, "dt-guard
    subcommand-sprawl re-debate trigger" (2026-05-21, raised by @code-reviewer during Wave 2
    #45). Owner already `code-reviewer + infrastructure`; `/debate` + ADR already named as the
    route. Filing a second entry would fork the history.
  - **Passed deliberately, not silently**: that entry *replaced* ADR-0034's bare ≥10 heuristic
    with three concrete re-triggers — subcommand count reaches **40**, OR
    `cargo build --release -p dt-guard` cold-cache exceeds the Layer-1 budget, OR
    `dt-guard --help` becomes unscannable.
  - **The count was wrong**: verified 31 today (31 `Command` variants, 31 dispatch arms), 32
    after #58 — **nine under the re-trigger, not at it.**

  Lesson for the Lead, recorded because it is the same failure this whole task is about: I
  accepted "the trigger has fired and was stepped over silently" and committed to an action on
  it without checking `docs/TODO.md` — which is where the answer was, in one grep. A tracked
  decision read as an untracked oversight. Verify the premise, not just the conclusion.
- **Live silent-under-detection hole in a shipped security guard**: `source_scan_patterns()`
  filters `HYGIENE_PATTERNS` by string name with nothing asserting the subset resolves against
  the catalog. `AWS access key` and `JWT` are incidentally pinned by existing tests;
  **`OpenAI/Stripe-style key`, `GitHub PAT`, and `Slack token` are pinned by nothing** — rename
  any of the three and `ts_secrets` Check 2 silently stops scanning for it, green build, no
  signal. Outside #58's changeset. Credit: @dry-reviewer.
- **`is_ts_build_artifact()` extraction** — 3rd caller recorded against the 4th-caller threshold
  both `ts_secrets.rs` and `ts_pii.rs` document. Credit: @dry-reviewer.
- **Retained password path removal** — @paired-client's analysis says the cost is low (~2 lines
  for an embedder, since `AuthApiClient` is publicly exported); recorded so a future devloop does
  not re-litigate from scratch.

### Open findings with @implementer (not yet resolved)

| Reviewer | Count | Highest-value item |
|----------|-------|--------------------|
| @dry-reviewer | P1 + conditions | A2 does not fire on `userToken` (BLOCKING, above) |
| @paired-infrastructure-2 | 7 constraints | fixture double-exemption (BLOCKING, above) |
| @paired-client | 7 (F1–F7) | (e) as specced would not catch a silent roster regression: `#joinSignaling` reads `options.credentials.displayName` as an un-narrowed union access; a later variant omitting it, "fixed" by narrowing, makes every participant join nameless while (e) still passes. Wants `participantName` pinned on the decoded MC `JoinRequest`. Also: exhaustive `switch` + `never` in `#authenticate`, because a future variant carrying email+password would silently take the login branch and re-introduce this exact defect class |
| @semantic-guard | 8 | A1-consumer findings (detail in transcript) |
| @observability | 6 | detail in transcript |
| @security | 8 lenses | answered in plan; re-ruling requested |
| @test | (c) + fixture rigor | item (b) must be demonstrated, not asserted |
| @operations | (c) executor | plus Layer 3 cost/blast-radius/runbook items |

---

## Pre-Work

None.

---

## Implementation Summary

### (A) Guard extension

**A2 — `dt-guard ts-no-retained-credentials`** (`crates/dt-guard/src/ts_retained_credentials.rs`,
wrapper `scripts/guards/simple/ts/no-retained-credentials.sh`, mode 100755). Full-tree over
`packages/*` via `get_tracked_files`, two-phase: `collect_credential_types` builds a repo-wide
declaration index (with alias/`extends`/member-of closure, cycle-guarded), then `scan_retention`
flags retention sites against it. ONE predicate — retention ∧ credential — with the rule ID
selected *inside* the retention branch, so the refined `auth_state_password_with_token` cannot
exist without retention. Vocabulary is a total 3-way partition of `PII_TOKENS_CATEGORY_A`
(union-equality tested) plus a declared `STEM_EXPANSIONS` table (referential-integrity tested).
No bypass marker, by design.

**A1 — `scripts/guards/semantic/checks.md`**: a document-level language-scope preamble; client
sink surfaces added to §Credential Leak as items 5-10; a new §Client Credential Lifetime with
**two gates** (retention, and transmission-while-token-held, the latter lens-only), a cross-file
resolution procedure step, and a SAFE list sized to stop it false-positiving on every auth form.
The semantic-check enumeration was dereferenced across all 7 sites to make `checks.md` the single
authoritative list.

### (B) The fix

`MeetingSession.join` presents the token the app already holds (`{mode:'token'}`), via an
exhaustive `switch` with a `never` default. `validateUserToken` shape-checks the caller-supplied
token (RFC 7235 `token68`) before it reaches an `Authorization` header — a header-injection
guard, explicitly not verification. New bounded `failure_stage` value `credential_invalid` plus
`gcJoinFailureStage`, because a caller-supplied token of arbitrary age makes credential rejection
a routine `gc_join` outcome. `AuthResult` → `AuthSession { subdomain, displayName?, userToken }`.
The auth nav is removed from the DOM once a session exists, and the effective view is
bidirectional so dropping the session lands on sign-in rather than a blank page.

### Scope growth after plan approval — recorded, not naturalised

**`onSessionInvalid` + the session clear was NOT in the approved plan.** It was added in response
to @security's F-SEC-3: B1 makes the retained `userToken` the sole join credential and B3 removes
the auth nav, so together they deleted the recovery path — an expired token left the user holding
a dead credential with no affordance to replace it. A later reader should see that **B3 as
originally specced was incomplete**, rather than assuming this was always planned. Wired at
*both* `userToken`-bearing call sites (join and create) per the Lead's ruling: the property is
credential-shaped, not view-shaped.

### Verification

| Item | Result |
|---|---|
| (a) | `ts_retained_credentials_fixtures.rs` — 6 tests, exhaustive 2×12 matrix; positive fires with exact rule ID + line, negative (same shape, credential field removed, retention site intact) is clean |
| (a′) | Pre-B2: **exactly one** hit on the real defect. Post-B2: clean. Both pasted verbatim above |
| (b) | 3 lens fixtures under `fixtures/ts_retained_credentials/lens/` — @semantic-guard's Gate-3 report is the executor |
| (c) | Re-scoped to #18 per the joint @test/@operations ruling; server half pinned by an explicit assertion in `24_join_flow.rs`; story amended |
| (d) | Rule 2 + an independent `git grep`, both recorded above, both agreeing |
| (e) | 4 sdk-core tests: zero AC-origin requests (allowlist, not denylist), spy-wiring control on the register path, `participantName` pinned on the decoded MC `JoinRequest`, local rejection of a CRLF token |
| (f) | 6 web-app browser tests: nav present unauthed → absent authed → no credential input in the authed shell → 401-at-join and 401-at-create both recover to the sign-in **view** → 403 likewise |

Rust: 333 unit + 6 fixture tests. sdk-core: 189 tests, branch coverage 90.32% (threshold 90%).
web-app: 15 browser tests. Guards: 34/34.

**Measured cost**: 8-9 ms for the two-pass shape over 62 files (`GUARD_TIMEOUT_SECS`=30).
**File counts by exemption stage**: 118 raw → 62 after `is_scan_exempt` + build-artifact layer.

### Known-failing, pre-existing, NOT caused by this change

`STATUS=FAIL REASON=pnpm-audit-failed` (Layer 6). Verified by stashing the entire changeset and
re-running: 23 advisories (1 critical, 12 high) are present on the base tree. This diff adds no
dependencies.


---

## Files Modified

**Guard (A)** — `crates/dt-guard/src/ts_retained_credentials.rs` (new, + 22 unit tests),
`crates/dt-guard/tests/ts_retained_credentials_fixtures.rs` (new, 6 tests),
15 fixtures under `crates/dt-guard/tests/fixtures/ts_retained_credentials/` (12 mechanical
+ 3 under `lens/`), `scripts/guards/simple/ts/no-retained-credentials.sh` (new, 100755),
`crates/dt-guard/src/{lib,main}.rs` (registration),
`crates/dt-guard/src/common/pii_vocabulary.rs` (partition + `STEM_EXPANSIONS` + 3 drift
tests + consumer map + `accessToken` comment repair),
`crates/dt-guard/src/secret_patterns.rs` (union-equality test closing the
`source_scan_patterns()` silent drop).

**Semantic lens (A1)** — `scripts/guards/semantic/checks.md`;
`.claude/agents/semantic-guard.md`, `CLAUDE.md`, `.claude/skills/devloop/SKILL.md`,
`docs/decisions/adr-0024-agent-teams-workflow.md` (SSoT dereference, 7 sites).

**Client fix (B)** — `packages/sdk-core/src/session/{events,MeetingSession}.ts`,
`packages/sdk-core/src/validation/limits.ts`, `packages/sdk-core/src/index.ts`,
`packages/web-app/src/lib/{types,errorText}.ts`, `packages/web-app/src/App.svelte`,
`packages/web-app/src/views/{SignUp,SignIn,CreateMeeting,JoinMeeting}.svelte`.

**Tests** — `packages/sdk-core/src/session/__tests__/meeting-session.test.ts` (+7),
`packages/sdk-core/src/validation/__tests__/limits.test.ts` (+5),
`packages/web-app/src/__tests__/appShell.test.ts` (new, 6),
`packages/web-app/src/__tests__/{authViews,createMeeting,joinMeeting}.test.ts`,
`crates/env-tests/tests/24_join_flow.rs` (token-only property pinned).

**Docs / ops** — `docs/runbooks/devloop-validation.md`,
`docs/observability/metrics/client.md`, `docs/TODO.md` (5 routed follow-ups),
`docs/user-stories/2026-05-02-browser-client-join.md` ((c) re-scope + #18 inheritance),
`docs/specialist-knowledge/{security,infrastructure}/INDEX.md`, `.gitignore`.


---

## Devloop Verification Steps

`./scripts/layer-all.sh` — full run:

```
LAYER=1 RESULT=OK   DURATION=1
LAYER=2 RESULT=OK   DURATION=1
LAYER=3 RESULT=OK   DURATION=7     <- 34/34 guards, incl. the new one
LAYER=4 RESULT=OK   DURATION=1
LAYER=5 RESULT=OK   DURATION=1
LAYER=6 RESULT=FAIL DURATION=1     <- pnpm-audit, PRE-EXISTING (see below)
LAYER=7 RESULT=OK   DURATION=372   <- live Kind cluster
TOTAL_DURATION=384 TOTAL_RESULT=FAIL
```

**Layer 7 ran green against a live cluster**, including the (c-i) assertion added to
`24_join_flow.rs`:

```
test test_mc_webtransport_connect_and_join ... ok
STATUS=OK REASON=env-tests-passed
```

That is the server half of (c) executing, not merely compiling — one AC registration, one
user token threaded through create → GC join → MC WebTransport join, with the token now
asserted byte-identical at the end rather than incidentally reused.

### The one FAIL — adjudicated, owned, and NOT this changeset's

`STATUS=FAIL REASON=pnpm-audit-failed` — **23 advisories, 1 critical + 12 high**, above the
gate and not covered by any of the three entries in `docs/TODO.md` §Suppressed Advisories.

**Evidence it is pre-existing** (method, so it can be re-run rather than trusted): the
entire changeset was `git stash push -u`'d, `pnpm audit --audit-level high` re-run against
the resulting base tree, and the changeset restored. Identical advisory set — 23, same
severities. This diff adds **no dependencies**: one Rust module, one 4-line shell wrapper,
and TypeScript source edits.

**Adjudication**: not blocking on this changeset's merits, and **not silently inherited
either**. Per @code-reviewer's Gate-3 point, a red gate recorded only in a devloop message
becomes precedent — the next loop reads "Layer 6 was already red" and nobody owns it, which
is exactly how a masked failure becomes permanent. So it is filed as a P1 entry under
`docs/TODO.md` §Supply Chain with the base-tree evidence, a named owner (infrastructure +
security, the task #47/#48 pairing that owns the suppression machinery), and an explicit
note that the entry is a **pointer, not an adjudication** — each advisory still needs the
ADR-0033 §11 reviewed-PR path (fix, or suppress with reason + expiry + ticket).

P1 rather than P3 despite the advisories being mostly build-tooling, for a reason that is
about the gate rather than the CVEs: **a permanently-red always-run gate trains reviewers
to ignore it.**

### Guard-specific evidence (per the Gate 2 acceptance criteria)

| Criterion | Result |
|---|---|
| Pre-B2 run, captured BEFORE B2 landed, pasted verbatim | ✅ above — exactly 1 hit, `App.svelte:14`, refined rule |
| Post-B2 clean run | ✅ `STATUS=OK REASON=ts-no-retained-credentials-clean-62-files` |
| `git ls-files -s` shows mode `100755` on the wrapper | ✅ `100755 93fdd664…` |
| Guard discovered + executed by `run-guards.sh` | ✅ 34 guards run (was 33) |
| Measured wall time, two-pass shape | ✅ 8-9 ms |
| Both file counts with the exemption stage named | ✅ 118 raw → 62 in-scope |
| A-P17 not relaxed to make a stale fixture pass | ✅ the stale fixture was REPLACED by the cross-file pair; the retention gate is unchanged |
| `gsa-sync` still green (ADR-0024 §6.4 untouched) | ✅ `STATUS=OK REASON=gsa-sync-all-5-mirrors-in-sync` |


---

## Code Review Results

Full per-reviewer detail is in §GATE 3 VERDICTS above. Summary:

| Reviewer | Verdict | Found / Fixed / Deferred |
|---|---|---|
| Security | RESOLVED-FIXED | 7 / 7 / 0 |
| Test | RESOLVED-FIXED | 3 / 3 / 0 |
| Observability | RESOLVED-FIXED | 4 / 4 / 0 |
| Code Quality | RESOLVED-FIXED | 3 / 3 / 0 |
| DRY | RESOLVED-DEFERRED | 3 / 3 / 0 |
| Operations | RESOLVED-FIXED | 2 / 2 / 0 (+1 withdrawn as own error) |
| Semantic Guard | RESOLVED-FIXED (native UNSAFE → SAFE) | 2 / 2 / 0 |
| Paired: Client | RESOLVED-FIXED | 3 / 3 / 0 |
| Paired: Infrastructure | RESOLVED-FIXED | 4 / 4 / 0 |

**32 findings raised, 32 fixed. Zero deferred, zero spun out, zero escalated.**

DRY's RESOLVED-DEFERRED carries **zero deferred findings** — the label is driven solely by
§Accepted Deferrals holding three DRY extraction opportunities, which the protocol counts toward
it. @dry-reviewer argued explicitly for reading it that way rather than rounding to a clean
label, so the cost-shifts stay visible.

---

## Accepted Deferrals

No finding raised against this diff was deferred or spun out. The entries below are DRY extraction opportunities (which the review protocol routes here rather than through fix-or-defer) plus pre-existing conditions surfaced during review and tracked rather than fixed:

- `docs/TODO.md` §Polyglot Pipeline Follow-ups — `is_ts_build_artifact()` extraction (3rd caller vs documented 4th-caller threshold)
- `docs/TODO.md` §Polyglot Pipeline Follow-ups — comment/string-aware line lexer duplicated 3×
- `docs/TODO.md` §Polyglot Pipeline Follow-ups — vocabulary reachability, three modes (A closed here; B and C explicitly NOT closed)
- `docs/TODO.md` §Polyglot Pipeline Follow-ups — `ts_pii` has no secret-identifier limb (pre-existing coverage gap, not a live leak)
- `docs/TODO.md` §Polyglot Pipeline Follow-ups — CATEGORY_A promotion of `credential`/`credentials`/`creds`
- `docs/TODO.md` §Supply Chain — pre-existing unsuppressed `pnpm audit` reds, P1 with re-check trigger
- `docs/TODO.md` — Layer 7 Playwright lane (ADR-0033 §3 vs `layer7.sh` drift); made a requirement *on* task #18
- `docs/TODO.md` §Documentation Hygiene — knowledge-index 75-line cap Goodhart problem (filed 2026-08-03 by Lead with attribution per this bullet's instruction; owner @paired-infrastructure-2)

---

## Rollback Procedure

1. Start commit: `72ca36c816277549f92499722dcb5230085b6d82`
2. Review: `git diff 72ca36c..HEAD`
3. Soft reset: `git reset --soft 72ca36c`
4. Hard reset: `git reset --hard 72ca36c`
5. No schema changes; no infrastructure manifests applied — no forward migration or
   `kubectl delete` needed.

**REVERT ORDER MATTERS — A2 must be reverted before or together with B2.**

The guard and the fix are entangled by design: reverting B2 restores `password` on the
retained auth type, which makes the guard fire, which reds Layer 3 **repo-wide** with no
in-code bypass — nobody can commit anything until A2 is also out. That is the backstop
working, not a defect, but an oncall doing an emergency revert at 3am needs it written
down rather than derived.

They are therefore **separable commits**, ordered:
1. `B1/B2/B3` — the client fix.
2. `A1/A2` — the guard, fixtures, checks.md, runbook, docs.

So: reverting (2) alone leaves (1) clean. Reverting (1) alone is only safe once (2) is
out. "The guard itself is wrong" is also the documented bypass — see the runbook §6.3 row.

---

## Issues Encountered & Resolutions

### Issue 1: Six teammates died mid-loop on API 500s; Lead misread it as permanent
**Problem**: Transient `API Error: 500` / `529 Overloaded` idle notifications were read as agent
deaths. Six duplicates were spawned, creating split-brain — two `code-reviewer`s, two
`paired-client`s — with the *context-carrying* originals being the ones the Lead had told
everyone to avoid.
**Resolution**: `SendMessage` resumes a failed agent from its transcript; the originals were
alive. All six duplicates stood down and the roster reverted to original names — **except**
`paired-infrastructure-2`, where the stand-down was rescinded because the respawn had already
produced the fixture double-exemption finding. Seat assignment follows the context, not the
name. Later re-verified that `dry-reviewer-2` had already sent two messages before standing
down; its content was routed to the canonical reviewer to adopt or retract rather than left
orphaned.

### Issue 2: Gate 2 validated a tree that then moved
**Problem**: The implementer made two docs edits *after* the Lead's `layer-all.sh` run, so nine
reviewers were briefly reading an unvalidated tree.
**Resolution**: Lead re-ran Layer 3 rather than accepting "docs-only, nothing else changed" —
docs edits can break `validate-todo-tracking` and the doc-citation guards. Clean. Tree frozen for
the remainder of review. Implementer's own diagnosis is the durable form: *"docs-only" is a claim
about risk, not about validation state.*

### Issue 3: Re-running Gate 2 mid-review produced false alarms
**Problem**: Lead re-ran Layer 3 during active fix cycles and hit `scope_drift_inbound` on
`auth_client.rs`, mid-write while a finding was being fixed.
**Resolution**: Checked before concluding — the file *was* modified and the plan row *did* exist.
Lead corrected its own approach: validation during fix cycles produces flicker, and reading it as
breakage creates pressure to stop fixing things. Policy set to re-validate once against the final
tree.

### Issue 4: Final validation failed on a layer Gate 2 had passed
**Problem**: Layer 5 `FAIL` (Prettier) on the final tree, having been `OK` at Gate 2.
**Resolution**: Root cause was not new breakage — Gate 2's Layer 5 was an **nx cache hit**
(`DURATION=1` across five TypeScript projects). See §INSTANCES 10 & 11. Fixed via
`prettier --write`; re-validated with the full pipeline, then Layer 5 re-verified with
`--skip-nx-cache` because the re-run *also* reported `DURATION=1`.

### Issue 5: Pre-commit hook blocked commit 2 on unfilled `main.md` sections
**Problem**: `Devloop output main.md incomplete: has unfilled TBD sections`.
**Resolution**: Legitimate catch by the repo's own guard — §Code Review Results, §Accepted
Deferrals, §Issues Encountered and §Lessons Learned were still placeholders while the analysis
lived in Lead-maintained sections higher up. Filled. Worth noting the hook enforced exactly the
discipline this task is about: an artifact that *looks* complete because its interesting parts
are written is not a complete artifact.

---

## Lessons Learned

### The headline: a claim reasoned to and never executed

Every design-changing finding in this loop was a claim someone reasoned to and never ran.
Each was reversed by running something in under a minute.

**It is not a role-specific failure mode.** Four roles made it, on four different artefacts:

| Who | The claim | What running it showed |
|---|---|---|
| implementer | "Rule 1 fires on the real pre-fix `AuthResult`" | `\b(token)\b` matches **neither** `userToken` nor `user_token`. The guard would have shipped green against its own motivating defect. |
| implementer | "zero false positives by construction" | Four ordinary transient DTOs fire (`{newPassword, resetToken}`, `{currentPassword, csrfToken}`, `{password, mfaToken}`, `{clientSecret, accessToken}`). |
| implementer | the FP matrix, measured once | Re-measuring after the closure changed surfaced a **fourth** site: `MeetingSession.ts:335`, a multi-line function parameter. Without excluding it the guard red-lines `MeetingSession.join`'s own signature. |
| Lead | the ADR-0034 sprawl trigger "fired and was stepped over" | One grep: already tracked, deliberate, with a re-trigger at 40. |
| paired-infrastructure | `scan_source(path, content)` from the `cite_extract_e2e.rs` precedent | That precedent is genuinely per-file; Rule 2's pass 1 is repo-wide. A correctly-executed analogy to the wrong precedent — which "run it" does **not** catch. The check that does: *does the precedent's shape match the rule's shape?* |
| dry-reviewer | `accessToken` "effectively dead by convention" | Dead in `metric_labels` only (it lowercases first); reachable in the other two, whose matchers run against raw line text. |

**The asymmetry is what makes it dangerous**: reasoning produces a confident claim at
near-zero cost, execution produces a correct one at slightly above zero.

### Why THIS task was unusually prone to it

@semantic-guard counted seven instances of *coverage that reads as real and isn't*: the
`\btoken\b` non-match; fixtures exercising the wrong sub-check; fixtures inside the
directory their own reviewer's procedure exempts; the fixture double-exemption; the
union-alias blindness; the stale positive fixture; and the per-file kernel seam.

The mechanism: **every one of them is a *negative* claim** — "nothing was retained", "no
credential leaked", "no AC call was made", "the guard found nothing". Negative claims fail
silently by construction, because *a broken detector over a clean tree is indistinguishable
from a working one*. This generalises well past credential guards to any absence-shaped
assertion.

Three corollaries worth keeping:

1. **For any new detector, assert it fires on the real pre-fix artefact, not only on a
   fixture** (@code-reviewer). In all three of this loop's "check passes while proving
   nothing" cases, *a fixture existed and passed* — so "add fixtures" is not the lesson.
2. **Generate probes adversarially, against the matcher — not from the vocabulary**
   (@dry-reviewer). A matrix built from the terms you match can only confirm they match;
   it cannot find the words you never thought to include. `creditCard` was missing from
   the first matrix for exactly this reason.
3. **When an assertion names a proxy for the outcome, check the state where the proxy
   holds and the outcome doesn't** (@test/Lead). "The Sign-in nav returns" passes with a
   blank `<main>`. Three false greens this loop were caught in *assertions*, not in code.

### The terminal state generalises beyond broken detectors (@code-reviewer)

A check whose signal has been destroyed by **permanent redness** and a check that
**asserts something other than what it names** arrive at the same place by different
routes: *a check that exists and tells you nothing.* Nobody reads a gate that is always
red, and nobody can read a gate that is green for the wrong reason.

That is why the pre-existing `pnpm audit` failure is filed P1 despite the advisories
themselves being mostly build-tooling and P3 on severity — the priority is a property of
the **signal**, not of the CVEs. A future triager who scores it on severity alone will
downgrade it, be locally correct, and destroy the gate. The reasoning is written into the
`docs/TODO.md` entry for exactly that reason.

Same lens on the guard this task adds: it is always-run, full-tree, and has no bypass, so
one uninvestigated false positive would convert it into a gate people route around. That —
not the CVE count — is what the measured FP matrix and its standing re-measurement
obligation are protecting.


### Gate 3 finding T1 — I wrote the loop's own defect into the fix for it

@test found the C2 assertion was **tautological**: it compared `user_token` against a
clone of itself, and since `user_token` is not `mut` and is never reassigned, the borrow
checker guarantees it holds. It could not fail.

Worse than vacuous — **actively misleading, and worse than the state it replaced.** Before
#58 the token-only property was incidental but *honestly unclaimed*. My version added a
20-line doc comment asserting the property was "now pinned explicitly", so a future
maintainer would trust it and ship the regression. And it was blind to the exact
regression it named: a re-auth introduces a NEW binding (`let token2 = register(...)`) and
passes that to the join, leaving the original untouched and the assertion green.

**An unasserted property with an accurate comment is safer than a vacuous assertion with a
confident one.** That is the sharpest statement of this loop's thesis, and it arrived as a
finding against the very changeset that documents the thesis — while §Lessons Learned above
was already written.

Fixed by counting AC requests that actually happened (`AuthClient::call_count()`), with the
residual stated rather than overclaimed: the counter observes calls through that client
instance, so constructing a second `AuthClient` would evade it.

**Why this instance is the most instructive of the loop's eight**: the others were claims
about *code someone else would write* or about *a matcher's behaviour*. This one was a claim
about **my own test**, written immediately after I had documented the failure mode, in the
file whose purpose was to close it. Knowing the pattern is not protection from it. The only
thing that catches it is another reader executing the claim — @test read the full function
body and traced what the assertion could observe, rather than reading the comment.


### Gate 3 round 2 — the guard's own fix introduced two false positives, caught by running it

@security's F-SEC-5/F-SEC-6 were both real and both verified against the built binary:
phase 2 read only the FIRST identifier of an annotation, so `$state<undefined | AuthResult>`
was **silent** while `$state<AuthResult | undefined>` fired — and since reordering a union
is semantically a no-op, that was an **invisible bypass**: with no `guard:ignore` hatch, the
natural response to a finding is to edit the line until it goes green, and a reorder does
exactly that while looking cosmetic and leaving no greppable marker.

**Fixing it broke the guard on the real tree, twice, and only running it showed that.**

Adding `class` to the declaration matcher (F-SEC-6) immediately produced two false
positives on `MeetingSession.ts`:

1. **Class bodies are code, not type syntax.** `password: input.password` inside
   `AuthApiClient.register`'s request body is field-SHAPED without being a field.
   Collecting fields at any nesting depth — correct for `interface`, where the whole body
   *is* the type — made the class credential-bearing.
2. **Method parameters in multi-line signatures.** `credentials: UserTokenCredentials,` on
   its own line sits at brace-depth 1 inside the class, lexically identical to a field.
   **This is the same trap phase 2 already handled** (`MeetingSession.ts:335 options:
   JoinOptions,`) — it reappeared in phase 1 the instant classes entered the index,
   because only classes have method bodies.

And a third defect the fix exposed rather than caused: `interface A { password: string }`
on ONE line was never indexed at all, because the block walker only inspected *subsequent*
lines for the opener.

Three lessons, all sharper than the originals:

- **A widening is exactly where new false positives live.** The measured FP matrix is a
  standing obligation precisely because a change to the matcher moves the surface — this
  is the clause in the module doc being exercised within a day of being written.
- **The same trap can recur in a second location.** Phase 2's multi-line-parameter
  exclusion did not protect phase 1. Fixing a shape once does not fix it everywhere the
  shape occurs, and the module doc now says so where a future declaration-kind addition
  will read it.
- **Reviewers reproduce against the binary; so should the implementer.** @security ran the
  four bypass shapes on a scratch repo rather than reading the regex. Doing the same after
  the fix is what surfaced the FPs before Gate 3 closed rather than after landing.

All three are locked by tests (`class_bodies_are_code_not_type_syntax`,
`class_method_parameters_are_not_class_fields`, `single_line_declarations_are_indexed`)
and by re-running the guard on the real tree: `STATUS=OK … clean-62-files`, with all four
of @security's bypass shapes firing on the scratch repo.


### The WARN channel is a check too — and it decayed the moment it was always on

**Found independently by FOUR reviewers** (@security F-SEC-7, @observability F-O4,
@paired-infrastructure F-INFRA-3, @code-reviewer Finding 2). Indexing `class` pulled
`SignalingClient` (468 lines) past `MAX_DECL_BLOCK_LINES = 400`, so the guard emitted a
truncation `WARN` on **every green run** while reporting `STATUS=OK`.

Benign today — nothing was actually missed, and every reviewer said so rather than
overstating it. The finding is about the **channel**:

- A warning present on every run has no signal value. The runbook asks triagers to watch
  for an *uptick* in dt-guard WARNs; an uptick is unreadable against a constant-1
  baseline.
- @code-reviewer supplied the connective tissue: *"a permanently-WARNing guard trains
  readers identically, and faster, because it sits under a green STATUS."* That is the
  exact reasoning used to file the pnpm-audit red as P1 — applied to a red gate while
  shipping a permanently-warning one.
- What it warns about — silent under-matching — is the one failure mode with **no bypass
  hatch and no other detector behind it.**

**The fix is a retune, not a bump**, because @code-reviewer's diagnosis was that the cap
counted the wrong thing *relative to its purpose*: 468 lines is a normal class, and a cap
that trips on ordinary code isn't bounding pathology, it is silently reducing coverage. So
the constant is now documented as a **runaway backstop** — the walk terminates on brace
depth; the cap exists only for unbalanced/generated input — set to 2000 (~4x the largest
real declaration, measured) with an explicit tuning rule, plus two tests: one fails the
build if a future declaration approaches the cap *before* the WARN starts firing, one
confirms the runaway guarantee survived retuning.

**Option (b) considered and declined, with reasoning recorded** so it isn't re-proposed:
counting field-shaped lines bounds *fields collected*, not the *walk*, so unbalanced
braces would still run to EOF. The line bound is what makes termination happen.

**Three things this instance adds beyond the earlier ones:**

1. **My re-measurement was real and still missed it.** I re-measured the FP surface after
   the class widening, as the standing clause requires, and it came back clean — while the
   guard warned on every run. The regression landed in a channel the clause didn't name.
   It now names both: a clean run must also be a WARN-free run.
2. **The honesty note covered false positives only.** The same standard now applies to the
   false-negative side.
3. **Four reviewers found it by running the guard; nobody found it by reading.** The
   symptom was one line of stderr under a green status.

### Instance 8 — a wrong justification inside a fix for a previous instance

@code-reviewer's Finding 3, and the most self-illustrating item of the loop. The comment
written into `MAX_DECL_BLOCK_LINES` *specifically so the next contributor would not
re-derive the rejected option* asserted the wrong property: it claimed the cap is what
makes the declaration walk terminate. It isn't — `cursor + 1 < lines.len()` in the loop
condition guarantees that independently, with or without the cap.

The conclusion was right and the reasoning was wrong, which is the dangerous combination:
a contributor who checks the stated claim finds it false, and the natural next step from
"the cap isn't needed for termination" is "the cap isn't needed" — the opposite of what
the comment existed to prevent. **A load-bearing comment has to be right or it is worse
than absent.**

What the cap actually buys is **bounded work per declaration**: without it an
unbalanced-brace file makes each walk O(file length), so N declarations cost
O(N x file length) and the scan goes quadratic. Corrected in place, with the error left
noted rather than silently overwritten.

Its place in the taxonomy: **object-axis displacement** — the justification asserts one
property (termination) while the code establishes a different one (bounded work) — inside
a comment written to prevent exactly that drift. Second instance appearing in a fix for a
previous instance, after the exhaustiveness `default` that leaked a credential. That is
the strongest evidence in this loop that the pattern is not individual carelessness but a
property of how a claim and its verification drift apart under iteration: every fix is a
new claim, and it arrives with less scrutiny than the original because it is *responding*
to scrutiny.

**Prefer checks whose failure mode is absurd output over checks whose failure mode is a
clean pass** (@code-reviewer). Their broken process-substitution reported "every file
missing" and was caught in one glance; the tautological assertion looked right and needed
another reader. Loudly-wrong checks are cheap; plausibly-wrong ones are what the taxonomy
is for — and it is worth *choosing* the loud shape when both are available.

### Instance 9 — the test written to enforce the tuning rule could not enforce it

@paired-infrastructure's F-INFRA-4, and the cleanest statement of the loop's pattern
because it happened in a fix for instance 8's fix.

`cap_clears_the_largest_real_declaration_with_margin` compared `MAX_DECL_BLOCK_LINES`
against a hardcoded `LARGEST_REAL_DECLARATION: usize = 468` — **two literals in the same
file**. It evaluated `2000 >= 1404` and would have done so forever, whatever `packages/**`
did. A class growing to 2200 lines leaves the constant at 468 (nobody updates a number
they have no reason to look at), the test green, the WARN firing, and coverage silently
reduced. Precisely the failure the test existed to prevent.

Its own assertion message said the cap "must sit far above the largest real declaration."
It could not know the largest real declaration. It knew `468`.

**Third instance of a specific shape** @dry-reviewer and @paired-infrastructure named: *a
value that must correspond to something else, with nothing asserting the correspondence.*
`STEM_EXPANSIONS` keys ⊆ CATEGORY_A — asserted. `HYGIENE_SOURCE_SCAN_SUBSET` names ⊆
`HYGIENE_PATTERNS` — asserted this loop. `LARGEST_REAL_DECLARATION` = the largest
declaration in `packages/**` — not asserted, and the one where **staleness is guaranteed
rather than possible**, because the tree changes without anyone touching that file.

Fixed by *deriving the measurement*: the test walks the real tree via `load_in_scope_files`
+ `largest_declaration_span`, reusing the scanner's own primitives so a second measuring
implementation cannot disagree with the thing it measures. The cross-tree coupling is the
mechanism, not a side effect — writing a 700-line class in `packages/` now fails at build
time with the file, declaration and span named.

**And this time the negative control was run before the claim was made**: with the cap
temporarily set to 900, the test fails with `SignalingClient … spans 468 lines`. That is
@test's remedy — *name the regression it targets and confirm it fails under that
regression* — applied to my own fix rather than only recommended in prose. It is also the
step whose absence produced instance 7.

### Prefer impossible over detected

Recurring across three independent findings: hold the fixture double-exemption open by
*calling the kernel*, not by exempting the exemption; hold the Rule-1 ⊂ Rule-2 subset by
*emitting from inside the retention branch*, not by a test asserting two predicates agree;
hold vocabulary integrity by *union-equality*, not by a member count. A test can be
deleted, skipped, or quietly amended. A structure cannot drift from itself.

### Duplication with a guard on it is a different category

The Lead's owner constraint on ADR-0024 §6.4: its five-way mirrored GSA list *looks* like
the most conspicuous duplication in the file, and dereferencing it would have deleted a
working control (`gsa_sync`'s count-check) while technically satisfying a
"remove the duplication" instruction. **Before dereferencing any enumeration, check
whether something already asserts it.**

### Systemic finding: three modes, not one root cause

Recorded as three in `docs/TODO.md` because a follow-up scoped from the original
one-root-cause framing would close Mode A, leave B and C open, and *look* closed — this
loop's own failure shape appearing in the *description* of a fix. (A: referential
integrity under rename — closed here. B: matcher-capability mismatch. C: absence masked
by naming.)

---

## Appendix: Verification Commands

```bash
# Full pipeline (Lead, Gate 2 and again against the final tree)
./scripts/layer-all.sh

# Gate 1 classification-sanity guard (requires the binary to exist first)
cargo build --release -p dt-guard
./scripts/guards/simple/validate-cross-boundary-classification.sh \
  docs/devloop-outputs/2026-07-29-client-credential-lifetime-guard-task58/main.md

# The new guard, directly
./scripts/guards/simple/ts/no-retained-credentials.sh
target/release/dt-guard ts-no-retained-credentials --root /work --explain

# Fixture suite (kernel-driven; bypasses the is_scan_exempt double-exemption)
cargo test -p dt-guard --test ts_retained_credentials_fixtures

# Layer 5 WITHOUT the nx cache — a green Layer 5 at DURATION=1 is a cache hit,
# not a lint run (see §INSTANCES 10 & 11)
cd packages && pnpm exec nx run-many -t lint --skip-nx-cache

# Layer 6 pre-existing-red verification (three independent methods)
git status --porcelain | grep -E "package\.json|pnpm-lock|Cargo\.(toml|lock)"  # expect: none
./scripts/layer6.sh 2>&1 | grep "SKIPPED-NO-DIFF"                              # rust: no-dep-changes
git stash push -u && ./scripts/layer6.sh; git stash pop                        # reproduces on base

# Post-commit: the wrapper's recorded mode. Returns EMPTY before the commit,
# because the path is untracked — establishing nothing (@code-reviewer, instance 6).
git ls-files -s scripts/guards/simple/ts/no-retained-credentials.sh   # expect 100755
```
