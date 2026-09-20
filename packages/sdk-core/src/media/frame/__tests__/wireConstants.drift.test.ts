// File: packages/sdk-core/src/media/frame/__tests__/wireConstants.drift.test.ts
//
// The TypeScript half of the cross-language constant chain.
//
//   crates/media-protocol/src/frame.rs        <- origin
//        |  guard g2 / g3 / g4
//        v
//   proto/test-vectors/frame-v2.vectors.json
//        |  THIS TEST (re-render + byte-compare)
//        v
//   src/media/frame/wireConstants.ts
//
// Re-renders the generated module in memory and byte-compares against the
// committed file — the shape of `crates/media-vector-gen/tests/vectors_are_current.rs`.
//
// TWO CHECKS, AND THEY CATCH DIFFERENT THINGS — do not delete either as
// redundant to the other:
//
//   * The whole-file byte-compare below catches DRIFT: a committed file that no
//     longer matches a fresh render (a hand edit, a stale regen).
//   * The `Object.entries(file.wire_constants)` loop lower down catches
//     ADDITIONS that the byte-compare CANNOT — a new SSoT key with no TypeScript
//     mirror is absent from both committed and freshly-rendered output, so they
//     byte-match and the compare stays green. @dry-reviewer verified this
//     empirically. The renderer's own exhaustiveness check now throws on such a
//     key at render time, and this loop is the test-tier assertion of the same
//     property. It is LOAD-BEARING, not supplementary.

import { readFileSync } from 'node:fs';
import { join } from 'node:path';

import { describe, expect, it } from 'vitest';

import { repoRoot } from './repoRoot.js';
import { loadFrameVectors } from './frameVectors.js';
import {
  WIRE_CONSTANTS,
  HEADER_VERSION,
  MAX_PAYLOAD_BYTES,
  CIPHER_SUITE_ID,
} from '../wireConstants.js';

// The renderer is plain ESM outside `src/` — imported directly rather than
// shelled out, so a failure is a stack trace rather than an exit code.
const generator = (await import(
  join(repoRoot(), 'packages/sdk-core/scripts/gen-wire-constants.mjs')
)) as {
  renderWireConstants: (v: unknown) => Promise<string>;
  loadVectors: () => unknown;
  OUTPUT_PATH: string;
  REGEN_COMMAND: string;
};

describe('wireConstants.ts is a faithful render of the SSoT', () => {
  it('matches a fresh render, byte for byte', async () => {
    const committed = readFileSync(join(repoRoot(), generator.OUTPUT_PATH), 'utf8');
    const rendered = await generator.renderWireConstants(generator.loadVectors());

    if (committed !== rendered) {
      // The failure message carries the command, deliberately. A drift test that
      // says only "files differ" costs twenty minutes at 3am.
      const at = [...committed].findIndex((c, i) => c !== rendered[i]);
      throw new Error(
        `${generator.OUTPUT_PATH} is not a faithful render of ` +
          `proto/test-vectors/frame-v2.vectors.json (first difference at byte ${at}).\n\n` +
          `Regenerate with:\n    ${generator.REGEN_COMMAND}\n\n` +
          `If you edited ${generator.OUTPUT_PATH} by hand: DO NOT re-render to make this pass — ` +
          `that discards your edit silently. The value's origin is ` +
          `crates/media-protocol/src/frame.rs; change it there, regenerate the vectors, then ` +
          `regenerate this file.\n\n` +
          `Ruling out fmt (this file is deliberately kept IN the prettier gate): check ` +
          `\${DEVLOOP_TMP}/layer-2.log. If the fmt run shows FMT_MODE=check (any SOURCE), fmt is ` +
          `EXCLUDED — check mode never writes, so stale content is the cause; regenerate as above. ` +
          `Only FMT_MODE=apply with this file listed in FMT_APPLIED= implicates fmt (the ` +
          `renderer↔prettier convergence broke — a prettier/plugin version or config split); ` +
          `reconcile them, don't just regenerate.`,
      );
    }
    expect(committed).toBe(rendered);
  });

  it('carries the GENERATED banner and the regenerate command', () => {
    const committed = readFileSync(join(repoRoot(), generator.OUTPUT_PATH), 'utf8');
    expect(committed).toContain('GENERATED — DO NOT EDIT');
    expect(committed).toContain(generator.REGEN_COMMAND);
    // The full SSoT chain must be recorded IN the file: someone opening the leaf
    // to change a number has to be told the change belongs at the far end in Rust.
    expect(committed).toContain('crates/media-protocol/src/frame.rs');
  });
});

describe('the rendered constants agree with the SSoT', () => {
  const file = loadFrameVectors();

  it('mirrors every scalar the vectors declare (LOAD-BEARING: the additions check)', () => {
    // This loop iterates the SSoT and asserts each key exists in the rendered
    // module. It is the test-tier half of the additions guarantee — see the file
    // header. Do NOT delete it as redundant to the byte-compare; the byte-compare
    // does not cover a key the SSoT adds.
    expect(HEADER_VERSION).toBe(file.header_version);
    expect(MAX_PAYLOAD_BYTES).toBe(file.max_payload_bytes);
    expect(CIPHER_SUITE_ID).toBe(file.cipher_suite_id);
    for (const [key, value] of Object.entries(file.wire_constants)) {
      if (key === 'key_id_layout') continue;
      expect(
        (WIRE_CONSTANTS as unknown as Record<string, number>)[key],
        `wire_constants.${key} is in the SSoT but not in the rendered module`,
      ).toBe(value);
    }
  });

  it('the wrapped-key block widths sum to the block size', () => {
    // The cross-check `crates/media-protocol/src/frame.rs:211` already makes on
    // the Rust side, mirrored here now that all three widths render from the same
    // source. This is why the 32 is never derived by subtracting the tag width
    // from the block width: that would make the transmit-key length a function of
    // the wrap-tag length and collapse two constants that are free to move apart.
    expect(
      WIRE_CONSTANTS.kek_generation_field_bytes +
        WIRE_CONSTANTS.wrapped_transmit_key_material_bytes +
        WIRE_CONSTANTS.aead_tag_bytes,
    ).toBe(WIRE_CONSTANTS.wrapped_transmit_key_bytes);
  });

  it('the key-id field widths exactly fill the key id', () => {
    const { sender_id_bits, stream_bits, generation_bits } = WIRE_CONSTANTS.key_id_layout;
    // A gap would be undecoded bits the nonce derivation depends on; an overlap
    // would alias two key ids onto one, which is a nonce collision.
    expect(sender_id_bits + stream_bits + generation_bits).toBe(WIRE_CONSTANTS.key_id_bytes * 8);
  });
});
