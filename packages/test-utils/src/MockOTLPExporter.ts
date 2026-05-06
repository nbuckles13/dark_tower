// File: packages/test-utils/src/MockOTLPExporter.ts
//
// Assertion stub for OTel-OTLP exporter calls. Captures every `export()`
// invocation so tests can assert on what would have been sent without
// performing real HTTP. Failure injection covers the production retry/error
// paths (429 with retryAfterMs, 500/502, network-error).
//
// PII discipline: this mock does NOT redact, normalize, or strip PII from
// payloads — that's the production exporter's responsibility (sdk-core task
// #12, R-2 PII allowlist filter on the GC side). Test fixtures driving this
// mock MUST use synthetic values like `user-test-001@example.test`. See
// README §`MockOTLPExporter` for the fixture-data contract.

/** Discriminator for OTLP signal kind. */
export type OTLPExportPayloadKind = 'metrics' | 'traces';

/**
 * One captured export() invocation. `body` is left as `unknown` because
 * sdk-core (task #12) chooses the concrete `ResourceMetrics`/`ResourceTraces`
 * type when it lands; tests cast as needed.
 */
export interface OTLPExportPayload {
  readonly kind: OTLPExportPayloadKind;
  readonly capturedAt: number;
  readonly body: unknown;
}

/** Result of an export(); mirrors the production exporter contract. */
export type OTLPExportResult =
  | { code: 'success' }
  | { code: 'failure'; error: Error };

/**
 * Failure-injection response specs. Tests set the next response (or a sticky
 * always-respond) to drive `OtelMetricsSink` retry/error paths.
 */
export type MockOTLPResponseSpec =
  | { status: 200 }
  | { status: 202 }
  | { status: 429; retryAfterMs?: number }
  | { status: 500 }
  | { status: 502 }
  | { status: 'network-error' };

interface RecordedCall {
  readonly method: 'export' | 'shutdown' | 'forceFlush';
  readonly payload?: OTLPExportPayload;
  readonly at: number;
}

/**
 * In-memory OTLP exporter mock. Per-call capture (one entry per `export()`
 * call), kind-discriminated inspection, and failure injection.
 *
 * @example
 * const exp = new MockOTLPExporter();
 * exp.simulateNextResponse({ status: 429, retryAfterMs: 100 });
 * await exp.export({ kind: 'metrics', capturedAt: Date.now(), body: rm });
 * exp.getMetricPayloads(); // [the captured payload]
 */
export class MockOTLPExporter {
  readonly #payloads: OTLPExportPayload[] = [];
  readonly #calls: RecordedCall[] = [];

  #nextResponse: MockOTLPResponseSpec | null = null;
  #stickyResponse: MockOTLPResponseSpec | null = null;

  /**
   * Capture a payload and return an `OTLPExportResult` based on the
   * configured response. Default response (no failure injection) is
   * `{ code: 'success' }`.
   */
  async export(payload: OTLPExportPayload): Promise<OTLPExportResult> {
    this.#payloads.push(payload);
    this.#calls.push({ method: 'export', payload, at: Date.now() });

    const spec = this.#nextResponse ?? this.#stickyResponse;
    if (this.#nextResponse !== null) {
      this.#nextResponse = null;
    }
    if (spec === null || spec === undefined) {
      return { code: 'success' };
    }
    return responseFromSpec(spec);
  }

  /** Mirror of production `shutdown()`. Captured for assertion. */
  async shutdown(): Promise<void> {
    this.#calls.push({ method: 'shutdown', at: Date.now() });
  }

  /** Mirror of production `forceFlush()`. Captured for assertion. */
  async forceFlush(): Promise<void> {
    this.#calls.push({ method: 'forceFlush', at: Date.now() });
  }

  // ---------------- Inspection ----------------

  /** All captured `export()` payloads, in invocation order. */
  getExportedPayloads(): readonly OTLPExportPayload[] {
    return this.#payloads;
  }

  /** Captured payloads filtered to `kind === 'metrics'`. */
  getMetricPayloads(): readonly OTLPExportPayload[] {
    return this.#payloads.filter((p) => p.kind === 'metrics');
  }

  /** Captured payloads filtered to `kind === 'traces'`. */
  getTracePayloads(): readonly OTLPExportPayload[] {
    return this.#payloads.filter((p) => p.kind === 'traces');
  }

  /** Total `export()` invocation count. */
  getExportCount(): number {
    return this.#payloads.length;
  }

  /**
   * Count of recorded calls. With no argument, returns total across all
   * methods. With a method name, returns count for that method only.
   */
  callCount(method?: 'export' | 'shutdown' | 'forceFlush'): number {
    if (method === undefined) return this.#calls.length;
    return this.#calls.filter((c) => c.method === method).length;
  }

  /** Reset captured state and clear pending failure-injection. */
  clear(): void {
    this.#payloads.length = 0;
    this.#calls.length = 0;
    this.#nextResponse = null;
    this.#stickyResponse = null;
  }

  // ---------------- Failure injection ----------------

  /**
   * Set a single-shot response for the NEXT `export()` call. Pops after
   * use, so subsequent calls fall back to the sticky response (if any) or
   * the default success response.
   */
  simulateNextResponse(response: MockOTLPResponseSpec): void {
    this.#nextResponse = response;
  }

  /**
   * Set a sticky response that all `export()` calls return until cleared
   * via `clear()` or replaced by another `simulateAlwaysRespond` call.
   * `simulateNextResponse` takes precedence for one call when both are set.
   */
  simulateAlwaysRespond(response: MockOTLPResponseSpec): void {
    this.#stickyResponse = response;
  }
}

function responseFromSpec(spec: MockOTLPResponseSpec): OTLPExportResult {
  if (spec.status === 200 || spec.status === 202) {
    return { code: 'success' };
  }
  if (spec.status === 429) {
    const retry = spec.retryAfterMs;
    return {
      code: 'failure',
      error: new Error(
        `OTLP export rate-limited (status=429${retry !== undefined ? `, retryAfterMs=${retry}` : ''})`,
      ),
    };
  }
  if (spec.status === 500 || spec.status === 502) {
    return { code: 'failure', error: new Error(`OTLP export failed (status=${spec.status})`) };
  }
  return { code: 'failure', error: new Error('OTLP export failed (network-error)') };
}
