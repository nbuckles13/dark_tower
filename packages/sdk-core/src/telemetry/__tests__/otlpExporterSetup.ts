// File: packages/sdk-core/src/telemetry/__tests__/otlpExporterSetup.ts
//
// `setupFiles` entry for sdk-core's unit tier (vitest.config.ts): the OTLP exporter
// is the shared no-op mock in EVERY test file, by construction, so a new test that
// configures telemetry cannot reach the network by forgetting a per-file mock.
// A file that must exercise the REAL exporter opts out with
// `vi.unmock('@opentelemetry/exporter-metrics-otlp-proto')` (telemetryConfig.delta.test.ts).

import { vi } from 'vitest';

vi.mock('@opentelemetry/exporter-metrics-otlp-proto', () => import('./otlpExporterMock.js'));
