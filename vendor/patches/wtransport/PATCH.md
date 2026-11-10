# Vendored wtransport — provenance, patch, drop procedure

`vendor/wtransport/` is the published crates.io source of **wtransport 0.7.2**,
byte-for-byte, plus ONE additive patch (`PATCH.diff`, beside this file). It is
wired in by the root `Cargo.toml` `[patch.crates-io]` entry and excluded from
the workspace. This file and `PATCH.diff` live OUTSIDE the crate directory, so
the crate tree is exactly upstream plus the patch and nothing else.

## Why it exists (ADR-0039 §7)

MH must end a client session it closes on purpose with a numeric
`MhCloseCode` that a **browser** can read. Browsers expose a close code only
from a `CLOSE_WEBTRANSPORT_SESSION` capsule (`WebTransportCloseInfo.closeCode`);
a QUIC `CONNECTION_CLOSE` carrying the number makes `WebTransport.closed`
reject, which the client must treat as transport loss. wtransport 0.7.2 can
RECEIVE that capsule but has no API to SEND one, and its CONNECT stream is
owned by its internal driver task. No newer upstream release adds it.

## The machine-readable record (read by `dt-guard vendor-patches`)

<!-- vendor-patch:begin -->
```
upstream-crate: wtransport
upstream-version: 0.7.2
upstream-crate-sha256: b4273ce3157a3262a68665f8d3f20a0ac0c5b8a69ffd67f05ae986832ebec036
pristine-tree-digest: fc514e3d51c40eb2ec44be816a23fb0a695f38e79e4aadf917785016b20f89a3
```
<!-- vendor-patch:end -->

- `upstream-crate-sha256` is the crates.io index checksum of
  `wtransport-0.7.2.crate` (it was also the root `Cargo.lock` checksum before
  the patch; a path dependency carries none, so this file is now its only home).
- `pristine-tree-digest` is `dt-guard`'s tree digest (SHA-256 over the sorted
  `path NUL x|- NUL sha256(content)` rows of every file) of that `.crate`
  unpacked. ONE implementation computes it, in `crates/dt-guard/src/vendor_patches.rs`.

## Classification

Guarded Shared Area by the ADR-0024 §6.4 criterion (wire-format runtime
coupling: it writes the `CLOSE_WEBTRANSPORT_SESSION` bytes every browser
client parses). Owners: **protocol + infrastructure + security**. Not
enumerated in `scripts/guards/simple/cross-boundary-ownership.yaml`
(classified by criterion; enumeration would be an ADR-0024 change).

## The patch (`PATCH.diff`) — additive only

Every added line is tagged `PATCH(dark-tower ADR-0039 §7)`. No upstream line is
removed or changed, so code that never calls the new API (MC, env-tests) runs
upstream's code byte-for-byte. Public surface added — exactly four items:

1. `Connection::close_session(code: u32) -> Result<(), SessionCloseError>` — the
   driver writes ONE HTTP/3 DATA frame holding ONE `CLOSE_WEBTRANSPORT_SESSION`
   capsule (code as u32 big-endian, zero-length message) verbatim on the CONNECT
   stream, then FINs it. The write is its OWN cancel-safe select arm in the driver
   (progress held in driver state, one quinn `write` at a time), so a peer that
   withholds flow-control credit never stalls the driver loop. The call resolves
   once the capsule is written; **the caller must bound it** (MH bounds the write
   AND the wait for the peer's reaction together by `CLOSE_SESSION_LINGER`, 1 s, and
   closes QUIC with `H3_NO_ERROR` on expiry, which also fails the stuck write). At
   most once per connection. After it, ANY peer read-side outcome on the CONNECT
   stream (FIN, reset, a frame, connection loss) completes the close with QUIC
   `H3_NO_ERROR` — never upstream's `H3_CLOSED_CRITICAL_STREAM` and never the
   application code (the stream has left upstream's `ConnectStream`).
2. `error::SessionCloseError` — why a session close could not be sent:
   `AlreadyClosing`, `DriverGone`, `NoConnectStream`, `Write(StreamWriteError)`. No
   peer data, no credential; MH logs it as `error = %e`. If the CONNECT stream is
   already gone it is `NoConnectStream` and nothing is sent (no fallback to a QUIC
   close carrying the code).
3. `Connection::session_close_received()` — once the driver has ended, the
   session close the PEER sent (a capsule's code and message, or code 0 for a
   bare CONNECT FIN). Filled only by upstream's existing parser. A test
   observer: MH production code is guarded from calling it
   (`crates/mh-service/tests/close_emission_scan.rs`).
4. `#[doc(hidden)] connection::encode_close_session_capsule(code)` — the exact
   bytes item 1 writes; the driver writes its output verbatim.

Residual (recorded): a capsule still unacknowledged when the caller's bound
expires (heavy loss) is discarded by the QUIC close, and the browser sees
transport loss, which the client handles safely.

## Where it is tested

The vendored crate is a non-member path patch, so its own tests and examples
never run in the pipeline (`cargo test -p` refuses a non-member with
dev-dependencies). The patch is tested from mh-service instead:

- golden bytes typed from RFC 9114 §7.2.1 / RFC 9297 §3.2 / draft-ietf-webtrans-http3
  / RFC 9000 §16, and an encode-then-parse round trip through upstream's own
  parser: `crates/mh-service/src/webtransport/close.rs` (unit tests
  `capsule_bytes_are_the_rfc_encoding`, `capsule_bytes_round_trip_through_the_upstream_parser`);
- behaviour over a real in-process QUIC pair:
  `crates/mh-service/tests/webtransport_close_code_integration.rs` and
  `media_session_binding_integration.rs`.

## Guards and exclusions

- `dt-guard vendor-patches` (Layer 3, `scripts/guards/simple/validate-vendor-patches.sh`):
  the tree minus `PATCH.diff` digests to `pristine-tree-digest`; the diff is
  additive-only and applies exactly; the crate is in root `exclude`; Cargo.lock
  resolves `wtransport 0.7.2` to the path copy with no registry copy left; no
  symlinks; nothing else under `vendor/`.
- Layer 6 (`scripts/lang/rust/audit.sh`): fetches the `.crate`, checks it against
  `upstream-crate-sha256`, and has `dt-guard vendor-patches --pristine-digest`
  compare its digest with the record (the record is checked, not trusted).
  Advisories: the audit scans a derived lock in which the vendored package
  carries its crates.io source again (`dt-guard vendor-patches --attributed-lock`),
  so a RustSec advisory against wtransport 0.7.2 still blocks.
- Images: `.dockerignore` admits `vendor/wtransport/`, and every service Dockerfile's
  builder stage does `COPY vendor/ vendor/` BEFORE `cargo chef cook` (cargo-chef's
  recipe skeletonizes workspace members only, so the cook would otherwise not find
  the patch target). `dt-guard docker-context` enforces both
  (`patch-path-not-admitted`, `patch-path-not-in-cook-stage`).
- `workspace-deps` skips R2/R3 for vendored crates (upstream's manifest cannot
  be edited); `docker-context` requires every `[patch.*]` path to be admitted by
  `.dockerignore`.
- Exclusions: ONE, in `scripts/setup.test.sh`: its no-`rust-version` (MSRV) scan
  skips `vendor/`, because upstream's manifest declares `rust-version` and cannot
  be edited. Nothing else: `cargo fmt --all`, `cargo clippy --workspace` and
  `cargo build --workspace` do not select a non-member path patch, and every other
  Layer-3 guard passed with this tree in place. Content scans skip `vendor/`
  through ONE predicate (`crates/dt-guard/src/common/git_changes.rs::is_vendored`);
  `dt-guard vendor-patches` requires every `[patch.*]` target to be exactly
  `vendor/<name>`, so that predicate and the vendored set agree.
- Advisories against `wtransport 0.7.2` are ALWAYS `AUDIT_FINDING=NEW(vendored)`
  and block (security ruling), on every base; the only escape hatch is the governed
  suppression manifest (`audit-suppressions.toml`).
- Dependabot: NO `ignore:` (forbidden by the audit-suppressions guard). A bump PR
  for `wtransport` goes red on `dt-guard vendor-patches`
  (`vendor-patch-not-in-lock`) and must follow the drop procedure below.

## Drop procedure (when upstream can send a session close)

1. Bump `wtransport` in `[workspace.dependencies]` to the release with the API.
2. Port MH's call sites (`crates/mh-service/src/webtransport/close.rs`) to it.
3. Delete the `[patch.crates-io]` entry, the `exclude` entry, `vendor/wtransport/`,
   `vendor/patches/wtransport/`, the `.dockerignore` line, the `VENDORED` comment in
   `.github/dependabot.yml`, the `PATCHED:` comment on the workspace `wtransport`
   entry, and the `scripts/setup.test.sh` vendor/ exclusion comment if nothing else
   is vendored.
4. Keep the GENERIC machinery: `dt-guard vendor-patches`, the docker_context
   `[patch.*]` rule, the workspace_deps vendored partition and the audit.sh vendored
   path all go quiet at zero vendored crates (`STATUS=OK
   REASON=vendor-patches-clean-vendored-0-added-0`). Do not delete the guard with the crate.
5. Keep every test in "Where it is tested" green — they assert the wire, not this patch.

## Updating the patch

Edit `vendor/wtransport/` (additive only), then regenerate `PATCH.diff` as a
unified diff from the pristine `.crate` (`a/<path>` → `b/<path>`, 3 lines of
context). `pristine-tree-digest` does not change unless the upstream version does.
