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
 * The datagram duplex pair, mirroring the browser
 * `WebTransportDatagramDuplexStream`.
 *
 * **CANONICAL DECLARATION AND ITS RATIONALE LIVE IN
 * `packages/sdk-core/src/transport/IWebTransport.ts::WebTransportDatagrams`** —
 * in particular why the three queue knobs are `number | undefined` rather than
 * `?: number` (assignability ignores missing OPTIONAL members, so optional
 * declarations would leave the Pattern-A drift guard blind to them), and why
 * they are mutable.
 *
 * That reasoning is DELIBERATELY NOT REPEATED HERE. Pattern A duplicates the
 * SHAPE because the type-assignment guard forces the shape to stay in sync; that
 * guard checks structure and not comments, so duplicated prose has no forcing
 * function at all and a correction to one copy would silently leave the other
 * asserting the old thing — which is precisely what the referenced block is
 * about.
 */
export interface WebTransportDatagrams {
  readonly readable: ReadableStream<Uint8Array>;
  readonly writable: WritableStream<Uint8Array>;
  /** Outgoing datagram queue depth, in datagrams (for audio: frames). */
  outgoingHighWaterMark: number | undefined;
  /**
   * Age after which the user agent silently discards a queued datagram, in ms.
   *
   * `null` is the platform's "no age bound"; `undefined` is a test double that
   * has not been told. Both are carried rather than collapsed, so "nobody chose"
   * stays distinguishable from "chosen to be unbounded".
   */
  outgoingMaxAge: number | null | undefined;
  /** Incoming datagram queue depth, in datagrams. */
  incomingHighWaterMark: number | undefined;
  /**
   * Largest datagram the transport will accept, in bytes.
   *
   * `undefined` under a test double or a user agent that does not report it, in
   * which case a sender SKIPS the oversize check rather than inventing a bound —
   * a fabricated limit would produce drops with no basis.
   */
  readonly maxDatagramSize: number | undefined;
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

  /** Datagram duplex pair plus its queue knobs. See {@link WebTransportDatagrams}. */
  readonly datagrams: WebTransportDatagrams;

  /** Open a new bidirectional stream. */
  createBidirectionalStream(): Promise<WebTransportBidirectionalStream>;

  /** Close the transport with optional close-info. */
  close(info?: WebTransportCloseInfo): void;
}
