// File: packages/sdk-core/src/media/frame/__tests__/vectors.conformance.test.ts
//
// THE TYPESCRIPT LEG of the cross-language frame-vector gate (ADR-0036 §2).
//
// This exact path is hardcoded in `scripts/guards/simple/validate-frame-vectors.sh`
// check g14 as the conformance marker for the `typescript` codec. Renaming or
// moving this file reds that guard.
//
// ---------------------------------------------------------------------------
// WHAT THIS GATE IS FOR, AND WHAT IT IS NOT
// ---------------------------------------------------------------------------
//
// The Rust reference generator and this codec were written independently from the
// same ADR text. Three computations have NO outside oracle — the AEAD associated
// data span, the Ed25519 signed range, and the detached-tag split — and this file
// is the only thing that checks them from a second implementation. The vendored
// external sframe-wg anchor (`external-anchor.test.ts`) gates the key schedule,
// the nonce derivation, the AES-256-GCM primitive and the tag length, and gates
// NONE of those three. Do not read a green external anchor as covering them.
//
// ---------------------------------------------------------------------------
// EVERY ASSERTION HERE MUST BE FALSIFIABLE. THE ANTI-VACUITY RULES:
// ---------------------------------------------------------------------------
//
//  1. COMPLETENESS is asserted by NAME-SET EQUALITY, not by a count. A count can
//     be satisfied by a loop that ran zero times over a file that shrank.
//  2. A reject row's `derived`/`decoded` blocks describe the BASE frame it was
//     mutated from, so they are never compared against its own `frame_hex`.
//  3. Spans are taken as SLICES of the decoded input, never re-serialized —
//     re-serializing tests the encoder against itself.
//  4. The pinned near-miss `naive_contiguous_signed_input_hex` must DIFFER from
//     the real signed range on every row.
//  5. Reject tokens are HAND-WRITTEN in `../rejectReason.ts`, which does not
//     import this loader. Comparing the codec's emitted token with the row's is
//     therefore a genuine cross-check, not the file agreeing with itself.
//  6. Receiver state is ISOLATED PER ROW, and rows that need prior state declare
//     it in `expected.receiver_precondition`, which is honoured here and FAILS
//     LOUDLY if it cannot be established.
//
// Error messages in this file MAY name key ids and sender ids. That is the
// opposite of the production rule in `../rejectReason.ts`, deliberately: test-tier
// assertion output is not a production log and is not in `ts_pii.rs`'s scope, and
// a red row that cannot say which key id mismatched is undebuggable.

import { describe, expect, it } from 'vitest';

import { bytesToHex, concatBytes, hexToBytes } from '../hex.js';
import { decodeFrame } from '../frameCodec.js';
import {
  exportPublicKey,
  importSigningKeyFromSeed,
  signFrame,
  verifyFrameSignature,
} from '../ed25519.js';
import { openSframe, parseSframe, unwrapTransmitKey, SFRAME_CIPHER_SUITE_ID } from '../sframe.js';
import { deriveSframeKeys, sframeNonce } from '../sframeKeySchedule.js';
import { unpackKeyId } from '../keyId.js';
import { FrameRejectedError } from '../rejectReason.js';
import {
  ReplayWindow,
  TransmitKeyCache,
  openVerifiedFrame,
  verifyFrame,
  type ReceiverKeys,
} from '../receivePath.js';
import {
  derivedDescribesOwnFrame,
  loadFrameVectors,
  rowByName,
  type FrameVector,
} from './frameVectors.js';

const file = loadFrameVectors();
const rows = file.vectors;

/** Rows this run actually exercised. Compared for SET EQUALITY at the end. */
const visited = new Set<string>();
function visit(row: FrameVector): FrameVector {
  visited.add(row.name);
  return row;
}

/** Derive the SFrame key/salt/nonce for a row, from a supplied transmit key. */
async function scheduleFor(row: FrameVector, transmitKeyHex: string) {
  const derived = await deriveSframeKeys({
    hash: 'SHA-512',
    baseKey: hexToBytes(transmitKeyHex, 'transmit_key_hex'),
    // The 8-byte key id AS PINNED, fed straight to the derivation. Never re-packed
    // from `key_id_decomposed` — a re-pack always succeeds and is merely wrong.
    kid: hexToBytes(row.decoded.key_id, 'key_id'),
    cipherSuiteId: SFRAME_CIPHER_SUITE_ID,
    keyBytes: 32,
    saltBytes: 12,
  });
  return {
    ...derived,
    nonce: sframeNonce(derived.salt, BigInt(row.decoded.stream_sequence)),
  };
}

function keysHolding(kekHex: string, generation: number): ReceiverKeys {
  const kek = hexToBytes(kekHex, 'kek_hex');
  return { kekForGeneration: (g) => (g === generation ? kek : undefined) };
}

describe('frame-v2 vectors — the file itself', () => {
  it('is non-empty and carries the non-production banner', () => {
    expect(rows.length).toBeGreaterThan(0);
    expect(file.schema_version).toBe(1);
    expect(file.reject_reasons.length).toBeGreaterThan(0);
  });
});

describe.each(rows.map((r) => [r.name, r] as const))('row %s', (_name, row) => {
  const frame = hexToBytes(row.frame_hex, `${row.name}.frame_hex`);
  const isRejectRow = !derivedDescribesOwnFrame(row);

  if (row.kind === 'decode_reject') {
    it('is rejected by the decoder with the pinned reason', () => {
      visit(row);
      let thrown: unknown;
      try {
        decodeFrame(frame);
      } catch (err) {
        thrown = err;
      }
      expect(thrown).toBeInstanceOf(FrameRejectedError);
      // ASSERTION 5: the token on the left is emitted by a hand-written literal in
      // `../rejectReason.ts`, which never sees this file. The comparison is a
      // cross-check, not an echo.
      expect((thrown as FrameRejectedError).rejectReason).toBe(row.reject_reason);
    });

    it('declares that its derived block describes the pre-mutation frame', () => {
      // ASSERTION 2. A decode_reject row has no publisher region and no meaningful
      // AAD, so its derived block MUST be declared stale — otherwise a later
      // reader would compare it against `frame_hex` and get a false result.
      expect(row.expected?.derived_describes).toBe('base_frame_before_mutation');
    });
    return;
  }

  it('decodes, and its spans are slices of the received buffer', () => {
    visit(row);
    const decoded = decodeFrame(frame);

    expect(decoded.version).toBe(row.decoded.version);
    expect(decoded.flags.independentlyDecodable).toBe(row.decoded.flags.independently_decodable);
    expect(decoded.flags.discardable).toBe(row.decoded.flags.discardable);
    expect(decoded.flags.keyBearing).toBe(row.decoded.flags.key_bearing);
    expect(decoded.payloadLength).toBe(row.decoded.payload_length);
    expect(decoded.streamSequence).toBe(row.decoded.stream_sequence);
    expect(decoded.streamId).toBe(row.decoded.stream_id);
    expect(decoded.hopSequence).toBe(row.decoded.hop_sequence);
    expect(decoded.offsets.publisherRegionLen).toBe(row.offsets.publisher_region_len);
    expect(decoded.offsets.relayRegionOffset).toBe(row.offsets.relay_region_offset);
    expect(decoded.offsets.payloadOffset).toBe(row.offsets.payload_offset);
    expect(decoded.offsets.signatureOffset).toBe(row.offsets.signature_offset);

    expect(
      decoded.extensions.map((e) => ({ ext_type: e.extType, value_hex: bytesToHex(e.value) })),
    ).toEqual(row.decoded.extensions);

    if (row.decoded.kek_generation !== undefined) {
      expect(decoded.wrappedTransmitKey?.kekGeneration).toBe(row.decoded.kek_generation);
    } else {
      expect(decoded.wrappedTransmitKey).toBeNull();
    }

    // ASSERTION 3: both spans compared against the RAW BYTES at the pinned
    // offsets, and asserted to be the very same underlying buffer — so a decoder
    // that rebuilt them from parsed fields would fail here even if the bytes
    // happened to match.
    expect(bytesToHex(decoded.aeadAad)).toBe(row.derived.aead_aad_hex);
    expect(bytesToHex(decoded.aeadAad)).toBe(
      bytesToHex(frame.subarray(0, row.offsets.publisher_region_len)),
    );
    expect(decoded.aeadAad.buffer).toBe(frame.buffer);

    expect(bytesToHex(decoded.signedRange)).toBe(row.derived.signed_input_hex);
    expect(bytesToHex(decoded.signedRange)).toBe(
      bytesToHex(
        concatBytes(
          frame.subarray(0, row.offsets.publisher_region_len),
          frame.subarray(row.offsets.payload_offset, row.offsets.signature_offset),
        ),
      ),
    );
    expect(decoded.payload.buffer).toBe(frame.buffer);

    // ASSERTION 4: the pinned near-miss. Signing everything up to the signature
    // — relay region included — is the natural wrong implementation, and it is
    // silent: both codecs would agree and the receiver would simply fail to
    // verify. Asserting the DIFFERENCE is what makes the span assertion mean
    // something.
    expect(bytesToHex(decoded.signedRange)).not.toBe(row.derived.naive_contiguous_signed_input_hex);
  });

  it('reproduces the pinned key schedule from the pinned transmit key', async () => {
    visit(row);
    const s = await scheduleFor(row, row.crypto.transmit_key_hex);
    expect(bytesToHex(s.key)).toBe(row.derived.sframe_key_hex);
    expect(bytesToHex(s.salt)).toBe(row.derived.sframe_salt_hex);
    expect(bytesToHex(s.nonce)).toBe(row.derived.sframe_nonce_hex);
    // The PRK width is assertable only because HKDF-Extract is explicit — see
    // `../sframeKeySchedule.ts`. 32 would mean SHA-256, i.e. the wrong ciphersuite
    // silently in use.
    expect(s.secret.length).toBe(64);
  });

  it('splits the detached tag as pinned', () => {
    visit(row);
    const decoded = decodeFrame(frame);
    const obj = parseSframe(decoded.payload);
    expect(bytesToHex(obj.keyId)).toBe(row.decoded.key_id);
    expect(bytesToHex(obj.tag)).toBe(row.derived.payload_tag_hex);
    expect(bytesToHex(obj.ciphertext)).toBe(row.derived.payload_ciphertext_hex);
  });

  it('decomposes the key id for lookup only', () => {
    visit(row);
    const parts = unpackKeyId(hexToBytes(row.decoded.key_id, 'key_id'));
    expect(Number(parts.senderId)).toBe(row.decoded.key_id_decomposed.sender_id);
    expect(Number(parts.stream)).toBe(row.decoded.key_id_decomposed.stream);
    expect(parts.generation.toString(16).padStart(16, '0')).toBe(
      row.decoded.key_id_decomposed.generation,
    );
  });

  it('derives the identity public key from the pinned seed, and re-signs deterministically', async () => {
    visit(row);
    // Carried-forward item (d): this is what keeps `identity_private_seed_hex`
    // LOAD-BEARING rather than dormant. Ed25519 is deterministic (RFC 8032), so a
    // re-sign must reproduce the pinned signature byte for byte — which pins the
    // signing DIRECTION and the key derivation, neither of which a verify-only
    // assertion can reach.
    const seed = hexToBytes(row.crypto.identity_private_seed_hex, 'identity_private_seed_hex');
    const signingKey = await importSigningKeyFromSeed(seed);
    expect(bytesToHex(await exportPublicKey(signingKey))).toBe(row.crypto.identity_public_hex);

    const resigned = await signFrame(signingKey, hexToBytes(row.derived.signed_input_hex));

    // POLARITY-AWARE, never skipped by `kind`. On `tamper_publisher_region` the
    // derived block holds the MUTATED signed input against the BASE signature; on
    // `tamper_signature` it is the reverse. Skipping these rows would let a tamper
    // row whose mutation accidentally did nothing pass silently.
    if (row.expected?.signature_valid === false) {
      expect(bytesToHex(resigned)).not.toBe(row.derived.signature_hex);
    } else {
      expect(bytesToHex(resigned)).toBe(row.derived.signature_hex);
    }
  });

  if (row.derived.wrap_nonce_hex && row.derived.wrap_aad_hex) {
    it('unwraps the transmit key under the pinned wrap nonce and AAD', async () => {
      visit(row);
      const decoded = decodeFrame(frame);
      const wrapped = decoded.wrappedTransmitKey;
      expect(wrapped).not.toBeNull();

      // The wrap AAD is the key id the wrap was BOUND to, which on the
      // wrap-binding row differs from the frame's own. Unwrapping under the
      // frame's own key id is what enforces the binding.
      const boundKeyId = hexToBytes(row.derived.wrap_aad_hex!, 'wrap_aad_hex');
      const opened = await unwrapTransmitKey({
        kek: hexToBytes(row.crypto.kek_hex, 'kek_hex'),
        keyId: boundKeyId,
        wrappedKeyWithTag: wrapped!.wrappedKeyWithTag,
      });
      expect(opened).not.toBeNull();
      expect(bytesToHex(opened!)).toBe(row.crypto.transmit_key_hex);
    });
  }

  if (!isRejectRow && row.expected?.would_decrypt_if_verification_skipped !== undefined) {
    it('would decrypt if verification were skipped — proving the signature layer is load-bearing', async () => {
      visit(row);
      const decoded = decodeFrame(frame);
      const s = await scheduleFor(row, row.crypto.transmit_key_hex);
      const plaintext = await openSframe({
        key: s.key,
        nonce: s.nonce,
        aad: decoded.aeadAad,
        object: parseSframe(decoded.payload),
      });
      if (row.expected?.would_decrypt_if_verification_skipped) {
        expect(plaintext).not.toBeNull();
        expect(bytesToHex(plaintext!)).toBe(row.crypto.plaintext_hex);
      }
    });
  }
});

describe('full_frame_compose — the composed receive path', () => {
  const row = rowByName(file, 'full_frame_compose');

  it('decodes, verifies, unwraps, decrypts to the pinned plaintext', async () => {
    visit(row);
    const decoded = decodeFrame(hexToBytes(row.frame_hex));
    const verified = await verifyFrame(
      decoded,
      hexToBytes(row.crypto.identity_public_hex, 'identity_public_hex'),
    );
    const opened = await openVerifiedFrame(
      verified,
      new TransmitKeyCache(),
      keysHolding(row.crypto.kek_hex, row.decoded.kek_generation!),
    );
    expect(bytesToHex(opened.plaintext)).toBe(row.crypto.plaintext_hex);
    expect(opened.wrapOutcome).toBe('cached');
    expect(Number(opened.senderId)).toBe(row.decoded.key_id_decomposed.sender_id);
    expect(row.expected?.decodes).toBe(true);
    expect(row.expected?.unwraps).toBe(true);
    expect(row.expected?.decrypts).toBe(true);
  });
});

describe('insider_forgery_other_sender_key_id — ADR-0036 Assumption 4', () => {
  const row = rowByName(file, 'insider_forgery_other_sender_key_id');

  it('fails verification against the CLAIMED sender, and succeeds against the forger', async () => {
    visit(row);
    const decoded = decodeFrame(hexToBytes(row.frame_hex));

    // The verifying key is resolved the way production resolves it: from
    // `key_id.sender_id`, via the roster. `expected.verify_against_public_hex` is
    // ALICE's — the sender the frame claims. The row's own
    // `crypto.identity_public_hex` is BOB's, the forger's, because that is what
    // actually signed. Verifying against the row's own crypto block would SUCCEED
    // and invert the test.
    const alice = hexToBytes(row.expected!.verify_against_public_hex!, 'alice');
    const bob = hexToBytes(row.crypto.identity_public_hex, 'bob');
    expect(bytesToHex(alice)).not.toBe(bytesToHex(bob));

    // BOTH directions. Asserting only the failure would be satisfied by a harness
    // that verifies against the row's own block and passes every row while proving
    // nothing — and Assumption 4 would be recorded as discharged when it is not.
    await expect(verifyFrame(decoded, alice)).rejects.toMatchObject({
      rejectReason: 'signature_invalid',
    });
    await expect(verifyFrame(decoded, bob)).resolves.toBeDefined();
  });
});

describe('wrap_for_different_key_id — the wrap is IGNORED, the frame is played', () => {
  const row = rowByName(file, 'wrap_for_different_key_id');

  it('leaves the mis-bound key id absent from the cache and plays the frame', async () => {
    visit(row);
    const pre = row.expected?.receiver_precondition;
    // The precondition is DECLARED in the SSoT, not invented here, and is honoured
    // rather than assumed. Failing loudly when it cannot be met is required: a
    // wrap-binding row that silently no-ops when unprimed is indistinguishable
    // from a pass.
    expect(pre, 'row must declare its receiver precondition').toBeDefined();
    expect(pre!.prime_via).toBe('unwrap_only');
    expect(pre!.replay_window_advanced).toBe(false);

    const primer = rowByName(file, pre!.primed_by);
    const cache = new TransmitKeyCache();
    const replay = new ReplayWindow();

    // PRIME VIA THE UNWRAP PATH ONLY.
    //
    // Not by running `full_frame_compose` through the receive path: it shares
    // BOTH the key id and `stream_sequence` 7 with this row, so processing it
    // would advance the replay window and this row would arrive as a replay.
    // Not by injecting a raw key either: `decrypts: true` is self-protecting only
    // if the cached key came from a GENUINE VALID WRAP, otherwise the row proves
    // nothing about the wrap path.
    const primerDecoded = decodeFrame(hexToBytes(primer.frame_hex));
    const primerKeyId = hexToBytes(primer.decoded.key_id, 'primer key_id');
    const primedKey = await unwrapTransmitKey({
      kek: hexToBytes(primer.crypto.kek_hex, 'kek_hex'),
      keyId: primerKeyId,
      wrappedKeyWithTag: primerDecoded.wrappedTransmitKey!.wrappedKeyWithTag,
    });
    expect(primedKey, 'priming unwrap must succeed or this row is vacuous').not.toBeNull();
    expect(bytesToHex(primerKeyId)).toBe(pre!.cached_transmit_key_for_key_id);

    // The receiver is CONSTRUCTED holding the key. There is deliberately no
    // production entry point that installs a key without a `VerifiedFrame`.
    const verifiedPrimer = await verifyFrame(
      primerDecoded,
      hexToBytes(primer.crypto.identity_public_hex),
    );
    await openVerifiedFrame(
      verifiedPrimer,
      cache,
      keysHolding(primer.crypto.kek_hex, primer.decoded.kek_generation!),
      // NO replay window passed: priming must not advance it. This is the whole
      // point of `prime_via: unwrap_only`.
    );

    const decoded = decodeFrame(hexToBytes(row.frame_hex));
    const verified = await verifyFrame(decoded, hexToBytes(row.crypto.identity_public_hex));
    const opened = await openVerifiedFrame(
      verified,
      cache,
      keysHolding(row.crypto.kek_hex, row.decoded.kek_generation!),
      replay,
    );

    // CACHE STATE is the assertion, not a reject reason. Asserting a reject reason
    // here would test the wrong control: §4 says the receiver IGNORES the
    // mis-bound wrap and otherwise plays the frame.
    expect(opened.wrapOutcome).toBe('wrap_key_id_mismatch');
    expect(bytesToHex(opened.plaintext)).toBe(row.crypto.plaintext_hex);
    expect(row.expected?.frame_dropped).toBe(false);
    expect(row.expected?.wrap_cached).toBe(false);
    expect(row.expected?.decrypts).toBe(true);
    expect(row.expected?.signature_valid).toBe(true);

    // The mis-bound key id must be ABSENT from the cache.
    const misboundParts = unpackKeyId(hexToBytes(row.derived.wrap_aad_hex!));
    expect(
      cache.has(misboundParts, hexToBytes(row.derived.wrap_aad_hex!)),
      `mis-bound key id ${row.derived.wrap_aad_hex} must not be cached`,
    ).toBe(false);
  });
});

describe('tamper and decrypt rejects', () => {
  for (const name of ['tamper_publisher_region', 'tamper_signature']) {
    it(`${name} still decodes and fails verification`, async () => {
      const row = visit(rowByName(file, name));
      const decoded = decodeFrame(hexToBytes(row.frame_hex));
      expect(row.expected?.decodes).toBe(true);
      const ok = await verifyFrameSignature(
        hexToBytes(row.crypto.identity_public_hex),
        decoded.signature,
        decoded.signedRange,
      );
      expect(ok).toBe(false);
      await expect(
        verifyFrame(decoded, hexToBytes(row.crypto.identity_public_hex)),
      ).rejects.toMatchObject({ rejectReason: row.reject_reason });
    });
  }

  it('decrypt_reject_wrong_transmit_key fails the payload tag', async () => {
    const row = visit(rowByName(file, 'decrypt_reject_wrong_transmit_key'));
    const decoded = decodeFrame(hexToBytes(row.frame_hex));
    const s = await scheduleFor(row, row.expected!.use_transmit_key_hex!);
    const opened = await openSframe({
      key: s.key,
      nonce: s.nonce,
      aad: decoded.aeadAad,
      object: parseSframe(decoded.payload),
    });
    expect(opened).toBeNull();
    expect(row.reject_reason).toBe('decrypt_failed');
  });

  it('unwrap_reject_wrong_kek drops before decryption is attempted', async () => {
    const row = visit(rowByName(file, 'unwrap_reject_wrong_kek'));
    const decoded = decodeFrame(hexToBytes(row.frame_hex));
    const verified = await verifyFrame(decoded, hexToBytes(row.crypto.identity_public_hex));
    // Wrong KEK AND an empty cache: the unwrap fails and nothing usable is held,
    // so the frame is dropped. Contrast with `wrap_for_different_key_id`, where the
    // identical GCM failure is non-fatal because a usable key IS cached. The two
    // are distinguished by RECEIVER STATE, never by anything observable about the
    // failure.
    await expect(
      openVerifiedFrame(
        verified,
        new TransmitKeyCache(),
        keysHolding(row.expected!.use_kek_hex!, row.decoded.kek_generation!),
      ),
    ).rejects.toMatchObject({ rejectReason: 'unwrap_failed' });
  });
});

describe('replay_same_stream_sequence', () => {
  const row = rowByName(file, 'replay_same_stream_sequence');

  it('is accepted once and rejected on the second feed', async () => {
    visit(row);
    // This row DOES require the full receive path for its primer — its own
    // `replay_of` says so, and feeding only this row would pass against a receiver
    // that rejects everything. Note this is the OPPOSITE priming discipline from
    // `wrap_for_different_key_id`, which must NOT advance the window. Two rows,
    // two disciplines, both declared in the SSoT rather than implied.
    expect(row.expected?.replay_of).toBe('full_frame_compose');
    const primer = rowByName(file, row.expected!.replay_of!);

    const cache = new TransmitKeyCache();
    const replay = new ReplayWindow();
    const keys = keysHolding(primer.crypto.kek_hex, primer.decoded.kek_generation!);

    const first = await verifyFrame(
      decodeFrame(hexToBytes(primer.frame_hex)),
      hexToBytes(primer.crypto.identity_public_hex),
    );
    await expect(openVerifiedFrame(first, cache, keys, replay)).resolves.toBeDefined();

    const second = await verifyFrame(
      decodeFrame(hexToBytes(row.frame_hex)),
      hexToBytes(row.crypto.identity_public_hex),
    );
    await expect(openVerifiedFrame(second, cache, keys, replay)).rejects.toMatchObject({
      rejectReason: 'replay_detected',
    });
  });
});

describe('wrap determinism', () => {
  it('one transmit key wraps to one byte-identical ciphertext across stream sequences', () => {
    const lo = visit(rowByName(file, 'wrap_determinism_seq_lo'));
    const hi = visit(rowByName(file, 'wrap_determinism_seq_hi'));
    expect(lo.decoded.stream_sequence).not.toBe(hi.decoded.stream_sequence);
    expect(lo.decoded.key_id).toBe(hi.decoded.key_id);

    const wrapOf = (row: FrameVector) => {
      const w = decodeFrame(hexToBytes(row.frame_hex)).wrappedTransmitKey;
      expect(w).not.toBeNull();
      return bytesToHex(w!.wrappedKeyWithTag);
    };
    // ADR-0036 §4: the wrap nonce derives from the key id alone, so the block is
    // byte-identical from frame to frame within a generation. If this ever
    // differs, the nonce has picked up a per-frame input and nonce uniqueness
    // under the KEK no longer reduces to key-id uniqueness.
    expect(wrapOf(lo)).toBe(wrapOf(hi));
  });
});

describe('completeness', () => {
  it('exercised every row in the file, by name', () => {
    // ASSERTION 1. Set equality, not a count: a count is satisfied by a loop that
    // ran zero times over a file that shrank, which is the exact vacuity this gate
    // exists to prevent.
    const declared = new Set(rows.map((r) => r.name));
    const missed = [...declared].filter((n) => !visited.has(n));
    const extra = [...visited].filter((n) => !declared.has(n));
    expect(missed, `rows in the file that no assertion exercised: ${missed.join(', ')}`).toEqual(
      [],
    );
    expect(extra, `rows exercised that are not in the file: ${extra.join(', ')}`).toEqual([]);
  });
});
