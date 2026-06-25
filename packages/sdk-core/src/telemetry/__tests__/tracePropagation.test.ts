// File: packages/sdk-core/src/telemetry/__tests__/tracePropagation.test.ts
//
// R-19: the single trace-injection path. Verifies that ONE helper populates
// W3C `traceParent` / `traceState` on BOTH envelope shapes (`ClientMessage`-like
// and `MhClientMessage`-like), and that it is a safe no-op when no span is
// active / no propagator is registered.

import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import {
  context,
  propagation,
  ROOT_CONTEXT,
  trace,
  TraceFlags,
  type SpanContext,
} from '@opentelemetry/api';
import { TraceState } from '@opentelemetry/core';
import { W3CTraceContextPropagator } from '@opentelemetry/core';
import { StackContextManager } from '@opentelemetry/sdk-trace-web';

import { injectIntoClientMessage, type TraceCarrier } from '../tracePropagation.js';

// `context.with(...)` only propagates `context.active()` when a ContextManager
// is registered. `StackContextManager` is the synchronous web manager — enough
// for these synchronous injection tests.
function enableContextManager(): void {
  const mgr = new StackContextManager();
  mgr.enable();
  context.setGlobalContextManager(mgr);
}

// A sampled span context with a known trace id, so we can assert the emitted
// `traceParent` carries it.
const SPAN_CONTEXT: SpanContext = {
  traceId: '0af7651916cd43dd8448eb211c80319c',
  spanId: 'b7ad6b7169203331',
  traceFlags: TraceFlags.SAMPLED,
  isRemote: false,
};

function withActiveSpanContext<T>(fn: () => T): T {
  const ctx = trace.setSpanContext(ROOT_CONTEXT, SPAN_CONTEXT);
  return context.with(ctx, fn);
}

describe('injectIntoClientMessage (R-19)', () => {
  afterEach(() => {
    propagation.disable();
    context.disable();
  });

  describe('with the global W3C propagator registered + an active span', () => {
    beforeEach(() => {
      enableContextManager();
    });

    it('populates traceParent on a ClientMessage-shaped carrier', () => {
      propagation.setGlobalPropagator(new W3CTraceContextPropagator());
      const carrier: TraceCarrier = {};
      withActiveSpanContext(() => injectIntoClientMessage(carrier));
      expect(carrier.traceParent).toBeDefined();
      // W3C format: 00-<trace-id>-<span-id>-<flags>
      expect(carrier.traceParent).toContain(SPAN_CONTEXT.traceId);
      expect(carrier.traceParent).toContain(SPAN_CONTEXT.spanId);
    });

    it('uses the SAME path for an MhClientMessage-shaped carrier', () => {
      propagation.setGlobalPropagator(new W3CTraceContextPropagator());
      // Structurally identical carrier — there is one injection path.
      const mhCarrier: TraceCarrier = {};
      withActiveSpanContext(() => injectIntoClientMessage(mhCarrier));
      expect(mhCarrier.traceParent).toContain(SPAN_CONTEXT.traceId);
    });

    it('returns the same carrier object (for call-site chaining)', () => {
      propagation.setGlobalPropagator(new W3CTraceContextPropagator());
      const carrier: TraceCarrier = {};
      const returned = withActiveSpanContext(() => injectIntoClientMessage(carrier));
      expect(returned).toBe(carrier);
    });

    it('writes a well-formed W3C traceparent (version-traceid-spanid-flags)', () => {
      propagation.setGlobalPropagator(new W3CTraceContextPropagator());
      const carrier: TraceCarrier = {};
      withActiveSpanContext(() => injectIntoClientMessage(carrier));
      expect(carrier.traceParent).toMatch(/^00-[0-9a-f]{32}-[0-9a-f]{16}-0[01]$/);
    });

    it('populates traceState onto the proto field when the span carries one', () => {
      propagation.setGlobalPropagator(new W3CTraceContextPropagator());
      const ctxWithState = trace.setSpanContext(ROOT_CONTEXT, {
        ...SPAN_CONTEXT,
        traceState: new TraceState('vendor=value'),
      });
      const carrier: TraceCarrier = {};
      context.with(ctxWithState, () => injectIntoClientMessage(carrier));
      expect(carrier.traceState).toBe('vendor=value');
      expect(carrier.traceParent).toBeDefined();
    });
  });

  describe('safe no-op behavior', () => {
    it('leaves the carrier untouched when no span is active', () => {
      propagation.setGlobalPropagator(new W3CTraceContextPropagator());
      const carrier: TraceCarrier = {};
      // No active span context — W3C propagator writes nothing.
      injectIntoClientMessage(carrier);
      expect(carrier.traceParent).toBeUndefined();
      expect(carrier.traceState).toBeUndefined();
    });

    it('does not throw when no propagator is registered', () => {
      // Default no-op propagator: inject is a no-op, carrier stays empty.
      const carrier: TraceCarrier = {};
      expect(() => injectIntoClientMessage(carrier)).not.toThrow();
      expect(carrier.traceParent).toBeUndefined();
    });
  });
});
