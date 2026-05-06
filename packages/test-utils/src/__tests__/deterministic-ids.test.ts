import { describe, expect, it } from 'vitest';
import { createIdFactory, createSeededRng, createSeededUuid } from '../deterministic-ids.js';

describe('deterministic-ids', () => {
  it('same seed produces identical UUID and RNG sequences across independent factories', () => {
    const a = createIdFactory('test-seed-1');
    const b = createIdFactory('test-seed-1');
    const aIds = [a.uuid(), a.uuid(), a.uuid()];
    const bIds = [b.uuid(), b.uuid(), b.uuid()];
    expect(aIds).toEqual(bIds);

    // Different seed → different sequence.
    const c = createIdFactory('different-seed');
    expect(c.uuid()).not.toBe(aIds[0]);
  });

  it('UUIDs match RFC 4122 v4 byte layout (variant + version bits)', () => {
    const rng = createSeededRng(42);
    const next = createSeededUuid(rng);
    const id = next();
    // Format: 8-4-4-4-12 lowercase hex.
    expect(id).toMatch(
      /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/,
    );
  });
});
