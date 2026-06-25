// File: packages/sdk-core/src/telemetry/index.ts
//
// Internal telemetry sub-barrel (R-19, R-24, R-25, R-26). Re-exported by the
// package root `src/index.ts`. This is SCAFFOLDING — the emission sites
// (join-flow metric increments, `dt_client.join` span creation, signaling trace
// injection at the wire) land in later tasks (#13+). Task #12 provides the
// surfaces those tasks consume.

export type { MetricLabels, MetricsSink } from './MetricsSink.js';

export { assertClientMetricName, type GuardMode } from './nameGuard.js';

export { NoopMetricsSink } from './NoopMetricsSink.js';
export { ConsoleMetricsSink } from './ConsoleMetricsSink.js';
export { OtelMetricsSink } from './OtelMetricsSink.js';

export {
  configureTelemetry,
  getMeter,
  getTracer,
  getMetricsSink,
  resetTelemetryForTest,
  type TelemetryConfig,
  type TelemetryEnv,
} from './telemetryConfig.js';

export { injectIntoClientMessage, type TraceCarrier } from './tracePropagation.js';

export { JoinEvent, logJoinEvent, type JoinLogRecord } from './logger.js';

export { CloseReason, normalizeCloseReason } from './closeReason.js';
