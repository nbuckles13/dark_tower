# Semantic Guard Checks

These checks are used by the `semantic-guard` agent during devloop validation. Each check describes a class of issues that pattern-based guards cannot catch reliably.

The agent analyzes the current diff (added/changed code only, excluding test files) against each check below.

**Language scope (read once, applies to every check).** Checks are written against the idioms of a specific language stack, and a check gives no signal outside it — say so rather than guessing:

- *Credential Leak* — items 1-4 are Rust/tracing-shaped and items 11-13 are Rust, MC-scoped, and about key custody; items 5-10 are the `.ts`/`.svelte` sink surfaces, which have no Rust analogue. Items 11-13 give **no** signal on TypeScript.
- *Client Credential Lifetime* — `.ts` / `.tsx` / `.svelte` only.
- *Actor Blocking*, *Error Context Preservation* — Rust only. No client analogue; do not apply them to TS.
- *Metrics Path Completeness* — written against `counter!` / `histogram!` / `gauge!`. It gives **no** signal on the client's `this.#counter(...)` sink; treat client metric paths as out of scope for it.

**Fixture-verification runs.** Test files are normally out of scope. When the Lead directs a fixture-verification run, named fixture files ARE in scope — otherwise a dutiful "no findings" would be procedurally correct and completely vacuous.

---

## Check: Credential Leak

Secrets reaching a **sink** — an exfiltration event. (Whether a credential is *retained too long* is a different question; see §Client Credential Lifetime. Keeping the two apart matters for finding attribution: a `console.log(session)` is a leak, not a lifetime violation.)

### Rust (items 1-4)

1. **Secrets in logs**: Passwords, tokens, secrets, or keys logged via `info!`, `debug!`, `warn!`, `error!`, `trace!`, or tracing macros.

2. **Missing `skip_all` in `#[instrument]`**: Functions with sensitive parameters (password, token, secret, key, credential) that use `#[instrument]` without `skip_all`. The `skip()` denylist approach is unsafe because new fields leak by default.

3. **Debug formatting secrets**: Structs containing secrets being formatted with `{:?}` in logs or errors.

4. **Error message leaks**: Error messages (`Err`, `anyhow!`, `bail!`) that include secret values.

### Client — `.ts` / `.tsx` / `.svelte` (items 5-10)

The client's sinks have no Rust analogue, so items 1-4 give no signal on them. A credential or token reaching any of these is a finding:

5. **Metric labels** — `MetricsSink.counter/histogram/gauge` label values. Labels are bounded, low-cardinality, and exported; a token in one is exported forever.

6. **Span attributes** — anything set on an OTel span.

7. **`console.*`** — the TS counterpart of item 1.

8. **Thrown / serialized errors** — a credential on `Error.message`, on a custom error field, or reachable through a `cause` chain. Client errors cross into UI text and telemetry.

9. **`JSON.stringify` / structured clone** — stringifying a whole options or session object is the usual way a credential reaches a sink that was only meant to receive a summary.

10. **The DOM render path** — a credential interpolated into markup, or reaching it via an error-formatting helper (e.g. `errorText(err)` → `{joinError}`). Client-only, and the surface a Rust-derived lens is most likely to miss.

**SAFE**: a credential passed to the function that transmits it (`AuthApiClient.login({...})` building a request body inline); an error formatter that **allowlists** fields rather than spreading (see §Client Credential Lifetime, "whitelist-projection boundaries").

### Rust — MC key custody (items 11-13)

Media-path key material, per ADR-0036 §4 and §11. **These are separate items rather than more vocabulary on item 1 because they key on a different event.** Items 1-4 key on a *sink*; these key on a *boundary crossing*. A KEK assigned into an MC→MH request reaches no log, no `{:?}` and no error string — items 1-4 give it exactly zero signal, and it is the one way a key leaves the component entitled to hold it.

Scope: `crates/mc-service/**` production code, plus any construction of a `dark_tower.internal.v1` message anywhere in the tree.

**Who is entitled to hold what** — the whole check is "did this value reach someone not in this table":

| Material | Entitled holders | Never |
|---|---|---|
| Meeting KEK | MC's per-meeting actor, in memory; every client MC admits, via the join response and the KEK-push message | MH; any `dark_tower.internal.v1` message; disk; any log, metric, span or error/panic payload |
| Transmit key, unwrapped | the sending client only | MC; MH; anything on the wire |
| Transmit key, wrapped | the publisher region of the sender's own frames | an MC→MH control message; a relay-side cache |

11. **Key material crossing the MC→MH contract** — KEK or transmit-key bytes assigned into, or passed to a builder or constructor for, any `dark_tower.internal.v1` message or any MH-client call. Absolute, and the reason it is absolute is that every property §7 and §11 derive from "MH cannot see the media" rests on it: ADR-0036 §4, *"MH stays keyless, and that is a guard, not a convention."*

12. **Key material into an MC sink** — the meeting KEK, a wrapped or unwrapped transmit key, or any value derived from one, reaching a tracing macro, a span field, a metric label, an error or panic payload, or a `{:?}` on a struct that transitively holds it (subject to the redacting-wrapper carve-out under item 13).

13. **Redaction defeated at the call site** — a hand-rolled `Debug`/`Display` that prints key bytes; an `expose_secret()` / `expose_*()` whose result flows to any item-12 sink; a `#[derive(Debug)]` newly added to a struct that transitively reaches key material; a `skip_debug` entry removed from `crates/proto-gen/build.rs`.

    **Redacting wrappers break the transitive reach.** A `#[derive(Debug)]` is SAFE when every field on the path to key bytes is a redacting secret wrapper whose own `Debug` redacts — `common::secret::SecretBox` is the in-tree one. It is a finding only when the derive would print raw key bytes because a field on the path is a raw `[u8; N]`, `Vec<u8>`, or another non-redacting type. A hand-rolled `Debug`/`Display` that prints the bytes is always a finding, wrapper or not.

    Verify the wrapper's `Debug` **actually redacts** rather than trusting a `Secret`-shaped name. This fails in both directions: a newtype over `SecretBox` is safe however it is named, and a newtype named `SecretKey` over a raw `[u8; 32]` is not.

    Scope: `Debug` / `Display` only. It does **not** extend to `Serialize` or other serialization derives — those are a different sink, under item 12.

**Opaque relay is not a crossing.** A wrapped transmit key travelling client→client inside a frame payload that MH forwards as an *uninterpreted byte range* is not a finding — it is ciphertext under a KEK MH does not hold, and relaying it is the design. Lifting the wrapped-key field out of a frame **into a `dark_tower.internal.v1` message** is a finding under item 11 — that construction is in scope anywhere in the tree. The other targets ADR-0036 §4 forecloses — an MH-side reuse cache that re-attaches the key to new subscribers' frames, or an MH-side log, metric or span of it — are **MH-side, outside items 11-13's MC scope, and named here as a boundary, not a rule**, so this clause does not read as covered where it is not. What holds them today: the re-attach is foreclosed structurally by the publisher-region signature (§2/§4 — MH can neither attach nor strip a signed field), and MH-side logs, metrics or spans of a wrapped key are **not covered by any shipped control today**. ADR-0036 §11 contemplates a directory-scoped macro deny over MH's media hot-path directory; neither that directory nor that deny exists yet, though `crates/mh-service/src/transport/mod.rs` §Placement is already laid out so it will scope exactly when it lands. Bringing the MH frame-forwarding path under *this* check is a scope extension into `crates/mh-service/**` and the `crates/media-protocol/**` GSA (owners: semantic-guard + media-handler + protocol), tracked in `docs/TODO.md` §Media Path Obligations under **"Credential-leak items 11-13 do not reach the MH frame-forwarding path"**, and **required before any MH code path parses the wrapped-key field out of a frame** — today MH forwards opaque byte ranges; the moment it parses that field the gap is live. The vocabulary half of the same gap — `wrapped_key` still absent from CATEGORY_A, and MH (story task 16) and the SDK (story task 19) both log — is tracked separately under **"dt-guard `pii_vocabulary.rs` CATEGORY_A does not contain `wrapped_key`"**, which is what independently substantiates "not covered by any shipped control today" above.

**Judge the value, not the name.** ADR-0036 §11 forecloses the word-list reading — *"Vocabulary additions cannot be cited as the protection… The directory-scoped deny catches by shape."* A `Vec<u8>` read out of the meeting actor's KEK field, moved through a helper, and renamed `bytes` / `payload` / `material` / `blob` is the same finding under a name no vocabulary contains. Follow the value across function and file boundaries the way §Client Credential Lifetime's Procedure does; a hunk that shows only the hand-off does not clear it. The vocabulary at `crates/dt-guard/src/common/pii_vocabulary.rs` is a **floor** these items sit above — do not restate its tokens here — the same rule §Client Credential Lifetime states of itself under *Relationship to the mechanical guard*: a second vocabulary would drift from the first silently.

**SAFE — read these before flagging.** None of the following is key material:

- `kek_generation`, a key id, or its `(sender, stream, generation)` components. Metadata that identifies *which* key, never the key. (Both remain barred from per-frame logs, metric labels and span attributes by §11 — a cardinality and voice-activity-trace rule, not this one.)
- `sender_id`, and `identity_public_key`, which is a public key. **Both** are out of scope for *this* check while remaining barred from metric labels and span attributes as stable per-participant identifiers — linkability, not leakage.
- The `key_custody` label, fixed at `operator` today (ADR-0036 §4 — the "today" is load-bearing: §4's KEK-source seam contemplates a key server that would change it).
- `JoinResponse.meeting_kek` and the KEK-push message. These **are** §4's delivery mechanism to a client MC has just admitted; delivery to an entitled holder is not leakage.
- Key **lengths** and length constants as metadata. `RedactedLen` in `crates/proto-gen/src/lib.rs` is the in-tree safe form — it prints a byte count, never content.

**Fixture poles** (for the positive-leak fixture task). Must fire: a `kek: [u8; 32]` or `MeetingKek` field added to an MC→MH request; `debug!(?kek)`; `info!("kek={:x?}", kek)`; `#[derive(Debug)]` on `struct Foo { kek: [u8; 32] }`. Must **not** fire: `debug!(kek_generation = gen, sender_id = id)`; `#[derive(Debug)]` on `struct Foo { kek: MeetingKek }` where `MeetingKek` wraps `SecretBox`.

**Not this check.** ADR-0036 §11 also bars any end-to-end or zero-trust boolean in a metric, log, dashboard or document. That is an **overclaim** predicate, not an exfiltration one, and folding it in here would blur this check's question. It needs its own named check; tracked in `docs/TODO.md` §Media Path Obligations under **"The end-to-end / zero-trust boolean overclaim has no named guard or check"**.

---

## Check: Client Credential Lifetime

**Scope**: `.ts` / `.tsx` / `.svelte` production code.

**The question**: is a raw credential still reachable after the exchange that made it unnecessary — or being re-sent when a token is already held? This is about *lifetime and transmission*, not about reaching a sink (that is §Credential Leak).

**Why this check exists**: the web-app held the user's password in a long-lived session object and replayed it at join time, long after it held a `userToken` the server would have accepted on its own. Nothing owned "don't keep the password once you hold a token" — the credential-leak check above is Rust-shaped and keyed on exfiltration, and the client's other guards are string-scans for hardcoded literals, which catch nothing about retained runtime state.

### Two gates

This check has **two independent predicates**. Do not collapse them.

- **Gate 1 — Retention**: a raw credential is reachable after the token exchange. Token-presence *refines* this gate (a credential retained alongside a token is provably unnecessary), it does not define it.
- **Gate 2 — Transmission-while-token-held**: a credential is included in an outbound request — or in an argument that produces one — at a call site where **a valid token is already held** — *not* one this very request is about to obtain. **Zero retention is required.** A view that collects a password into a bare local, posts it while already holding a valid token, and stores it nowhere satisfies this gate completely.
  - The qualifier is load-bearing, not throat-clearing: "a token identifier is in lexical scope" would flag **every login form**, because a form that declares `let token = $state('')` and posts the password to fill it has a token in scope at that call site. The credential→token exchange *is* the legitimate case — see SAFE-A. Ask whether a valid token is **held**, not whether one is **visible**.

### Relationship to the mechanical guard (`dt-guard ts-no-retained-credentials`)

The guard implements **Gate 1 only**, and only where a type annotation gives it something to key on. It is a **floor**: this lens is additive and never green-lights something the guard fails.

The guard's vocabulary is `crates/dt-guard/src/common/pii_vocabulary.rs::PII_TOKENS_CATEGORY_A`. This lens is deliberately **not** word-list-bound — judging whether a value is a credential is the whole point of the intent layer. Do not restate a word list here; a second vocabulary would drift from the first silently.

**Gate 2 has no mechanical counterpart at all.** Transmission is not retention, and no syntax matcher reaches it.

### Procedure — follow the credential across boundaries (do this, don't skim for it)

The motivating defect was invisible on any single line. `SignIn.svelte` constructed `{email, password, userToken}` and handed it to `onAuthed` — which looks like an ordinary function call. `App.svelte` stored the result in `$state` — a line whose type's fields aren't in the hunk.

So: **when a credential-bearing value is passed to a callback, a prop, an event dispatch, or a store setter, resolve the destination and judge the destination's lifetime.** This crosses files. A hunk that only shows the hand-off is not enough to clear it.

### UNSAFE

```ts
// 1. Credential retained in session state alongside a token (the original defect).
interface AuthResult { password: string; userToken: string; }
let auth = $state<AuthResult | undefined>(undefined);

// 2. Credential crossing OUT of the view that collected it, into parent state.
onAuthed({ subdomain, email, password, userToken: resp.accessToken });

// 3. Gate 2 — re-sending a credential while a token is in scope.
function credentials() {
  return { mode: 'login', email: auth.email, password: auth.password }; // auth.userToken is RIGHT THERE
}

// 4. Persisting either a credential or a bearer token.
localStorage.setItem('session', JSON.stringify({ password, userToken }));
```

### SAFE — read these before flagging; without them this check false-positives on every auth form

```ts
// A. Form binding in the view that COLLECTS the credential. SAFE for that view's
//    lifetime. The boundary is the CROSSING, not the holding.
let password = $state('');
```

A bare-string credential in the collecting view becomes a finding only when it (a) **crosses out of that view** — callback, prop, store, parent state, persistence — or (b) is **read again after a token exists**. Clearing it after the exchange (`password = ''`) is good defense-in-depth; its **absence is NOT itself a finding**. Flagging correct login forms is how a check gets ignored.

```ts
// B. Call-scoped parameter — the credential is an argument nobody stores.
validatePassword(password);
await client.login({ subdomain, email, password });

// C. A credential-bearing PARAMETER TYPE that nobody retains. `LoginCredentials` is a
//    member of the public `JoinCredentials` union and is reached only through
//    `join(options)`. Reach the same verdict the mechanical guard does: a function
//    parameter is not a retention site.
export interface LoginCredentials { mode: 'login'; email: string; password: string; }

// D. A request DTO — a type that only ever describes a request body.
export interface RegisterInput { email: string; password: string; }

// E. Whitelist-projection boundary. A long-lived buffer or bus fed by a projection
//    function is SAFE when the projection ALLOWLISTS fields; it is a finding when it
//    spreads. `packages/web-app/src/lib/e2eBus.ts` is the in-tree example of the safe
//    form — it drops `bindingToken`/`correlationId` explicitly rather than spreading.
```

### Known blind spots this lens OWNS (the mechanical guard cannot see them)

- **Bare-string credentials** — no type annotation to key on, so both mechanical rules are silent. Present in-tree twice (`SignUp.svelte`, `SignIn.svelte`). Judge them by the crossing rule above.
- **Novel storage idioms** — a credential pushed into a `Map`, held in an untyped object literal, or captured in a closure. The guard's retention-idiom set is enumerated, not exhaustive.
- **All of Gate 2** — transmission has no mechanical counterpart.

---

## Check: Actor Blocking

In actor-based code (files in `actors/` directory or with "Actor" in struct names):

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
- Awaiting `mpsc::Sender::send()` (backpressure, nearly instant)
- Awaiting oneshot for request-response within same message handling
- `tokio::spawn()` wrapping long operations (fire-and-forget)

**UNSAFE patterns** (flag these):
- Helper methods called by the actor that await external responses (blocks the message loop)
- `timeout(Duration::from_secs(N))` where N > 1 in non-`select!` context
- Awaiting `task_handle.await` (waiting for child task completion)
- Awaiting Redis/gRPC calls directly without `spawn()`

**Key insight**: The danger is when async methods CALLED BY the actor block the message loop. The actor can't process new messages while waiting.

---

## Check: Error Context Preservation

Look for `.map_err(|e| ...)` patterns where error context may be lost:

**UNSAFE patterns** (flag these):

1. **Error logged but not included in returned error**:
```rust
.map_err(|e| {
    tracing::error!("Operation failed: {}", e);
    MyError::Internal  // Error context logged but not in returned error
})
```

2. **Generic error message without original context**:
```rust
.map_err(|e| MyError::Crypto("Encryption failed".to_string()))  // No context from e
```

3. **Error variable captured but not used**:
```rust
.map_err(|e| MyError::Internal("Something failed".to_string()))  // e captured but unused
```

**SAFE patterns** (do not flag):

1. **Error context included in returned error**:
```rust
.map_err(|e| MyError::Internal(format!("Operation failed: {}", e)))
```

2. **Error context in structured error type**:
```rust
.map_err(|e| MyError::CryptoError {
    msg: "Encryption failed".to_string(),
    source: e.to_string()
})
```

**Key principle**: The error variable `e` should be included in the RETURNED error type, not just logged and discarded. Client-facing errors can use generic messages, but the underlying error should capture full context.

---

## Check: Metrics Path Completeness

When a function records metrics (counter!, histogram!, gauge!) on some code paths,
verify that ALL exit paths record equivalent metrics.

UNSAFE patterns (flag these):
1. Early return via `?` that bypasses metric recording when other paths in the
   same function record metrics
2. `match`/`if let` branches where some arms record metrics and others don't
3. Error paths that `return Err(...)` before reaching metric recording calls

SAFE patterns (do not flag):
- Pure metrics functions (functions whose only purpose is recording metrics)
- Functions that record metrics unconditionally (all paths go through recording)
- Test code
- Functions where the early return is before any business logic (e.g., input
  validation at function start, before the operation being measured)

Key insight: Look for functions where `histogram!`, `counter!`, or `gauge!` calls
appear deep in the function body, then check whether earlier `?` or `return`
statements can exit the function before reaching those calls.
