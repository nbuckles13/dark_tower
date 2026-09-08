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
// the transport tests (the DRY drift forcing-function).
//
// WHAT THAT GUARD ACTUALLY COVERS, stated precisely because the shorter form
// ("if this shape grows a member the mock lacks, that assignment fails to
// compile") is FALSE for one member shape and would have gone silently blind
// here: assignability catches a missing REQUIRED member and a missing
// required-but-possibly-undefined member (`x: T | undefined`), and IGNORES a
// missing OPTIONAL member (`x?: T`). A member added as optional is therefore NOT
// covered by this guard. Anything added to this interface must be declared
// required, or required-but-possibly-undefined — which is why the three datagram
// queue knobs are spelled `number | undefined` rather than `?: number`.
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
 * The datagram duplex pair, mirroring the browser
 * `WebTransportDatagramDuplexStream`.
 *
 * ---------------------------------------------------------------------------
 * THE THREE QUEUE KNOBS ARE REQUIRED-BUT-POSSIBLY-UNDEFINED, NOT OPTIONAL
 * ---------------------------------------------------------------------------
 *
 * They are declared `number | undefined` rather than `?: number` DELIBERATELY.
 * TypeScript assignability IGNORES MISSING OPTIONAL PROPERTIES, so declaring
 * them optional would leave the Pattern-A drift guard (the
 * `const transport: IWebTransport = mock;` assignment in the transport tests)
 * silently blind to exactly the members most recently added, while its comment
 * kept asserting a guarantee it no longer provided. Required-but-undefined
 * restores the assignability break with ONE guard rather than adding a second.
 *
 * They are MUTABLE because setting them is the point (ADR-0036 §1, §11): the
 * SDK keeps the transport send queue shallow and owns a bounded queue above it,
 * so the drop decision — and the count — happen in our code. A drop inside the
 * user agent's queue is uncountable by us AND structurally invisible to the
 * media handler, which is the one drop class §11 says matters most. The real
 * `WebTransportDatagramDuplexStream` declares all three as plain `number`
 * accessors, so the platform object satisfies this shape.
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

  /** Datagram duplex pair plus its queue knobs. See {@link WebTransportDatagrams}. */
  readonly datagrams: WebTransportDatagrams;

  /** Open a new bidirectional stream. */
  createBidirectionalStream(): Promise<WebTransportBidirectionalStream>;

  /** Close the transport with optional close-info. */
  close(info?: WebTransportCloseInfo): void;
}
