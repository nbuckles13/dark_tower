// File: packages/sdk-core/src/media/frame/__tests__/frameVectors.ts
//
// Loader for `proto/test-vectors/frame-v2.vectors.json`, the cross-language SSoT
// for the ADR-0036 §2 frame format.
//
// TEST TIER ONLY. Nothing under `src/` outside `__tests__/` may reach this file
// or the JSON it reads: the file is 94 KB, carries a `_non_production` banner,
// and every row holds `kek_hex`, `transmit_key_hex` and
// `identity_private_seed_hex`. `vite.config.ts` externalises only
// `/^@opentelemetry\//`, so an import from production would bundle the fixtures
// — key material included — into `dist/`. `tests/bundle-content.test.ts` asserts
// they are absent from the built artifacts.
//
// Production reads the same numbers through `../wireConstants.js`, which is
// RENDERED from this file at build time and byte-compared by
// `wireConstants.drift.test.ts`.

import { readFileSync } from 'node:fs';
import { join } from 'node:path';

import { repoRoot } from './repoRoot.js';

/** Path of the SSoT relative to the repository root. */
export const VECTORS_PATH = 'proto/test-vectors/frame-v2.vectors.json';

/** Decoded flag bits, as the vectors spell them. */
export interface VectorFlags {
  readonly independently_decodable: boolean;
  readonly discardable: boolean;
  readonly key_bearing: boolean;
}

/** One row's `decoded` block. */
export interface VectorDecoded {
  readonly version: number;
  readonly flags: VectorFlags;
  readonly payload_length: number;
  readonly stream_sequence: number;
  readonly kek_generation?: number;
  readonly stream_id: number;
  readonly hop_sequence: number;
  readonly extensions: readonly { readonly ext_type: number; readonly value_hex: string }[];
  readonly key_id: string;
  readonly key_id_decomposed: {
    readonly sender_id: number;
    readonly stream: number;
    readonly generation: string;
  };
}

/** One row's `derived` block: the spans and schedule outputs. */
export interface VectorDerived {
  readonly signed_input_hex: string;
  readonly aead_aad_hex: string;
  /** The pinned near-miss: the whole pre-signature frame, relay region INCLUDED. */
  readonly naive_contiguous_signed_input_hex: string;
  readonly sframe_nonce_hex: string;
  readonly sframe_key_hex: string;
  readonly sframe_salt_hex: string;
  readonly payload_tag_hex: string;
  readonly payload_ciphertext_hex: string;
  readonly signature_hex: string;
  readonly wrap_nonce_hex?: string;
  readonly wrap_aad_hex?: string;
}

/** One row's `crypto` block. Synthetic fixtures; guard g13 enforces the pattern. */
export interface VectorCrypto {
  readonly identity_public_hex: string;
  readonly identity_private_seed_hex: string;
  readonly kek_hex: string;
  readonly transmit_key_hex: string;
  readonly plaintext_hex: string;
}

/** Declarative receiver state a row needs before it can be exercised. */
export interface ReceiverPrecondition {
  readonly cached_transmit_key_for_key_id: string;
  readonly primed_by: string;
  readonly prime_via: string;
  readonly replay_window_advanced: boolean;
}

/** One row's `expected` block. Shape varies by `kind`. */
export interface VectorExpected {
  readonly derived_describes?: string;
  readonly decodes?: boolean;
  readonly decrypts?: boolean;
  readonly signature_valid?: boolean;
  readonly unwraps?: boolean;
  readonly frame_dropped?: boolean;
  readonly wrap_cached?: boolean;
  readonly outcome?: string;
  readonly replay_of?: string;
  readonly verify_against_public_hex?: string;
  readonly would_decrypt_if_verification_skipped?: boolean;
  readonly use_transmit_key_hex?: string;
  readonly use_kek_hex?: string;
  readonly receiver_precondition?: ReceiverPrecondition;
}

/** One vector row. */
export interface FrameVector {
  readonly name: string;
  readonly description: string;
  readonly kind: string;
  readonly frame_hex: string;
  readonly offsets: {
    readonly publisher_region_len: number;
    readonly relay_region_offset: number;
    readonly payload_offset: number;
    readonly signature_offset: number;
  };
  readonly decoded: VectorDecoded;
  readonly derived: VectorDerived;
  readonly crypto: VectorCrypto;
  readonly reject_reason?: string;
  readonly expected?: VectorExpected;
}

/** A `reject_reasons[]` entry. */
export interface RejectReasonSpec {
  readonly token: string;
  readonly layer: 'codec' | 'crypto' | 'key';
  readonly drops_frame: boolean;
  readonly has_vector: boolean;
  readonly fleet_spelling: string | null;
  readonly spec_anchor: string | null;
}

/** The whole SSoT. */
export interface FrameVectorFile {
  readonly schema_version: number;
  readonly header_version: number;
  readonly cipher_suite_id: number;
  readonly max_payload_bytes: number;
  readonly gated_by: { readonly rust: boolean; readonly typescript: boolean };
  readonly cross_language_property_established: boolean;
  readonly wire_constants: Record<string, unknown>;
  readonly extension_registry: Record<string, unknown>;
  readonly reject_reasons: readonly RejectReasonSpec[];
  readonly key_material_fields: readonly string[];
  readonly derived_public_fields: readonly string[];
  readonly vectors: readonly FrameVector[];
}

let cached: FrameVectorFile | undefined;

/**
 * Load the SSoT.
 *
 * Does NOT skip when the file is absent. An absent SSoT and a passing conformance
 * run must not look alike — a harness that silently verifies nothing is the exact
 * failure this whole exercise exists to prevent.
 */
export function loadFrameVectors(): FrameVectorFile {
  if (cached) return cached;
  const path = join(repoRoot(), VECTORS_PATH);
  let text: string;
  try {
    text = readFileSync(path, 'utf8');
  } catch {
    throw new Error(
      `frame-v2 vectors missing at ${path}. This gate does not skip when its input is absent. ` +
        `Regenerate with: cargo run -p media-vector-gen --bin generate-frame-vectors`,
    );
  }
  cached = JSON.parse(text) as FrameVectorFile;
  return cached;
}

/** Look a row up by name, failing loudly if it is gone. */
export function rowByName(file: FrameVectorFile, name: string): FrameVector {
  const found = file.vectors.find((v) => v.name === name);
  if (!found) {
    throw new Error(
      `vector row '${name}' is absent. A row referenced by name and no longer present means the ` +
        `assertion built on it silently stopped running.`,
    );
  }
  return found;
}

/**
 * Whether a row's `derived`/`decoded` blocks describe its own `frame_hex`.
 *
 * On a mutated row they describe the well-formed BASE frame instead, which the
 * row declares via `expected.derived_describes`. Comparing a reject row's derived
 * block against its own bytes would pass vacuously — the block was never
 * recomputed for the mutation.
 */
export function derivedDescribesOwnFrame(row: FrameVector): boolean {
  return row.expected?.derived_describes !== 'base_frame_before_mutation';
}
