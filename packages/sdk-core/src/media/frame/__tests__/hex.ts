// File: packages/sdk-core/src/media/frame/__tests__/hex.ts
//
// Hex codec for the cross-language test vectors: lowercase, unprefixed,
// even-length — the convention `proto/test-vectors/` uses for every byte-string
// field.
//
// ---------------------------------------------------------------------------
// SINGLE HOME — READ THIS BEFORE WRITING YOUR OWN
// ---------------------------------------------------------------------------
//
// This is the only hex implementation under `media/frame/**`, and the only
// NAMED one in `sdk-core`. It is NOT the only one in the package:
// `src/session/MeetingSession.ts` (`meetingIdHash`) carries an unnamed inline
// byte->hex loop that is character-for-character the body of `bytesToHex`
// below. That duplication predates this file and is untouched here; it is
// named rather than glossed because the earlier draft of this notice claimed
// to be the only implementation, and a reader who believes that has a reason
// NOT to look — which is worse than no claim, since the sentence written to
// prevent a second home would have redirected the scrutiny that finds the
// existing one. (Found by @dry-reviewer at Gate 3; a Gate-1 sweep for the
// NAMED helpers `toHex|fromHex|hexToBytes|bytesToHex` could not see an inline
// loop, which is the form this duplication almost always takes, because nobody
// extracts four lines until a second caller appears.)
//
// So the promotion below has TWO consumers, not one: when this moves to `src/`,
// fold `meetingIdHash`'s loop into it. Tracked in `docs/TODO.md`
// §Cross-Service Duplication — deliberately not fixed now, because unifying
// them today would require promoting this file to `src/` early, which is
// exactly what the task-8 scope ruling forbids.
//
// It sits in the test tree rather than `src/` because at story task 8 there is
// no production consumer:
// the TypeScript v2 codec lands at story task 15. A `src/` module now would be
// production code with zero production callers, inside a directory the task-8
// cross-boundary table deliberately excludes.
//
// When task 15's codec needs hex, **MOVE this file to `src/` — do not copy it.**
//
// A copy would produce two implementations that agree forever and diverge only
// when someone edits one. Their agreement is precisely what makes the
// duplication invisible: no test fails, nothing is observably wrong, and it
// surfaces only if a reviewer happens to look. Most duplication announces
// itself by drifting; this kind does not.
//
// This notice lives here, and not only in the devloop output, because the
// person who would create the second home is whoever writes task 15's codec,
// reaches for hex, does not find it in `src/`, and writes four fresh lines.
// They have no reason to open a devloop output. A deferral list is a record for
// people auditing the task; it is not a control on the person who would violate
// it.
//
// Tracked as a task-15 obligation ("promote, do not duplicate", this exact
// path) in
// `docs/devloop-outputs/2026-09-02-frame-v2-cross-language-vectors/main.md`.

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
    throw new Error(
      `${what}: not canonical vector hex (lowercase, unprefixed, [0-9a-f] only): ${JSON.stringify(
        hex.slice(0, 32),
      )}`,
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
