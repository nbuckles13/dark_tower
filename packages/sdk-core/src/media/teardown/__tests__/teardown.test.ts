// File: packages/sdk-core/src/media/teardown/__tests__/teardown.test.ts

import { describe, expect, it } from 'vitest';

import { TeardownRegistry } from '../teardown.js';

describe('TeardownRegistry', () => {
  it('disposes in REVERSE registration order — last acquired, first released', async () => {
    const order: string[] = [];
    const registry = new TeardownRegistry();
    registry.register('capture', () => {
      order.push('capture');
    });
    registry.register('encoder', () => {
      order.push('encoder');
    });
    await registry.dispose();
    expect(order).toEqual(['encoder', 'capture']);
  });

  it('reaches a PARTIALLY-CONSTRUCTED pipeline', async () => {
    // The property that matters. Setup is a chain of awaits; if a later step
    // throws, the resources acquired before it must still be released. A leaked
    // microphone track keeps the device hot and the browser's recording
    // indicator lit — a user-visible privacy failure, not a memory nit.
    let captureStopped = false;
    const registry = new TeardownRegistry();
    const setup = async (): Promise<void> => {
      registry.register('capture', () => {
        captureStopped = true;
      });
      await Promise.resolve();
      throw new Error('encoder configure failed');
    };
    await expect(setup()).rejects.toThrow('encoder configure failed');
    await registry.dispose();
    expect(captureStopped).toBe(true);
  });

  it('is idempotent', async () => {
    let calls = 0;
    const registry = new TeardownRegistry();
    registry.register('thing', () => {
      calls += 1;
    });
    await registry.dispose();
    await registry.dispose();
    expect(calls).toBe(1);
    expect(registry.isDisposed).toBe(true);
  });

  it('disposes a resource that finishes acquiring AFTER teardown began', async () => {
    // The race an async setup cancelled mid-flight actually produces: the
    // registration lands after `dispose()` has already run. Silently dropping it
    // would leave the microphone on.
    let disposed = false;
    const registry = new TeardownRegistry();
    await registry.dispose();
    registry.register('late', () => {
      disposed = true;
    });
    await new Promise<void>((resolve) => {
      setTimeout(resolve, 0);
    });
    expect(disposed).toBe(true);
  });

  it('isolates a throwing disposer and RETURNS the failure rather than swallowing it', async () => {
    // One bad disposer must not strand the rest — but silence would be the
    // masked failure CLAUDE.md's fail-loudly rule names, so the errors come back
    // for the caller to surface.
    let secondRan = false;
    const registry = new TeardownRegistry();
    registry.register('good', () => {
      secondRan = true;
    });
    registry.register('bad', () => {
      throw new Error('close failed');
    });
    const failures = await registry.dispose();
    expect(secondRan).toBe(true);
    expect(failures).toHaveLength(1);
    expect(failures[0]?.name).toBe('bad');
  });

  it('awaits async disposers', async () => {
    let done = false;
    const registry = new TeardownRegistry();
    registry.register('async', async () => {
      await Promise.resolve();
      done = true;
    });
    await registry.dispose();
    expect(done).toBe(true);
  });
});
