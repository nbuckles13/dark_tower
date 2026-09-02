# Provenance — sframe-wg SFrame test vectors

Vendored, verbatim, committed. **Nothing fetches this at validation time**: no `curl`, no `git clone`,
no build script reaching out. It is inert committed data, reversible by deleting one path.

## Source

| Field | Value |
|---|---|
| Upstream repository | `https://github.com/sframe-wg/sframe` |
| Commit (full SHA) | `025d568a506937c901af8e7f0a663f39aeaf67ad` |
| Commit subject | `Update test vectors` |
| Upstream path | `test-vectors/test-vectors.json` |
| Vendored path | `proto/test-vectors/external/sframe-wg/test-vectors.json` |
| Size | 35121 bytes |
| SHA-256 of vendored bytes | `b8d35efd41749567427cb9ae52d9a7362904154978ff6a5fb20c0258a8ffdec1` |
| Retrieved | 2026-09-02 |

The digest covers **verbatim upstream bytes**. The file is not subsetted, reformatted, or
re-serialised, so there is no extraction transform to audit and the digest means what it appears to
mean. Row selection happens at read time via `manifest.json`.

`validate-frame-vectors.sh` check **g10** recomputes this digest on every devloop and CI run and fails
on mismatch. Digest recomputation is owned by the guard alone, not by either language's harness — one
recomputation, one failure site.

## Licence — read the words, not a summary

Upstream `LICENSE.md` at `025d568` is **not a licence text**. It is three lines pointing elsewhere.
Reproduced verbatim so a future reader evaluates the actual words rather than anyone's paraphrase:

```
# License

See the
[guidelines for contributions](https://github.com/mlswg/mls-architecture/blob/master/CONTRIBUTING.md).
```

Assessment recorded at vendoring, raised to security, operations and the team lead before the file
landed rather than after: this is IETF working-group output derived from RFC 9605, reproducible under
BCP 78 / the IETF Trust Legal Provisions, and vendoring IETF test vectors is routine practice. **This
is the repository's first vendored third-party file** — there is no `vendor/` directory, no
`THIRD_PARTY` manifest and no prior convention — so the pattern is being set here rather than
followed.

## What this artifact gates, and what it does not

Stated as a list rather than as prose. An understated scope invites a redundant gate later; an
overstated one invites someone to skip a real one.

**In scope — four things:**

| # | Gated | How |
|---|---|---|
| 1 | HKDF-SHA512 key schedule | `sframe_secret` (PRK, 64 B), `sframe_key` (32 B), `sframe_salt` (12 B), and the exact `sframe_key_label` / `sframe_salt_label` byte strings |
| 2 | Nonce derivation | `nonce == sframe_salt XOR BE12(ctr)`. Zero-extending our BE32 `stream_sequence` into 12 bytes **is** BE12 of the same integer, so the constructions are identical; the upstream `ctr` is `17767`, which exceeds 16 bits, so multi-byte placement is genuinely exercised |
| 3 | AES-256-GCM primitive | upstream `pt` / `aad` / `ct` through our seal |
| 4 | 128-bit tag length | implied by the `ct` length relative to `pt` |

**Out of scope — and this half matters more.** None of the following is gated by anything outside
this repository:

- our SFrame object layout (`key_id(8) || tag(16) || ciphertext`); RFC 9605's own is `config || KID || CTR` with a trailing tag
- our AEAD associated data (the publisher region); RFC 9605's is `sframe_header || metadata`
- our signed range (publisher region ‖ payload)
- our detached-tag split
- our KEK unwrap
- the entire frame header, the relay region, the signature and the TLV extension registry

The external rows have no relay region, no signature, no wrap and no extensions. **The AAD span, the
signed range and the detached-tag split are gated only by the Rust reference generator against the
TypeScript codec** — the three computations with no other independent check, undiminished by this
anchor. Do not read across.

## Precedence outcome — and the ladder not taken

Recorded here rather than only in a devloop output, because the devloop output is not what a future
reader opens.

The task specified a precedence ladder: gate against **0x0005**; if absent at the pinned commit, fall
back to **0x0004** as a partial check of the identical code path; only if neither GCM suite has usable
derivations, drop to a third-party SFrame cross-check, stated explicitly rather than proceeding on
independent authorship alone.

**Outcome: the primary branch. No fallback, nothing degraded, nothing to declare.**
`cipher_suite: 5` is present at `025d568` with the complete derivation chain — `base_key`, `kid`,
`ctr`, both labels, `sframe_secret`, `sframe_key`, `sframe_salt`, `nonce`, `aad`, `pt`, `ct`. Its
`sframe_secret` is 64 bytes, confirming HKDF-SHA512 as ciphersuite 0x0005 forces. ## Independence of the re-derivation — stated as it actually happened

The earlier draft of this record said "three parties re-derived the schedule independently and matched
it byte for byte." That overstates it, and an overstated provenance record is worse than an understated
one for the same reason the scope list above must be exact in both directions. The accurate statement,
in @security's words:

> **Two independent implementations, written from the RFC in different tooling, both reproduce the
> upstream artifact's self-consistent fields; the second was written with knowledge of the first's
> results.**

What that qualification does and does not cost:

- **The fetch and the digest are fully independent and load-bearing.** Each party retrieved the upstream
  bytes and hashed them separately. One party's reported SHA-256 could not influence what another's
  `sha256sum` printed over bytes they fetched themselves.
- **The schedule re-derivation remains a genuine check**, for a reason that is not obvious: the
  comparison target was never the other party's numbers, it was the upstream file's own `sframe_secret`
  / `sframe_key` / `sframe_salt` fields. Knowing that the PRK "should be" 64 bytes does not help an
  independent HMAC implementation emit the *correct* 64 bytes.
- **What is genuinely weakened is the failure branch.** Had the second derivation disagreed, its author
  would have begun by hunting a bug in their own code rather than doubting upstream or the first party.
  That bias is real and is the honest limit of the claim.

**And the confidence is pointed at the part that was never at risk.** This upstream material arrives
externally anchored and internally self-consistent; agreement on it confirms our HKDF and GCM, which
was not the exposure. The correlated-error risk lives entirely in the out-of-scope list above — the AAD
span, the signed range, the detached-tag split, the SFrame clear-header shape and the KEK unwrap. **The
external anchor's strength is not transferable to those rows, and no comfort about them may be drawn
from the rigour here, however many derivations agree.**

0x0004 **is** vendored and gated, but as the SHA-256 **contrast case** that makes "the PRK is 64 bytes
and the hash is SHA-512" falsifiable rather than self-referential — its PRK is 32 bytes. It is not the
fallback rung. Its gate terminates at key-schedule output (hash, PRK length 32, derived key length 16)
and never reaches a seal or open call, alongside an explicit assertion that our AEAD constructor
**rejects** any key length other than 32 — otherwise we would have built an AES-128 acceptance path,
reachable by anything able to influence a suite id, in order to test that we do not have one.
ADR-0036's amendment table permits AES-128-GCM "only for SFrame interop", and we do none.

## The 289-row `header` array

The vendored file also contains a 289-row `header` array covering RFC 9605's own SFrame header
encoding. **It gates nothing of ours** — our header is the ADR-0036 §2 binary frame header, not
RFC 9605's — and `manifest.json` does not select it.

It is present because the file is vendored **verbatim**: hand-subsetting to the two GCM rows would
have required a committed extraction script to stay auditable, and would have made the SHA-256 cover
our transform rather than upstream's bytes. Recorded explicitly so a future reader reads its presence
as verbatim-vendoring rather than as relevance, and its non-selection as deliberate rather than as
oversight.
