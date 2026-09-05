// File: packages/sdk-core/src/media/frame/__tests__/rejectReason.test.ts
//
// Set equality between the TypeScript token union and the SSoT's
// `reject_reasons[].token`, in BOTH directions.
//
// This is the only cover for the three tokens no conformance row exercises —
// `no_transmit_key`, `no_kek_for_generation`, `no_roster_entry` (`has_vector:
// false`). Those are exactly where a typo would ship green: the conformance
// harness never emits them, so no per-arm comparison reaches them.
//
// Carried-forward item (b) is precise about why this is not enough on its own and
// not redundant with the per-arm assertions either: exhaustiveness proves every
// error has an arm, NOT that the token on that arm is the right string, because
// both sides read it from the same file. The per-arm equality in
// `vectors.conformance.test.ts` covers spelling for the thirteen tokens that have
// rows; this covers membership for all sixteen.

import { describe, expect, it } from 'vitest';

import { ALL_REJECT_REASONS, NON_DROPPING_REASON, FrameRejectedError } from '../rejectReason.js';
import { RECEIVE_PATH_REASONS } from '../receivePath.js';
import { loadFrameVectors } from './frameVectors.js';

const file = loadFrameVectors();

describe('the token set matches the SSoT exactly', () => {
  it('has no token the SSoT lacks, and lacks none the SSoT has', () => {
    const ours = [...ALL_REJECT_REASONS].sort();
    const theirs = file.reject_reasons.map((r) => r.token).sort();
    // Equality, not subset, in both directions. A subset assertion stays green
    // when a token is DELETED from one side.
    expect(ours).toEqual(theirs);
  });

  it('declares no duplicates', () => {
    expect(new Set(ALL_REJECT_REASONS).size).toBe(ALL_REJECT_REASONS.length);
  });

  it('covers the tokens no vector row exercises', () => {
    // Named explicitly rather than left implicit in the set comparison: these
    // three are the reason this file exists, and if the SSoT ever stops carrying
    // them the omission should be visible here rather than absorbed silently into
    // a passing set-equality.
    const unvectored = file.reject_reasons.filter((r) => !r.has_vector).map((r) => r.token);
    expect(unvectored.sort()).toEqual(
      ['no_kek_for_generation', 'no_roster_entry', 'no_transmit_key'].sort(),
    );
    for (const token of unvectored) {
      expect(ALL_REJECT_REASONS as readonly string[]).toContain(token);
    }
  });
});

describe('drops_frame is read from the SSoT, never hand-written', () => {
  it('identifies exactly one non-dropping reason, and it is the one we model as an outcome', () => {
    // Reading it from the file rather than restating it here is what stops
    // `wrap_key_id_mismatch` becoming a drop through someone retyping a boolean.
    const nonDropping = file.reject_reasons.filter((r) => !r.drops_frame).map((r) => r.token);
    expect(nonDropping).toEqual([NON_DROPPING_REASON]);
  });

  it('the non-dropping reason is not thrown by the receive path', () => {
    // It travels on the SUCCESS return channel as a `wrapOutcome`, so it is
    // structurally impossible to catch into a drop counter. That protects R-25's
    // `received = played + sum(drops)` identity, which fails only in aggregate and
    // long after the label set is frozen.
    expect(RECEIVE_PATH_REASONS as readonly string[]).not.toContain(NON_DROPPING_REASON);
  });
});

describe('semantic pairs stay distinct', () => {
  it('keeps the two AES-GCM failures apart', () => {
    // `unwrap_failed` (KEK unwrap) and `decrypt_failed` (SFrame payload) are both
    // GCM tag failures on one receive path with OPPOSITE remedies — one points at
    // key distribution, the other at the sender's schedule. Collapsing them would
    // send triage to the wrong team.
    expect(ALL_REJECT_REASONS).toContain('unwrap_failed');
    expect(ALL_REJECT_REASONS).toContain('decrypt_failed');
  });

  it('keeps the two payload-length failures apart', () => {
    // `payload_length_exceeds_max` is a limit breach (hostile or broken sender);
    // `payload_length_exceeds_available` is a truncation (transport or framing).
    expect(ALL_REJECT_REASONS).toContain('payload_length_exceeds_max');
    expect(ALL_REJECT_REASONS).toContain('payload_length_exceeds_available');
  });
});

describe('errors carry a machine-readable discriminant', () => {
  it('exposes rejectReason as a field, not only in the message', () => {
    // Story task 19 does `counter.add(1, { reason: err.rejectReason })`. Without a
    // stable field it would have to string-match a message or re-derive the
    // taxonomy at the call site.
    const err = new FrameRejectedError('truncated', 'codec', 'frame ends early', { at: 3 });
    expect(err.rejectReason).toBe('truncated');
    expect(err.layer).toBe('codec');
    expect(err.detail.at).toBe(3);
  });

  it('carries no key material in its message', () => {
    // Review-enforced, not guard-enforced: `ts_pii.rs` scans `console.*`/`logger.*`
    // call sites and does not see an `Error(...)` constructor. This asserts the
    // shape rather than the absence of every possible secret.
    const err = new FrameRejectedError('decrypt_failed', 'crypto', 'SFrame payload failed', {});
    expect(err.message).not.toMatch(/[0-9a-f]{32,}/);
  });
});
