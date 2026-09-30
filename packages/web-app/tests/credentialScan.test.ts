// File: packages/web-app/tests/credentialScan.test.ts
//
// Node tier for the pure token-only scan (`e2e/credentialScan.ts`) — each failure
// class fires with its OWN reason token, and the telemetry positive control fails
// distinctly when there is nothing to have scanned (story 2 task 15; test +
// security, handed over from task 3).

import { describe, expect, it } from 'vitest';
import { scanRecordedRequests, type RecordedRequest } from '../e2e/credentialScan.js';

const creds = { email: 'e2e-x@darktower.test', password: 'p4ss-uuid-value' };
const ORIGIN = 'http://org.localhost:5173';

function req(path: string, headers: Record<string, string> = {}, postData: string | null = null) {
  return { url: `${ORIGIN}${path}`, method: 'POST', headers, postData } satisfies RecordedRequest;
}
const meeting = req('/api/v1/meetings/abc/join', { authorization: 'Bearer ey.meeting' });
const telemetry = req('/api/v1/telemetry/v1/metrics', { authorization: 'Bearer ey.user' });

const reasons = (records: RecordedRequest[]): string[] =>
  scanRecordedRequests(records, creds).findings.map((f) => f.reason);

describe('scanRecordedRequests', () => {
  it('a clean window with a meeting request and a Bearer telemetry export passes, and COUNTS what it ran over', () => {
    const report = scanRecordedRequests([meeting, telemetry, telemetry], creds);
    expect(report.findings).toEqual([]);
    expect(report.surfaces.telemetry).toEqual({ scanned: 2, bearerChecked: 2 });
    expect(report.surfaces.meetings).toEqual({ scanned: 1, bearerChecked: 1 });
  });

  it('ZERO telemetry requests fails with its own reason — never a vacuous pass', () => {
    expect(reasons([meeting])).toEqual(['telemetry-not-recorded']);
  });

  it('a telemetry export WITHOUT an Authorization header fails (absent is not a skip)', () => {
    expect(reasons([meeting, req('/api/v1/telemetry/v1/metrics')])).toEqual([
      'telemetry-not-bearer',
    ]);
  });

  it('a non-Bearer telemetry header fails both the scheme check and the surface check', () => {
    expect(
      reasons([meeting, req('/api/v1/telemetry/v1/metrics', { authorization: 'Basic abc' })]),
    ).toEqual(['non-bearer-auth', 'telemetry-not-bearer']);
  });

  it('a credential needle on a telemetry export fails as a CONTENT failure, distinct from the above', () => {
    const leaky = req(
      '/api/v1/telemetry/v1/metrics',
      { authorization: 'Bearer ey.user' },
      `{"attr":"${encodeURIComponent(creds.email)}"}`,
    );
    expect(reasons([meeting, leaky])).toEqual(['credential-on-request']);
  });

  it('keeps the join-window properties: auth endpoint, missing meeting request, non-Bearer meeting', () => {
    expect(reasons([req('/api/v1/auth/user/token'), telemetry])).toEqual([
      'auth-endpoint-hit',
      'no-meeting-request',
    ]);
    expect(reasons([req('/api/v1/meetings/abc/join'), telemetry])).toEqual(['meeting-not-bearer']);
  });

  it('never echoes a credential or a header value into a finding', () => {
    const report = scanRecordedRequests(
      [
        req(`/api/v1/meetings/x?e=${creds.email}`, { authorization: `Basic ${creds.password}` }),
        req('/api/v1/telemetry/v1/metrics', { authorization: `Digest ${creds.password}` }),
      ],
      creds,
    );
    const text = JSON.stringify(report.findings);
    expect(report.findings.length).toBeGreaterThan(0);
    expect(text).not.toContain(creds.email);
    expect(text).not.toContain(creds.password);
    expect(text).toContain('[email]');
  });

  it('a SCHEMELESS header (a bare token) is never printed, even partially', () => {
    const fakeToken = 'eyJfake.header-token.sig';
    const report = scanRecordedRequests(
      [meeting, req('/api/v1/telemetry/v1/metrics', { authorization: fakeToken })],
      creds,
    );
    expect(report.findings.map((f) => f.reason)).toEqual([
      'non-bearer-auth',
      'telemetry-not-bearer',
    ]);
    const text = JSON.stringify(report.findings);
    expect(text).not.toContain(fakeToken);
    expect(text).not.toContain('eyJfake');
    expect(text).toContain('<no scheme; value withheld>');
  });
});
