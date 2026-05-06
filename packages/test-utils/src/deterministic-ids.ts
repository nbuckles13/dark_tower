// File: packages/test-utils/src/deterministic-ids.ts
//
// Seeded NON-cryptographic RNG. UUIDs produced here are reproducible-by-design,
// NOT unpredictable. RFC 4122 §4.4 byte-layout (variant + version bits set)
// for shape conformance only. Test-only.

/**
 * Mulberry32 PRNG. Public-domain ~5-line generator. Returns a function that
 * yields a Number in [0, 1) on each call.
 *
 * NOT cryptographically secure. Identical seeds produce identical sequences;
 * that is the entire point.
 */
export function createSeededRng(seed: string | number): () => number {
  let state = typeof seed === 'number' ? seed >>> 0 : hashStringToUint32(seed);
  return function mulberry32(): number {
    state = (state + 0x6d2b79f5) >>> 0;
    let t = state;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

/**
 * Returns a function that yields RFC 4122 §4.4-shaped UUIDv4 strings driven
 * by the supplied RNG. NOT cryptographic — for reproducible test IDs only.
 *
 * Shape conformance: variant bits (10xx) and version nibble (4) are set per
 * the spec so the output is parseable as a v4 UUID by ordinary tools.
 */
export function createSeededUuid(rng: () => number): () => string {
  return function nextUuid(): string {
    const bytes = new Uint8Array(16);
    for (let i = 0; i < 16; i++) {
      bytes[i] = Math.floor(rng() * 256) & 0xff;
    }
    // Set version (4) and variant (10xx) bits per RFC 4122 §4.4.
    bytes[6] = ((bytes[6] ?? 0) & 0x0f) | 0x40;
    bytes[8] = ((bytes[8] ?? 0) & 0x3f) | 0x80;
    return formatUuid(bytes);
  };
}

/**
 * Convenience factory: one seed string yields a paired `(rng, uuid)` factory.
 *
 * @example
 * const ids = createIdFactory('test-seed-1');
 * const a = ids.uuid(); // always the same value for the same seed
 * const r = ids.rng();  // a number in [0, 1)
 */
export function createIdFactory(seed: string): { uuid: () => string; rng: () => number } {
  const rng = createSeededRng(seed);
  const uuid = createSeededUuid(rng);
  return { uuid, rng };
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

function hashStringToUint32(s: string): number {
  // FNV-1a 32-bit. Stable across platforms; not crypto-grade.
  let hash = 0x811c9dc5;
  for (let i = 0; i < s.length; i++) {
    hash ^= s.charCodeAt(i);
    hash = Math.imul(hash, 0x01000193);
  }
  return hash >>> 0;
}

function formatUuid(bytes: Uint8Array): string {
  const hex: string[] = [];
  for (let i = 0; i < 16; i++) {
    hex.push((bytes[i] ?? 0).toString(16).padStart(2, '0'));
  }
  return (
    hex.slice(0, 4).join('') +
    '-' +
    hex.slice(4, 6).join('') +
    '-' +
    hex.slice(6, 8).join('') +
    '-' +
    hex.slice(8, 10).join('') +
    '-' +
    hex.slice(10, 16).join('')
  );
}
