// File: packages/sdk-core/src/telemetry/__tests__/otlpExporterMock.ts
//
// The ONE no-op stand-in for `@opentelemetry/exporter-metrics-otlp-proto` in the
// unit tier. It is installed for EVERY test file by `otlpExporterSetup.ts`
// (`setupFiles` in vitest.config.ts), so no real exporter is constructed and
// nothing leaves the process, whether or not a test remembers to mock it.
// `resetTelemetryForTest()` shuts the MeterProvider down, which forces a final
// collect + export: with a real exporter that is a POST to the configured
// endpoint (measured against a local server). The reader calls `export`,
// `forceFlush` and `shutdown`, so all three must exist.

import { vi } from 'vitest';

/** Constructor options of every exporter built, in order. */
export const exporterCtor = vi.fn();

/** One entry per `export` call, so a test can prove the mock is the one wired in. */
export const exportCalls: unknown[] = [];

export class OTLPMetricExporter {
  constructor(opts: unknown) {
    exporterCtor(opts);
  }

  export(metrics: unknown, done: (result: { code: number }) => void): void {
    exportCalls.push(metrics);
    done({ code: 0 }); // ExportResultCode.SUCCESS
  }

  forceFlush(): Promise<void> {
    return Promise.resolve();
  }

  shutdown(): Promise<void> {
    return Promise.resolve();
  }
}
