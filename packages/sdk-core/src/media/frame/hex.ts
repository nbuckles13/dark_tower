// File: packages/sdk-core/src/media/frame/hex.ts
//
// Hex codec and byte primitives for the v2 media frame stack: lowercase,
// unprefixed, even-length hex — the convention `proto/test-vectors/` uses for
// every byte-string field — plus the concat / big-endian-encode / XOR helpers
// the SFrame key schedule and the frame codec are built on.
//
// ---------------------------------------------------------------------------
// PROMOTED FROM THE TEST TREE AT STORY TASK 15 (moved, not copied)
// ---------------------------------------------------------------------------
//
// This file was authored at story task 8 under `__tests__/`, because at that
// point it had no production consumer: the TypeScript v2 codec did not exist.
// Task 15 is that consumer, so the file MOVED here rather than being copied.
//
// The reason a copy was forbidden, recorded because it still applies to the
// next person who needs hex and does not find it: two implementations that
// agree forever and diverge only when someone edits one are invisible while
// they agree. No test fails, nothing is observably wrong, and it surfaces only
// if a reviewer happens to look. Most duplication announces itself by drifting;
// this kind does not.
//
// THIS IS THE ONLY BYTE<->HEX IMPLEMENTATION IN `sdk-core`. `MeetingSession.ts`
// (`meetingIdHash`) previously carried an unnamed inline byte->hex loop that was
// character-for-character `bytesToHex` below; it was folded into this file in
// the same commit as the promotion, closing the `docs/TODO.md`
// §Cross-Service Duplication entry that tracked it. `packages/test-utils/src/
// deterministic-ids.ts` spells the same loop in a DIFFERENT PACKAGE for a
// different purpose and is deliberately left alone — a real boundary, not
// duplication.
//
// The methodological note from that finding, which generalises past this file:
// the Gate-1 sweep that declared this a first home grepped for NAMED helpers
// (`bytesToHex|hexToBytes|toHex|fromHex`) and could not see an unnamed inline
// loop — and the inline form is the majority case, because nobody extracts four
// lines until a second caller exists. A DRY sweep for named helpers
// systematically under-reports exactly the duplication class that is cheapest to
// create.

/**
 * A byte run backed by a plain `ArrayBuffer`.
 *
 * WebCrypto's `BufferSource` rejects the `ArrayBufferLike` that `Uint8Array`
 * defaults to under TS 6 (it admits `SharedArrayBuffer`, which the platform
 * refuses for key material). Naming the narrow type once keeps every crypto
 * call site free of casts.
 */
export type Bytes = Uint8Array<ArrayBuffer>;

/** Lowercase, unprefixed, even-length hex — the vectors' wire convention. */
const HEX_RE = /^[0-9a-f]*$/;

/**
 * Decode canonical vector hex into bytes.
 *
 * Rejects loudly rather than coercing: uppercase, `0x` prefixes, odd length and
 * non-hex characters are all errors. A lenient decoder would silently accept a
 * malformed vector file and turn a fixture defect into a crypto mismatch 300
 * bytes downstream, where it reads as an implementation bug.
 */
export function hexToBytes(hex: string, what = 'value'): Bytes {
  if (typeof hex !== 'string') {
    throw new TypeError(`${what}: expected a hex string, got ${typeof hex}`);
  }
  if (hex.length % 2 !== 0) {
    throw new Error(`${what}: odd-length hex (${hex.length} chars) — not a whole number of bytes`);
  }
  if (!HEX_RE.test(hex)) {
    // Report the OFFENDING INDEX, never the content. `what` already names the
    // field, which is what locates a bad vector row; the input bytes add nothing
    // a debugger needs. This matters because this file was promoted out of
    // `__tests__` at story task 15 for production use: the natural task-19 caller
    // is `hexToBytes(kekHex, 'kek')`, and an echo of the input here would put half
    // a KEK into an `Error.message` and out to an embedder's Sentry hook —
    // `ts_pii.rs` scans `console.*`/`logger.*` only, so nothing would catch it.
    // Capped now, before a secret-bearing caller exists (@observability,
    // @semantic-guard).
    const badIndex = [...hex].findIndex((c) => !/[0-9a-f]/.test(c));
    throw new Error(
      `${what}: not canonical vector hex (lowercase, unprefixed, [0-9a-f] only); ` +
        `first offending character at index ${badIndex === -1 ? hex.length : badIndex}`,
    );
  }
  const out = new Uint8Array(hex.length / 2);
  for (let i = 0; i < out.length; i += 1) {
    out[i] = Number.parseInt(hex.slice(i * 2, i * 2 + 2), 16);
  }
  return out;
}

/** Encode bytes as canonical vector hex (lowercase, unprefixed). */
export function bytesToHex(bytes: Uint8Array): string {
  let out = '';
  for (const b of bytes) out += b.toString(16).padStart(2, '0');
  return out;
}

/** Byte-wise equality. Not constant-time — these are public test vectors. */
export function bytesEqual(a: Uint8Array, b: Uint8Array): boolean {
  if (a.length !== b.length) return false;
  return a.every((v, i) => v === b[i]);
}

/** Concatenate byte runs in order. */
export function concatBytes(...parts: readonly Uint8Array[]): Bytes {
  const total = parts.reduce((n, p) => n + p.length, 0);
  const out = new Uint8Array(total);
  let at = 0;
  for (const p of parts) {
    out.set(p, at);
    at += p.length;
  }
  return out;
}

/**
 * Big-endian encoding of `value` into exactly `width` bytes.
 *
 * Takes a `bigint` because the SFrame key id is 64 bits and exceeds
 * `Number.MAX_SAFE_INTEGER`. Throws rather than truncating if the value does
 * not fit: a masking encoder aliases distinct key ids onto one another, which
 * is a key-id collision and therefore a nonce reuse.
 */
export function bigUintToBytesBE(value: bigint, width: number, what = 'value'): Bytes {
  if (value < 0n) throw new RangeError(`${what}: negative (${value})`);
  const limit = 1n << BigInt(width * 8);
  if (value >= limit) {
    throw new RangeError(`${what}: ${value} does not fit in ${width} bytes (max ${limit - 1n})`);
  }
  const out = new Uint8Array(width);
  let v = value;
  for (let i = width - 1; i >= 0; i -= 1) {
    out[i] = Number(v & 0xffn);
    v >>= 8n;
  }
  return out;
}

/**
 * Byte-wise XOR of two equal-length runs.
 *
 * Both the length check and the per-index check throw rather than defaulting a
 * missing byte to zero. Under `noUncheckedIndexedAccess` the tempting fix is
 * `a[i] ?? 0`, which would silently produce a WRONG nonce out of a length bug
 * instead of failing — and a wrong nonce is the one error class this whole
 * vector exercise exists to prevent.
 */
export function xorBytes(a: Uint8Array, b: Uint8Array): Bytes {
  if (a.length !== b.length) {
    throw new RangeError(`xorBytes: length mismatch (${a.length} vs ${b.length})`);
  }
  const out = new Uint8Array(a.length);
  for (let i = 0; i < a.length; i += 1) {
    const av = a[i];
    const bv = b[i];
    if (av === undefined || bv === undefined) {
      throw new RangeError(`xorBytes: index ${i} out of range`);
    }
    out[i] = av ^ bv;
  }
  return out;
}
