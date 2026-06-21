// File: packages/sdk-core/src/transport/__tests__/BrowserWebTransport.test.ts
//
// Transport-layer tests. Drives the SDK code against `MockWebTransport` from
// `@darktower/test-utils` (NO inline double — the canonical home of the mock is
// test-utils per ADR-0028). Also exercises framing end-to-end over a bidi
// stream so the codec and transport are validated together.

import { afterEach, describe, expect, it } from 'vitest';
import { MockWebTransport } from '@darktower/test-utils';
import type {
  IWebTransport,
  WebTransportBidirectionalStream,
  WebTransportCloseInfo,
} from '../IWebTransport.js';
import { BrowserWebTransport, connect } from '../BrowserWebTransport.js';
import { TransportError, TransportErrorCode } from '../errors.js';
import { encodeFrame, FrameDecoder } from '../../framing/length-prefix.js';

// DRY drift forcing-function (Pattern A, docs/TODO.md #58): this explicit
// type-annotated binding is the compiler check that `MockWebTransport` (and
// thus sdk-core's canonical `IWebTransport`) stay structurally in sync. If
// sdk-core's `IWebTransport` grows a member the mock lacks, this fails to
// compile. DO NOT remove the `: IWebTransport` annotation — that is the guard.
function asTransport(mock: MockWebTransport): IWebTransport {
  const transport: IWebTransport = mock;
  return transport;
}

describe('IWebTransport / MockWebTransport contract (Pattern A drift guard)', () => {
  it('test 11a: MockWebTransport satisfies sdk-core canonical IWebTransport', () => {
    const transport = asTransport(new MockWebTransport());
    // Members of the canonical interface are all present on the mock.
    expect(typeof transport.createBidirectionalStream).toBe('function');
    expect(typeof transport.close).toBe('function');
    expect(transport.ready).toBeInstanceOf(Promise);
    expect(transport.closed).toBeInstanceOf(Promise);
    expect(transport.datagrams).toBeDefined();
  });

  it('test 11b: BrowserWebTransport instance is assignable to IWebTransport', () => {
    // Compile-time assertion that the production class exposes exactly the
    // interface surface (no wider public type leaks to consumers).
    const make = (wt: ConstructorParameters<typeof BrowserWebTransport>[0]): IWebTransport =>
      new BrowserWebTransport(wt);
    expect(make).toBeTypeOf('function');
  });
});

describe('transport + framing round-trip over a bidi stream', () => {
  it('test 11c: open stream, write framed payload, receive + decode framed reply, close', async () => {
    const transport = asTransport(new MockWebTransport());
    const mock = transport as unknown as MockWebTransport;

    mock.simulateReady();
    await expect(transport.ready).resolves.toBeUndefined();

    const stream = await transport.createBidirectionalStream();

    // --- Outbound: SDK writes a length-prefixed frame ---
    const outPayload = new Uint8Array([0x11, 0x22, 0x33]);
    const writer = stream.writable.getWriter();
    await writer.write(encodeFrame(outPayload));
    await writer.close();

    const outboundChunks = mock.getOutboundBidiWrites(0);
    expect(outboundChunks).toHaveLength(1);
    // The written bytes are the framed payload (BE prefix + body).
    expect(Array.from(outboundChunks[0]!)).toEqual([0x00, 0x00, 0x00, 0x03, 0x11, 0x22, 0x33]);

    // --- Inbound: server pushes a framed reply; SDK decodes it ---
    const replyPayload = new Uint8Array([0xde, 0xad, 0xbe, 0xef]);
    mock.simulateServerMessage(0, encodeFrame(replyPayload));

    const reader = stream.readable.getReader();
    const { value, done } = await reader.read();
    expect(done).toBe(false);
    expect(value).toBeDefined();

    const decoder = new FrameDecoder();
    const frames = decoder.push(value!);
    expect(frames).toHaveLength(1);
    expect(frames[0]).toEqual(replyPayload);

    // --- Close: closed promise resolves with close info ---
    mock.simulateClose(0, 'bye');
    await expect(transport.closed).resolves.toEqual({ closeCode: 0, reason: 'bye' });
  });
});

// --- Fake platform WebTransport ctor double (sdk-core's own wrapper is under
// test here, so the double lives inline rather than in test-utils). Records the
// constructor args so the prod/dev `connect` paths can be asserted, and exposes
// the minimal `IWebTransport`-shaped surface the wrapper delegates to.
interface FakeCtorCall {
  readonly url: string;
  readonly options: unknown;
}

function installFakeWebTransport(): {
  calls: FakeCtorCall[];
  instances: FakeTransport[];
} {
  const calls: FakeCtorCall[] = [];
  const instances: FakeTransport[] = [];

  class FakeTransportImpl implements FakeTransport {
    readonly ready = Promise.resolve();
    readonly closed = Promise.resolve<WebTransportCloseInfo>({});
    readonly datagrams = {
      readable: new ReadableStream<Uint8Array>(),
      writable: new WritableStream<Uint8Array>(),
    };
    closeCalls: Array<WebTransportCloseInfo | undefined> = [];
    bidiOpened = 0;

    constructor(url: string, options?: unknown) {
      calls.push({ url, options });
      instances.push(this);
    }

    createBidirectionalStream(): Promise<WebTransportBidirectionalStream> {
      this.bidiOpened += 1;
      return Promise.resolve({
        readable: new ReadableStream<Uint8Array>(),
        writable: new WritableStream<Uint8Array>(),
      });
    }

    close(info?: WebTransportCloseInfo): void {
      this.closeCalls.push(info);
    }
  }

  (globalThis as { WebTransport?: unknown }).WebTransport = FakeTransportImpl;
  return { calls, instances };
}

interface FakeTransport {
  readonly ready: Promise<void>;
  readonly closed: Promise<WebTransportCloseInfo>;
  readonly datagrams: {
    readonly readable: ReadableStream<Uint8Array>;
    readonly writable: WritableStream<Uint8Array>;
  };
  closeCalls: Array<WebTransportCloseInfo | undefined>;
  bidiOpened: number;
  createBidirectionalStream(): Promise<WebTransportBidirectionalStream>;
  close(info?: WebTransportCloseInfo): void;
}

describe('connect() + BrowserWebTransport runtime behavior', () => {
  const g = globalThis as { WebTransport?: unknown };
  const saved = Object.getOwnPropertyDescriptor(g, 'WebTransport');

  afterEach(() => {
    if (saved) {
      Object.defineProperty(g, 'WebTransport', saved);
    } else {
      delete g.WebTransport;
    }
  });

  it('test 11d: throws a typed TransportError when WebTransport is unavailable', () => {
    // Node test env has no WebTransport; ensure it is absent regardless.
    delete g.WebTransport;
    try {
      connect('https://example.invalid:4433/');
      expect.unreachable('connect should throw when WebTransport is unavailable');
    } catch (e) {
      expect(e).toBeInstanceOf(TransportError);
      const err = e as TransportError;
      // Stable discriminant (not message-string matching) — instrumentation hook.
      expect(err.code).toBe(TransportErrorCode.Unavailable);
      expect(err.code).toBe('WEBTRANSPORT_UNAVAILABLE');
    }
  });

  it('test 11e: prod path — connect(url) constructs with the URL and NO options literal', () => {
    const { calls } = installFakeWebTransport();
    const transport = connect('https://mc.example:4433/meet');

    expect(transport).toBeInstanceOf(BrowserWebTransport);
    expect(calls).toHaveLength(1);
    expect(calls[0]!.url).toBe('https://mc.example:4433/meet');
    // No options object on the prod path (CA-validated TLS).
    expect(calls[0]!.options).toBeUndefined();
  });

  it('test 11f: dev path — connect with devCertificateHashes forwards serverCertificateHashes to the ctor', () => {
    // Positive counterpart to bundle-content.test.ts's negative assertion: when
    // `__DEV_TRUST_FINGERPRINT__` is true (vitest define), the dev hashes are
    // forwarded under the browser's `serverCertificateHashes` key.
    const { calls } = installFakeWebTransport();
    const hashes = [{ algorithm: 'sha-256', value: new Uint8Array([1, 2, 3, 4]) }];
    connect('https://dev.example:4433/', { devCertificateHashes: hashes });

    expect(calls).toHaveLength(1);
    const options = calls[0]!.options as { serverCertificateHashes?: unknown };
    expect(options).toBeDefined();
    expect(options.serverCertificateHashes).toBe(hashes);
  });

  it('test 11g: dev path with empty/absent hashes falls back to the prod (no-options) path', () => {
    const { calls } = installFakeWebTransport();
    connect('https://dev.example:4433/', { devCertificateHashes: [] });
    expect(calls[0]!.options).toBeUndefined();
  });

  it('test 11h: BrowserWebTransport delegates every member to the wrapped transport', async () => {
    const { instances } = installFakeWebTransport();
    const transport: IWebTransport = connect('https://mc.example:4433/');
    const fake = instances[0]!;

    // Identity-delegation for the promise/getter members.
    expect(transport.ready).toBe(fake.ready);
    expect(transport.closed).toBe(fake.closed);
    expect(transport.datagrams).toBe(fake.datagrams);

    // createBidirectionalStream delegates (returns a stream pair).
    const stream = await transport.createBidirectionalStream();
    expect(fake.bidiOpened).toBe(1);
    expect(stream.readable).toBeInstanceOf(ReadableStream);
    expect(stream.writable).toBeInstanceOf(WritableStream);

    // close forwards the optional close-info.
    transport.close({ closeCode: 7, reason: 'done' });
    expect(fake.closeCalls).toEqual([{ closeCode: 7, reason: 'done' }]);
  });
});
