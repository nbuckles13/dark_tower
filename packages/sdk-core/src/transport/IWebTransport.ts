// File: packages/sdk-core/src/transport/IWebTransport.ts
//
// R-13: CANONICAL home of the `IWebTransport` contract for the Dark Tower
// browser SDK. All WebTransport usage inside sdk-core flows through this
// interface; `BrowserWebTransport` (production) and `@darktower/test-utils`'
// `MockWebTransport` (tests) both satisfy it.
//
// Pattern A (docs/TODO.md #58): `packages/test-utils/src/contracts/IWebTransport.ts`
// holds a structurally-identical minimal copy so the test double had something
// to implement before sdk-core existed. There is intentionally NO package
// dependency edge in either direction — the two declarations stay in sync via
// TypeScript structural typing, exercised by the explicit type-assignment in
// the transport tests (the DRY drift forcing-function). If this shape ever
// grows a member the mock lacks, that assignment fails to compile.
//
// The shape mirrors the browser `WebTransport` API so the real platform object
// also satisfies it. No metric / trace / log emissions originate from this
// declaration.

/**
 * Close-info envelope mirroring the browser `WebTransportCloseInfo`.
 */
export interface WebTransportCloseInfo {
  readonly closeCode?: number;
  readonly reason?: string;
}

/**
 * Bidirectional stream pair. Both sides are standard Web Streams so consumers
 * apply length-prefix framing themselves (see `../framing/length-prefix.ts`).
 */
export interface WebTransportBidirectionalStream {
  readonly readable: ReadableStream<Uint8Array>;
  readonly writable: WritableStream<Uint8Array>;
}

/**
 * Canonical contract for a WebTransport-shaped client.
 *
 * Connection establishment is modelled as a constructor/factory concern (see
 * {@link BrowserWebTransport} and its `connect` factory) rather than a method
 * on this interface, matching both the browser `WebTransport` (whose
 * constructor initiates the handshake) and `MockWebTransport`. An instance of
 * this interface therefore represents an already-initiated transport whose
 * lifecycle is observed through the `ready` / `closed` promises.
 *
 * Event hooks (open / close / error) are surfaced as promises rather than an
 * `EventTarget` to stay structurally compatible with the test double:
 *   - open  -> `ready` resolves
 *   - close -> `closed` resolves with {@link WebTransportCloseInfo}
 *   - error -> `ready` (if pending) and/or `closed` reject
 */
export interface IWebTransport {
  /** Resolves once the transport handshake is complete (open hook). */
  readonly ready: Promise<void>;

  /** Resolves with close info when the connection terminates (close hook). */
  readonly closed: Promise<WebTransportCloseInfo>;

  /** Datagram readable/writable pair. */
  readonly datagrams: {
    readonly readable: ReadableStream<Uint8Array>;
    readonly writable: WritableStream<Uint8Array>;
  };

  /** Open a new bidirectional stream. */
  createBidirectionalStream(): Promise<WebTransportBidirectionalStream>;

  /** Close the transport with optional close-info. */
  close(info?: WebTransportCloseInfo): void;
}
