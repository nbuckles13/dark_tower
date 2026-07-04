//! Per-request ambient tracing span for the MC→MH gRPC server (R-56).
//!
//! # Why this exists
//!
//! `common::observability::otel_grpc::server_interceptor()` extracts the
//! inbound W3C trace context and calls `tracing::Span::current().set_parent(cx)`.
//! That call only has an effect if SOME tracing span is already active
//! ("entered") at the moment the interceptor runs, and that span stays
//! active (via [`tracing::Instrument`]) through to the handler's own
//! `#[instrument]` span creation — so the handler span resolves the
//! re-parented span as its immediate ancestor and inherits the injected
//! trace id.
//!
//! Tonic's generated server provides no such ambient span on its own: a
//! `tonic::service::Interceptor` runs synchronously inside
//! `InterceptedService::call()`, which is invoked *eagerly* when the
//! surrounding `Service::call()` executes — before the response future is
//! ever polled. The handler's `#[instrument]` span, by contrast, is created
//! only once that future is *polled* (an `async fn`'s body doesn't run until
//! then). Nothing wraps that whole synchronous-call-then-poll sequence in a
//! tracing span by default (MH doesn't use `tower_http::trace::TraceLayer`
//! or tonic's `.trace_fn`, and even tonic's own `.trace_fn` span is entered
//! only around the *future's poll*, still too late for the interceptor).
//! Verified empirically: without this layer, an inbound `traceparent` header
//! never reaches the handler's exported span, even with the global
//! propagator correctly registered by `init_otel`.
//!
//! This layer supplies exactly the missing attach point: applied OUTSIDE
//! `with_interceptor` in the `Server` builder, it creates and enters a bare
//! per-request span *before* the interceptor runs, then
//! [`tracing::Instrument`]s the response future so the span stays active
//! across the whole async request lifecycle — including every nested
//! `#[instrument]` span created downstream.
//!
//! Carries no attributes (method name, metadata, etc.) by design — it exists
//! purely as a context-propagation anchor, not an observability surface.
//! Request-level detail belongs to the handler's own `#[instrument]` span
//! (e.g. `register_meeting`), which becomes its child.

use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use tower::{Layer, Service};
use tracing::Instrument;

/// Tower [`Layer`] that wraps a service with [`SpanService`].
#[derive(Clone, Copy, Default)]
pub struct SpanLayer;

impl<S> Layer<S> for SpanLayer {
    type Service = SpanService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        SpanService { inner }
    }
}

/// Tower [`Service`] that enters a bare per-request span before calling the
/// inner service, then [`tracing::Instrument`]s the returned future so the
/// span (and any parent re-attached to it by a downstream interceptor)
/// stays active for the full async lifecycle. See module docs for why this
/// is required.
#[derive(Clone)]
pub struct SpanService<S> {
    inner: S,
}

impl<S, Req> Service<Req> for SpanService<S>
where
    S: Service<Req> + Send,
    S::Future: Send + 'static,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Req) -> Self::Future {
        let span = tracing::info_span!("mh.grpc.request");
        // Enter synchronously so a downstream Interceptor's `Span::current()`
        // (which runs eagerly inside `self.inner.call`, not during a later
        // poll) sees this span. The guard is dropped once `call` returns;
        // `.instrument(span)` below re-enters the span on every subsequent
        // poll of the response future.
        let entered = span.clone().entered();
        let fut = self.inner.call(req);
        drop(entered);
        Box::pin(fut.instrument(span))
    }
}
