# @darktower/sdk-core

Browser SDK core for Dark Tower: a WebTransport transport abstraction and the
length-prefix wire framing codec. Private package (`@darktower/` scope,
`publishConfig.access: "restricted"`); not published.

> Scope: this package currently ships the transport layer (R-13/R-14) and the
> byte-level framing codec (R-16). The `SignalingClient` (protobuf
> `ClientMessage`/`ServerMessage` encode/decode, `JoinRequest`, join flow) and
> the metrics sinks land in later tasks.

## Build

Vite library mode → ESM (`.mjs`) primary + CJS (`.cjs`) fallback + `.d.ts`
types. The package `exports` map is closed (no `./*` wildcard); the single
entry point is the root barrel.

```sh
pnpm -C packages/sdk-core build       # vite build
pnpm -C packages/sdk-core test:unit   # fast unit tier (framing + transport)
pnpm -C packages/sdk-core test:component  # real prod build + bundle-content contract
pnpm -C packages/sdk-core lint        # tsc --noEmit
```

## Transport (`IWebTransport`)

`IWebTransport` (in `src/transport/IWebTransport.ts`) is the **canonical** home
of the transport contract. `BrowserWebTransport` (production) and
`@darktower/test-utils`' `MockWebTransport` (tests) both satisfy it via
structural typing — there is no package dependency edge between sdk-core and
test-utils (Pattern A; see `docs/TODO.md` #58).

`connect(url, options?)` initiates a handshake and returns a
`BrowserWebTransport`. In **production** it uses standard CA-validated TLS. The
dev-only self-signed-cert trust path (`serverCertificateHashes`) is gated behind
the build-time flag `__DEV_TRUST_FINGERPRINT__` (`false` in prod, tree-shaken
out — asserted by `tests/bundle-content.test.ts`). The fingerprint value is
supplied at runtime via config; it is never hardcoded.

## Framing codec (length-prefix)

`src/framing/length-prefix.ts` implements the 4-byte big-endian length-prefix
wire format:

```
[ 4-byte BE u32 length ][ payload bytes ]
```

- `encodeFrame(payload)` → framed `Uint8Array`. Rejects empty payloads and
  payloads larger than `MAX_MESSAGE_SIZE` (64 KiB) with a typed `FramingError`.
- `FrameDecoder.push(chunk)` → array of complete payloads. Buffers partial
  reads across `ReadableStream` chunks and emits multiple frames from one chunk.
  The declared length is range-checked **before** any body is buffered, so the
  decoder's carry buffer is bounded by one in-flight frame (≤ 64 KiB + 4).

### Wire-contract source of truth

The codec matches the Meeting Controller's wire contract **exactly**:
`crates/mc-service/src/webtransport/connection.rs` — `MAX_MESSAGE_SIZE` at
`:33`, the BE `put_u32` length prefix at `:648`, oversize rejection at `:607`,
empty-message rejection at `:617`.

The length prefix is a hand-rolled transport envelope (not a proto-defined
message), so it is intentionally **out of scope** for the
`proto/test-vectors/` shared TS↔Rust vector harness (that covers the 42-byte
media frame header). The byte-layout test in
`src/framing/__tests__/length-prefix.test.ts` (test 10) plus the source
references above are the contract anchors. If the length prefix ever needs
cross-language vectoring, fold it into the media-frame vector harness.
