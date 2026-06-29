// File: packages/sdk-core/src/events/__tests__/TypedEventEmitter.test.ts
//
// R-41: the shared generic emitter. Subclass to reach the protected `emit`.

import { describe, expect, it } from 'vitest';

import { TypedEventEmitter } from '../TypedEventEmitter.js';

interface Events {
  ping: number;
  note: string;
}

class Probe extends TypedEventEmitter<Events> {
  fire<K extends keyof Events>(type: K, payload: Events[K]): void {
    this.emit(type, payload);
  }
}

describe('TypedEventEmitter', () => {
  it('delivers to registered listeners and returns a working unsubscribe', () => {
    const probe = new Probe();
    const seen: number[] = [];
    const unsub = probe.on('ping', (n) => seen.push(n));
    probe.fire('ping', 1);
    unsub();
    probe.fire('ping', 2);
    expect(seen).toEqual([1]);
  });

  it('off removes a listener; emit with no listeners is a safe no-op', () => {
    const probe = new Probe();
    const seen: string[] = [];
    const fn = (s: string): void => {
      seen.push(s);
    };
    probe.on('note', fn);
    probe.off('note', fn);
    probe.fire('note', 'x'); // no listeners for 'note' now
    probe.fire('ping', 9); // no listeners for 'ping' ever
    expect(seen).toEqual([]);
  });

  it('copy-on-emit: a listener that unsubscribes mid-emit does not disturb the dispatch', () => {
    const probe = new Probe();
    const order: string[] = [];
    const a = (): void => {
      order.push('a');
      probe.off('ping', b); // mutate the set mid-dispatch
    };
    const b = (): void => {
      order.push('b');
    };
    probe.on('ping', a);
    probe.on('ping', b);
    probe.fire('ping', 1);
    expect(order).toEqual(['a', 'b']); // b still received this emit
  });
});
