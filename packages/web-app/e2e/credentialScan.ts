// File: packages/web-app/e2e/credentialScan.ts
//
// THE token-only scan over one browsing context's recorded join window — the pure
// body of `fixtures.ts:assertTokenOnlyJoinTraffic`, extracted (story 2 task 15)
// so a node-tier test can prove each failure class fires and so the scan REPORTS
// what it ran over. No Playwright import: node vitest loads it directly.
//
// Within the window:
//   1. NO request carries the actual email/password strings anywhere (URL,
//      headers, body) — values, not field names, so renames can't dodge it, and
//      both raw AND percent-encoded forms are scanned;
//   2. NO request targets /api/v1/auth/* at all (the forced re-login band-aid of
//      commit 7b69288 must never be re-encoded);
//   3. ANY `Authorization` header is `Bearer ...` — nothing else (closes the
//      `Basic base64(email:password)` channel, which the value-scan cannot see);
//   4. the GC meeting request(s) authenticate via `Authorization: Bearer`;
//   5. (story 2 task 15, handed over from task 3) at least one
//      `/api/v1/telemetry` export was RECORDED in the window, every one of them
//      CARRIES `Authorization: Bearer` (present, not merely "not non-Bearer" —
//      an absent header is a failure on this surface, never a skip), and the
//      needle scan and the Bearer check each demonstrably RAN over them (the
//      per-surface counts). Since R15 every metric export carries the user
//      bearer, which makes telemetry the surface this control most needs to
//      watch; whether an export lands in the window is timing-dependent, so the
//      caller FORCES one (`flushMetrics()` on the E2E bus) inside it.
//
// Properties 1-3 apply to EVERY recorded request, not only join traffic. Do NOT
// narrow them to join paths, or the newest token-carrying surface silently
// leaves this control.
//
// EVERY string placed in a finding goes through `redact()`: the regression this
// detects is a credential on a request, and the diagnostic must show WHERE
// without repeating the value into whatever log sink renders it. Header VALUES
// are never placed in a finding — only the scheme word, redacted
// (@semantic-guard, credential-leak item 8).

/** One recorded HTTP request. */
export interface RecordedRequest {
  readonly url: string;
  readonly method: string;
  readonly headers: Readonly<Record<string, string>>;
  readonly postData: string | null;
}

/** The credential values the scan looks for. */
export interface ScanCredentials {
  readonly email: string;
  readonly password: string;
}

/** How many requests on a surface each check actually ran over. */
export interface SurfaceCounts {
  /** Requests the credential-needle scan ran over. */
  readonly scanned: number;
  /** Requests whose `Authorization` header was checked to be present AND `Bearer `. */
  readonly bearerChecked: number;
}

/** Reason tokens, one per failure class, so a failure names its class. */
export type ScanFindingReason =
  | 'credential-on-request'
  | 'auth-endpoint-hit'
  | 'non-bearer-auth'
  | 'no-meeting-request'
  | 'meeting-not-bearer'
  | 'telemetry-not-recorded'
  | 'telemetry-not-bearer';

export interface ScanFinding {
  readonly reason: ScanFindingReason;
  /** Already redacted. */
  readonly detail: string;
}

export interface ScanReport {
  readonly findings: readonly ScanFinding[];
  readonly surfaces: {
    readonly meetings: SurfaceCounts;
    readonly telemetry: SurfaceCounts;
  };
}

/** The telemetry surface: GC's proxy path, and the OTel exporter's `/v1/metrics` under it. */
const TELEMETRY_PREFIX = '/api/v1/telemetry';

/**
 * The auth SCHEME word of a header value, and nothing else. Only a bounded
 * scheme token FOLLOWED BY A SPACE is printed: a schemeless value (`<jwt>` with
 * the `Bearer ` prefix dropped — the likeliest regression on the telemetry
 * surface) has no scheme, and its first "word" IS the credential, so it is
 * withheld rather than split out (@semantic-guard).
 */
function schemeOf(authHeader: string): string {
  const match = /^([A-Za-z][A-Za-z0-9-]{0,19}) /.exec(authHeader);
  return match?.[1] ?? '<no scheme; value withheld>';
}

/** Scan one context's recorded window. Pure; never throws on content. */
export function scanRecordedRequests(
  records: readonly RecordedRequest[],
  creds: ScanCredentials,
): ScanReport {
  const needles: ReadonlyArray<readonly [string, string]> = [
    [creds.email, '[email]'],
    [creds.password, '[password]'],
    [encodeURIComponent(creds.email), '[email:urlencoded]'],
    [encodeURIComponent(creds.password), '[password:urlencoded]'],
  ];
  const redact = (s: string): string =>
    needles.reduce((acc, [needle, marker]) => acc.replaceAll(needle, marker), s);

  const findings: ScanFinding[] = [];
  const counts = {
    meetings: { scanned: 0, bearerChecked: 0 },
    telemetry: { scanned: 0, bearerChecked: 0 },
  };

  for (const record of records) {
    const where = redact(`${record.method} ${record.url}`);
    // DELIBERATELY independent literals (NOT the fixture's driver consts): a test
    // asserting the ABSENCE of auth calls must not derive the forbidden prefix
    // from the same encoding the drivers use — docs/TODO.md §Cross-Service
    // Duplication, #18 entry, nuance (b). Do not "clean up".
    const pathname = new URL(record.url).pathname;
    const surface = pathname.startsWith('/api/v1/meetings')
      ? counts.meetings
      : pathname.startsWith(TELEMETRY_PREFIX)
        ? counts.telemetry
        : undefined;

    // 1. The needle scan — counted per surface as it RUNS, not inferred later.
    const haystack = `${record.url}\n${JSON.stringify(record.headers)}\n${record.postData ?? ''}`;
    if (surface) surface.scanned += 1;
    if (needles.some(([needle]) => haystack.includes(needle))) {
      findings.push({ reason: 'credential-on-request', detail: where });
    }

    // 3. Any Authorization header is Bearer.
    const authHeader = record.headers['authorization'];
    if (authHeader !== undefined && !authHeader.startsWith('Bearer ')) {
      findings.push({
        reason: 'non-bearer-auth',
        detail: redact(`${where} (scheme: ${schemeOf(authHeader)})`),
      });
    }

    // 2. No auth endpoint.
    if (pathname.startsWith('/api/v1/auth/')) {
      findings.push({ reason: 'auth-endpoint-hit', detail: where });
    }

    // 4 / 5. On the token-carrying surfaces the header must be PRESENT and Bearer.
    if (surface) {
      surface.bearerChecked += 1;
      if (authHeader === undefined || !authHeader.startsWith('Bearer ')) {
        findings.push({
          reason: surface === counts.meetings ? 'meeting-not-bearer' : 'telemetry-not-bearer',
          detail: `${where} (${authHeader === undefined ? 'no Authorization header' : 'not Bearer'})`,
        });
      }
    }
  }

  if (counts.meetings.scanned === 0) {
    findings.push({
      reason: 'no-meeting-request',
      detail: 'no GC /api/v1/meetings request was recorded in the join window',
    });
  }
  if (counts.telemetry.scanned === 0) {
    findings.push({
      reason: 'telemetry-not-recorded',
      detail:
        'no /api/v1/telemetry export was recorded in the window, so the needle scan and the ' +
        'Bearer check observed NOTHING on the surface that carries the bearer on every export. ' +
        'The window must force one (flushMetrics() on the E2E bus) — this is a HARNESS failure, ' +
        'not a pass.',
    });
  }
  return { findings, surfaces: counts };
}
