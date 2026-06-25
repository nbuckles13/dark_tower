// File: packages/sdk-core/src/telemetry/NoopMetricsSink.ts
//
// R-24: the DEFAULT `MetricsSink`. Until `configureTelemetry` installs a real
// sink, the SDK emits nothing — telemetry is opt-in, so an unconfigured SDK is
// silent (no console noise, no network). Every method is a no-op.
//
// The name guard is intentionally NOT run here: a sink that emits nothing has
// nothing to guard, and skipping the regex keeps the default path allocation-
// and-work-free on every join.
//
// The methods take NO parameters: TypeScript structural typing makes a
// parameterless method assignable to the wider `MetricsSink` signature
// (`counter(name, labels, value?)` etc.), so `implements MetricsSink` still
// holds while the no-op bodies have no unused bindings to lint.

import type { MetricsSink } from './MetricsSink.js';

/** A `MetricsSink` that discards every emission. The unconfigured default. */
export class NoopMetricsSink implements MetricsSink {
  counter(): void {
    // intentional no-op — discards the emission.
  }

  histogram(): void {
    // intentional no-op — discards the emission.
  }

  gauge(): void {
    // intentional no-op — discards the emission.
  }
}
