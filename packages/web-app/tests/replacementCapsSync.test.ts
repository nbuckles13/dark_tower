// File: packages/web-app/tests/replacementCapsSync.test.ts
//
// story 4 C2 (operations Gate-3 F2): the client's replacement caps are sized on
// MC's heartbeat timeout N, which the browser cannot read. This pins the link
// against the repo ConfigMap (the file Kind runs), so raising N past what the
// caps cover FAILS here instead of turning every ordinary crash into
// `resume_ms{outcome="abandoned"}` and an overlay that expires mid-replacement.

import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';
import { DEFAULT_CLIENT_CONFIG, REPLACEMENT_AFTER_DETECTION_BUDGET_MS } from '@darktower/sdk-core';
import { parseConfigEnv, requirePositiveInt, serviceConfigEnvPath } from '../e2e/configEnv.js';

const N_KEY = 'MC_MH_HEARTBEAT_TIMEOUT_MS';

function heartbeatTimeoutMs(text: string): number {
  return requirePositiveInt(
    parseConfigEnv(text),
    N_KEY,
    'mc config.env',
    'cannot size the client replacement caps',
  );
}

describe('client replacement caps vs MC heartbeat timeout N', () => {
  const caps = DEFAULT_CLIENT_CONFIG.media.handlers;

  it('the overlay cap covers detection (N) plus the after-detection budget', () => {
    const n = heartbeatTimeoutMs(readFileSync(serviceConfigEnvPath('mc-service'), 'utf8'));
    expect(n).toBeGreaterThan(0); // extraction control
    expect(n + REPLACEMENT_AFTER_DETECTION_BUDGET_MS).toBeLessThanOrEqual(caps.reconnectingCapMs);
  });

  it('abandonment is at least twice the overlay cap ("abandoned" means failed, not slow)', () => {
    expect(caps.resumeAbandonMs).toBeGreaterThanOrEqual(2 * caps.reconnectingCapMs);
  });

  it('DEMONSTRATES that it fires: a raised N fails the overlay check', () => {
    const raised = heartbeatTimeoutMs(`${N_KEY}=30000\n`);
    expect(raised + REPLACEMENT_AFTER_DETECTION_BUDGET_MS).toBeGreaterThan(caps.reconnectingCapMs);
  });
});
