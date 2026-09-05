// File: packages/sdk-core/scripts/gen-wire-constants.mjs
//
// Renders `src/media/frame/wireConstants.ts` from the cross-language SSoT,
// `proto/test-vectors/frame-v2.vectors.json`.
//
// ---------------------------------------------------------------------------
// WHY A RENDERER RATHER THAN A HAND-WRITTEN CONSTANT WITH A DRIFT ASSERTION
// ---------------------------------------------------------------------------
//
// Both shapes catch DRIFT. The ADDITIONS direction — a new `wire_constants` key
// in the SSoT with no TypeScript mirror — is caught by the EXHAUSTIVENESS check
// in `renderWireConstants` (a key not in `SCALAR_KEYS` throws at render time),
// NOT by the byte-compare: a byte-compare of committed-vs-fresh-render stays
// green on an addition, because the unmirrored key is absent from both. That
// distinction was verified empirically by @dry-reviewer and is stated here
// because getting it wrong invites a future reader to delete the load-bearing
// check as redundant.
//
// The whole point of a rendered module rather than hand-declared constants is
// that there is no hand-authored second copy of any of these numbers in
// TypeScript: the generated module is provably a function of the SSoT rather
// than a relocation of a literal into a third file.
//
// ---------------------------------------------------------------------------
// WHY THIS SCRIPT LIVES OUTSIDE `src/`
// ---------------------------------------------------------------------------
//
// It reads `proto/test-vectors/frame-v2.vectors.json` with `node:fs`. That file
// is 94 KB, carries a `_non_production` banner, and every one of its rows holds
// `kek_hex`, `transmit_key_hex` and `identity_private_seed_hex`. Nothing under
// `src/` may reach it: `vite.config.ts` externalises only `/^@opentelemetry\//`,
// so an import from `src/` would bundle the fixtures — including the key
// material — into `dist/index.mjs` and `dist/index.cjs`.
//
// Being outside `src/` also keeps it out of `tsconfig.build.json` and the `dts`
// plugin's `include`, so the declaration build never picks it up.
//
// Run: `node packages/sdk-core/scripts/gen-wire-constants.mjs --write`
// Check: `__tests__/wireConstants.drift.test.ts` re-renders in memory and
// byte-compares against the committed file.

import { readFileSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

import * as prettier from 'prettier';

/** Path of the SSoT, relative to the repository root. */
export const VECTORS_PATH = 'proto/test-vectors/frame-v2.vectors.json';
/** Path of the rendered module, relative to the repository root. */
export const OUTPUT_PATH = 'packages/sdk-core/src/media/frame/wireConstants.ts';
/** The command a reader must run to regenerate. Quoted verbatim in the output. */
export const REGEN_COMMAND = 'node packages/sdk-core/scripts/gen-wire-constants.mjs --write';

/**
 * Locate the repository root by walking up to `pnpm-workspace.yaml`.
 *
 * DELIBERATELY DUPLICATED with the test-tier home at
 * `src/media/frame/__tests__/repoRoot.ts` (character-identical). Not unified
 * because this file must run under bare `node` with no TypeScript loader, and
 * that `.ts` is what locates THIS file at test time — so neither can import the
 * other without a types shim for a 12-line function. The boundary is recorded at
 * both sites rather than claimed away. Anchored on a marker file rather than a
 * fixed `../../..` count, so moving this script one directory does not silently
 * resolve to the wrong tree.
 */
export function repoRoot(from = dirname(fileURLToPath(import.meta.url))) {
  let dir = from;
  for (let i = 0; i < 12; i += 1) {
    try {
      readFileSync(join(dir, 'pnpm-workspace.yaml'));
      return dir;
    } catch {
      const parent = dirname(dir);
      if (parent === dir) break;
      dir = parent;
    }
  }
  throw new Error('could not locate repository root (no pnpm-workspace.yaml above this file)');
}

/**
 * The keys this renderer emits, in order.
 *
 * DECLARED, never discovered from the JSON. A renderer that emitted whatever
 * keys it happened to find would silently shrink if the SSoT lost one — and a
 * shrunken render still byte-matches a shrunken commit, so the drift test would
 * agree with it. Declaring the list means a removed key throws here instead.
 */
const SCALAR_KEYS = [
  'legal_flag_mask',
  'signature_bytes',
  'aead_tag_bytes',
  'key_id_bytes',
  'relay_region_bytes',
  'publisher_fixed_prefix_bytes',
  'wrapped_transmit_key_bytes',
  'kek_generation_field_bytes',
  'wrapped_transmit_key_material_bytes',
  'ext_length_field_bytes',
  'max_ext_bytes',
];

const KEY_ID_LAYOUT_KEYS = ['sender_id_bits', 'stream_bits', 'generation_bits'];

function requireNumber(obj, key, where) {
  const value = obj?.[key];
  if (typeof value !== 'number' || !Number.isInteger(value)) {
    throw new Error(
      `${VECTORS_PATH}: ${where}.${key} is missing or not an integer (got ${JSON.stringify(value)}). ` +
        `The renderer declares its key list rather than discovering it, so a removed or renamed ` +
        `key fails here instead of silently shrinking the rendered module.`,
    );
  }
  return value;
}

/**
 * Render the TypeScript module source for `vectors`.
 *
 * Pure: takes the parsed SSoT, returns a string. The drift test calls this
 * directly rather than shelling out.
 */
export async function renderWireConstants(vectors) {
  const wc = vectors.wire_constants;
  if (!wc) throw new Error(`${VECTORS_PATH}: wire_constants is absent`);

  // EXHAUSTIVENESS: the declared list must also be COMPLETE. `SCALAR_KEYS` is
  // declared, not discovered, which is what makes REMOVALS red (a dropped key
  // fails `requireNumber` below). But a declared list says nothing about a key
  // the SSoT ADDS — that key is simply not rendered, so the committed file and a
  // fresh render agree and the byte-compare stays green. @dry-reviewer ran
  // exactly this and caught it. This check is what actually closes the additions
  // direction: a new `wire_constants` key with no mirror throws HERE, at the
  // render site, and the drift test inherits the throw. It is not the
  // byte-compare that catches additions; it is this.
  const mirrored = new Set([...SCALAR_KEYS, 'key_id_layout']);
  const unmirrored = Object.keys(wc).filter((k) => !mirrored.has(k));
  if (unmirrored.length > 0) {
    throw new Error(
      `${VECTORS_PATH}: wire_constants has key(s) with no TypeScript mirror: ` +
        `${unmirrored.join(', ')}. Add them to SCALAR_KEYS in this renderer and regenerate.`,
    );
  }

  const scalars = SCALAR_KEYS.map((k) => `  ${k}: ${requireNumber(wc, k, 'wire_constants')},`);
  const layout = KEY_ID_LAYOUT_KEYS.map(
    (k) => `    ${k}: ${requireNumber(wc.key_id_layout, k, 'wire_constants.key_id_layout')},`,
  );

  const registry = vectors.extension_registry;
  if (!registry?.entries?.length) {
    throw new Error(`${VECTORS_PATH}: extension_registry.entries is absent or empty`);
  }
  const entries = registry.entries.map(
    (e) =>
      `    {\n      ext_type: ${e.ext_type},\n      value_len: ${e.value_len},\n` +
      `      accepted_min: ${e.accepted_min},\n      accepted_max: ${e.accepted_max},\n    },`,
  );
  for (const flag of [
    'unknown_type_is_rejected',
    'duplicate_type_is_rejected',
    'ascending_type_order_required',
    'value_outside_accepted_set_is_rejected',
  ]) {
    if (registry[flag] !== true) {
      throw new Error(
        `${VECTORS_PATH}: extension_registry.${flag} is not true. The rejection semantics travel ` +
          `with the table: a decoder could pin an identical registry and still SKIP unknown types, ` +
          `which is the ADR-0036 §2 defect behind a perfectly matching table.`,
      );
    }
  }

  const source = `// File: ${OUTPUT_PATH}
//
// GENERATED — DO NOT EDIT.
//
//   Renderer: packages/sdk-core/scripts/gen-wire-constants.mjs
//   Source:   ${VECTORS_PATH}
//   Regenerate: ${REGEN_COMMAND}
//
// ---------------------------------------------------------------------------
// THE CHANGE YOU WANT TO MAKE ALMOST CERTAINLY BELONGS AT THE FAR END OF THIS
// CHAIN, IN RUST. READ THIS BEFORE EDITING A NUMBER BELOW.
// ---------------------------------------------------------------------------
//
//   crates/media-protocol/src/frame.rs        <- the origin; the constants are declared here
//        |  guard g2 / g3 / g4 (scripts/guards/simple/validate-frame-vectors.sh)
//        v
//   ${VECTORS_PATH}
//        |  __tests__/wireConstants.drift.test.ts  (re-render + byte-compare)
//        v
//   this file
//
// Every hop is guarded, which is what makes three hops acceptable rather than
// three drift surfaces. Editing THIS file makes the byte-compare red, and the
// tempting repair — re-rendering afterwards — silently discards the edit. Change
// the Rust constant, regenerate the vectors, then regenerate this.
//
// ---------------------------------------------------------------------------
// PROPERTY NAMES ARE THE SSoT's OWN SPELLINGS, ON PURPOSE
// ---------------------------------------------------------------------------
//
// \`snake_case\` rather than \`SCREAMING_SNAKE\` so a consumer site reads
// \`WIRE_CONSTANTS.max_payload_bytes\` — the exact key name in the source file.
// That keeps the traceability visible at the call site, and it is what guard
// g12 greps for in the two enumerated production sites (\`frameCodec.ts\`,
// \`sframe.ts\`). Renaming these to TypeScript casing would break that link.

/** Frame header version. ADR-0036 §2; mirrors \`frame.rs::PROTOCOL_VERSION\`. */
export const HEADER_VERSION = ${requireNumber(vectors, 'header_version', 'root')};

/**
 * Maximum payload length, enforced BEFORE the payload slice is taken.
 *
 * ADR-0036 §2 calls the length field a parsing trust boundary: a reader that
 * pre-allocates from this attacker-controlled field is the defect. Cross-language
 * SSoT — guard g2 pins it against \`frame.rs::MAX_PAYLOAD_BYTES\`.
 */
export const MAX_PAYLOAD_BYTES = ${requireNumber(vectors, 'max_payload_bytes', 'root')};

/**
 * RFC 9605 ciphersuite id. \`AES_256_GCM_SHA512_128\` (RFC 9605 §8.1), which
 * ADR-0036 §4 selects. The suite fixes the hash, so HKDF is SHA-512 throughout
 * and the extract PRK is 64 bytes, not 32.
 */
export const CIPHER_SUITE_ID = ${requireNumber(vectors, 'cipher_suite_id', 'root')};

/** Wire field widths and region sizes, keyed by their SSoT spellings. */
export const WIRE_CONSTANTS = {
${scalars.join('\n')}
  max_payload_bytes: ${requireNumber(vectors, 'max_payload_bytes', 'root')},
  cipher_suite_id: ${requireNumber(vectors, 'cipher_suite_id', 'root')},
  key_id_layout: {
${layout.join('\n')}
  },
} as const;

/**
 * The publisher-set TLV registry.
 *
 * The rejection semantics travel WITH the table, not just the entries: a decoder
 * could pin an identical registry and still skip unknown types, which is the
 * ADR-0036 §2 defect behind a perfectly matching table.
 */
export const EXTENSION_REGISTRY = {
  entries: [
${entries.join('\n')}
  ],
  unknown_type_is_rejected: true,
  duplicate_type_is_rejected: true,
  ascending_type_order_required: true,
  value_outside_accepted_set_is_rejected: true,
} as const;
`;

  // Formatted by the renderer, NOT excluded via `.prettierignore`.
  //
  // `packages/sdk-core` lint runs `prettier --check "src/**/*.ts"` and this file
  // lands under `src/`. If the render's formatting differed from prettier's by
  // so much as a quote style, `prettier --check` would red — and running
  // `prettier --write` would then red the byte-compare. Two gates demanding
  // different bytes of one file is an unresolvable red that gets "fixed" by
  // deleting one of them, and the one that would go is the formatting check on
  // a file that ships.
  return prettier.format(source, {
    ...(await prettier.resolveConfig(join(repoRoot(), OUTPUT_PATH))),
    filepath: OUTPUT_PATH,
  });
}

/** Read and parse the SSoT. */
export function loadVectors(root = repoRoot()) {
  const path = join(root, VECTORS_PATH);
  try {
    return JSON.parse(readFileSync(path, 'utf8'));
  } catch (err) {
    throw new Error(`cannot read the frame-vector SSoT at ${path}: ${err.message}`);
  }
}

async function main() {
  const root = repoRoot();
  const rendered = await renderWireConstants(loadVectors(root));
  const out = join(root, OUTPUT_PATH);
  if (process.argv.includes('--write')) {
    writeFileSync(out, rendered);
    process.stdout.write(`wrote ${OUTPUT_PATH}\n`);
  } else {
    process.stdout.write(rendered);
  }
}

if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) {
  await main();
}
