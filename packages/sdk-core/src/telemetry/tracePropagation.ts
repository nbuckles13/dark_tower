// File: packages/sdk-core/src/telemetry/tracePropagation.ts
//
// R-19: ONE injection path that populates W3C `trace_parent` / `trace_state` on
// BOTH outbound envelopes — `ClientMessage` (browser → MC) and `MhClientMessage`
// (browser → MH). Proto fields 20/21 already exist on both messages
// (proto/dark_tower/signaling/v1/signaling.proto); we consume that wire
// contract, we do not define it.
//
// DESIGN — structural, not nominal. The generated TS proto types are NOT yet
// produced (task #6/#13). So this helper is typed against a minimal STRUCTURAL
// shape — anything with optional `traceParent` / `traceState` string fields.
// Both `ClientMessage` and `MhClientMessage` (and `ServerMessage`, though MC
// owns that side) satisfy this shape once generated, so the single function
// serves every current and future envelope without importing types that don't
// exist yet.
//
// The active span whose context is injected is created by a LATER task (the
// `dt_client.join` root span, R-19). Here we provide only the injection helper
// + the global propagator registration (telemetryConfig.ts). If there is no
// active span context (e.g. telemetry unconfigured, or called outside a span),
// the W3C propagator writes nothing and the envelope's trace fields are left
// untouched — a safe no-op.

import { context as otelContext, propagation, type TextMapSetter } from '@opentelemetry/api';
import { TRACE_PARENT_HEADER, TRACE_STATE_HEADER } from '@opentelemetry/core';

/**
 * Minimal structural shape of an outbound signaling envelope that carries W3C
 * trace context. Satisfied by the generated `ClientMessage` and
 * `MhClientMessage` proto types (fields 20/21) once they exist. Kept as a bare
 * structural type so this module never imports not-yet-generated proto types.
 */
export interface TraceCarrier {
  traceParent?: string;
  traceState?: string;
}

// Adapter setter: the W3C propagator writes into a carrier using the W3C HEADER
// names (`traceparent` / `tracestate`). We translate those header keys onto the
// proto FIELD names (`traceParent` / `traceState`). Only the two W3C keys are
// honored; anything else is ignored (the propagator never emits other keys, but
// this keeps the setter total and side-effect-free for unknown keys).
const protoFieldSetter: TextMapSetter<TraceCarrier> = {
  set(carrier, key, value) {
    // `carrier` is always the object we pass to `propagation.inject` below
    // (never undefined), so no nil-guard is needed here.
    if (key === TRACE_PARENT_HEADER) {
      carrier.traceParent = value;
    } else if (key === TRACE_STATE_HEADER) {
      carrier.traceState = value;
    }
  },
};

/**
 * Inject the active span's W3C trace context into a signaling envelope's
 * `traceParent` / `traceState` fields, in place. The SAME path serves
 * `ClientMessage` (→ MC) and `MhClientMessage` (→ MH).
 *
 * Uses the globally-registered propagator (set by `configureTelemetry`). If no
 * propagator is configured or no span is active, this is a no-op and the
 * carrier is returned unchanged.
 *
 * @param carrier the outbound envelope (mutated in place).
 * @returns the same `carrier`, for call-site convenience.
 */
export function injectIntoClientMessage<T extends TraceCarrier>(carrier: T): T {
  propagation.inject(otelContext.active(), carrier, protoFieldSetter);
  return carrier;
}
