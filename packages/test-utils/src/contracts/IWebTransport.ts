// File: packages/test-utils/src/contracts/IWebTransport.ts
//
// Minimal interface that test-utils' MockWebTransport implements. Mirrors the
// browser WebTransport API's structural shape so that:
//   - The real browser WebTransport satisfies this interface.
//   - sdk-core (task #9, R-13) can declare its canonical IWebTransport with the
//     same shape and MockWebTransport will satisfy both via TypeScript
//     structural typing — no circular package dep.
// Shipping this declaration here unblocks the test-utils package; the
// canonical-home decision is tracked in docs/TODO.md as a Gate 3 follow-up.

/**
 * Close-info envelope mirroring the browser `WebTransportCloseInfo`.
 */
export interface WebTransportCloseInfo {
  readonly closeCode?: number;
  readonly reason?: string;
}

/**
 * Bidirectional stream pair. Test doubles model both sides as standard Web
 * Streams so consumers can apply length-prefix framing themselves.
 */
export interface WebTransportBidirectionalStream {
  readonly readable: ReadableStream<Uint8Array>;
  readonly writable: WritableStream<Uint8Array>;
}

/**
 * Minimal contract for a WebTransport-shaped client. `MockWebTransport`
 * implements this; production code in sdk-core will declare an equivalent
 * interface and consume either the real `WebTransport` or this mock.
 */
export interface IWebTransport {
  /** Resolves once the transport handshake is complete. */
  readonly ready: Promise<void>;

  /** Resolves with close info when the connection terminates. */
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
