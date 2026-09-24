// File: packages/web-app/src/__tests__/telemetryOrigin.test.ts
//
// The telemetry endpoint decides who receives a live user bearer token: every
// metric export carries `Authorization: Bearer <userToken>` because GC's
// telemetry proxy is behind `require_user_auth`. A deploy that points it at a
// third-party collector ships user JWTs off-estate and works perfectly from the
// app's point of view — nothing breaks, nothing logs, the metrics even arrive.
//
// So the origin check is a security control, not input hygiene, and these tests
// pin the two properties that make it one: it THROWS (a warning at deploy time is
// read by nobody), and it does not reject the ordinary all-relative deployment
// (a control that false-fires on the normal case gets deleted).

import { describe, expect, it } from 'vitest';

import { loadConfig } from '../lib/config.js';

/** Drive `loadConfig` with a specific `import.meta.env` shape. */
function withEnv<T>(overrides: Record<string, string | undefined>, run: () => T): T {
  const env = import.meta.env as unknown as Record<string, unknown>;
  const saved = new Map<string, unknown>();
  for (const [k, v] of Object.entries(overrides)) {
    saved.set(k, env[k]);
    if (v === undefined) delete env[k];
    else env[k] = v;
  }
  try {
    return run();
  } finally {
    for (const [k, v] of saved) {
      if (v === undefined) delete env[k];
      else env[k] = v;
    }
  }
}

describe('telemetry endpoint origin', () => {
  it('accepts the ordinary all-relative deployment', () => {
    // Both default to relative: gcBaseUrl is '' and R10 sets the telemetry
    // endpoint to '/api/v1/telemetry'. This is the normal case and MUST pass,
    // or the check would be deleted the first time someone ran the app.
    withEnv({ VITE_TELEMETRY_ENDPOINT: '/api/v1/telemetry', VITE_GC_BASE_URL: undefined }, () => {
      const config = loadConfig();
      expect(config.telemetryEndpoint).toBe('/api/v1/telemetry');
    });
  });

  it('accepts an absolute telemetry endpoint on the GC origin', () => {
    withEnv(
      {
        VITE_TELEMETRY_ENDPOINT: 'https://gc.example.test/api/v1/telemetry',
        VITE_GC_BASE_URL: 'https://gc.example.test',
      },
      () => {
        expect(loadConfig().telemetryEndpoint).toBe('https://gc.example.test/api/v1/telemetry');
      },
    );
  });

  it('THROWS when the telemetry endpoint is on a different origin than GC', () => {
    // The failure this exists to stop: a live user credential sent to a service
    // that is not GC.
    withEnv(
      {
        VITE_TELEMETRY_ENDPOINT: 'https://collector.third-party.test/v1',
        VITE_GC_BASE_URL: 'https://gc.example.test',
      },
      () => {
        expect(() => loadConfig()).toThrow(/must share an origin with the GC API/);
      },
    );
  });

  it('throws on the mixed case too — relative GC, absolute off-origin telemetry', () => {
    // Resolving BOTH against the page origin is what makes this case behave the
    // same as the both-absolute one, rather than needing its own branch.
    withEnv(
      {
        VITE_TELEMETRY_ENDPOINT: 'https://collector.third-party.test/v1',
        VITE_GC_BASE_URL: undefined,
      },
      () => {
        expect(() => loadConfig()).toThrow(/bearer token/);
      },
    );
  });

  it('does not check anything when telemetry is disabled', () => {
    // Telemetry is opt-in and off by default; an unset endpoint must not throw.
    withEnv(
      { VITE_TELEMETRY_ENDPOINT: undefined, VITE_GC_BASE_URL: 'https://gc.example.test' },
      () => {
        expect(loadConfig().telemetryEndpoint).toBeUndefined();
      },
    );
  });
});
