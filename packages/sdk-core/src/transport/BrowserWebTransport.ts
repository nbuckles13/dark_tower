// File: packages/sdk-core/src/transport/BrowserWebTransport.ts
//
// R-14: production `IWebTransport` implementation wrapping the browser
// `WebTransport` API.
//
// The dev-only self-signed-cert trust path (`serverCertificateHashes`) is gated
// behind the build-time literal `__DEV_TRUST_FINGERPRINT__` (Vite `define`,
// `false` in production). Because the literal is statically `false` in prod
// bundles, the entire `if (__DEV_TRUST_FINGERPRINT__) { ... }` block — and the
// `serverCertificateHashes` string itself — is dead-code-eliminated /
// tree-shaken out. `tests/bundle-content.test.ts` asserts the prod artifact
// contains neither `serverCertificateHashes` nor `__DEV_TRUST_FINGERPRINT__`.
//
// SECURITY: the fingerprint VALUE is supplied by the caller at runtime via
// `WebTransportConnectOptions` — there is no fingerprint constant anywhere in
// this source. Production builds use standard CA-validated TLS
// (`new WebTransport(url)` with no options literal). Per R-32 this module uses
// no `Math.random()` and introduces no security randomness.
//
// Public instance shape is EXACTLY `IWebTransport` (Pattern A drift guard). The
// `connect` factory is a module function, not an interface member, so consumers
// never couple to a wider type.

import type {
  IWebTransport,
  WebTransportBidirectionalStream,
  WebTransportCloseInfo,
} from './IWebTransport.js';
import type { WebTransportConnectOptions } from './types.js';
import { TransportError, TransportErrorCode } from './errors.js';

/**
 * Local declaration of just the browser `WebTransport` CONSTRUCTOR signature —
 * the one piece lib.dom doesn't give us cleanly under a Node `tsconfig`. The
 * constructed instance is typed as the canonical {@link IWebTransport}: the
 * browser transport structurally satisfies it (see `IWebTransport.ts` — the
 * shape mirrors the platform API by design), so there is no separate local
 * copy of the instance shape to drift.
 */
interface BrowserWebTransportCtor {
  new (url: string, options?: unknown): IWebTransport;
}

/**
 * Production `IWebTransport` backed by the platform `WebTransport`.
 *
 * Construct via {@link connect}, which initiates the handshake. The constructor
 * is kept narrow (wrap an already-created platform transport) so the class is
 * trivially testable and the connect-time policy lives in one place.
 */
export class BrowserWebTransport implements IWebTransport {
  readonly #wt: IWebTransport;

  constructor(wt: IWebTransport) {
    this.#wt = wt;
  }

  get ready(): Promise<void> {
    return this.#wt.ready;
  }

  get closed(): Promise<WebTransportCloseInfo> {
    return this.#wt.closed;
  }

  get datagrams(): {
    readonly readable: ReadableStream<Uint8Array>;
    readonly writable: WritableStream<Uint8Array>;
  } {
    return this.#wt.datagrams;
  }

  createBidirectionalStream(): Promise<WebTransportBidirectionalStream> {
    return this.#wt.createBidirectionalStream();
  }

  close(info?: WebTransportCloseInfo): void {
    this.#wt.close(info);
  }
}

/**
 * Resolve the platform `WebTransport` constructor. Throws a clear error in
 * environments that lack it (e.g. unsupported browser / Node without polyfill).
 */
function resolveWebTransportCtor(): BrowserWebTransportCtor {
  const ctor = (globalThis as { WebTransport?: BrowserWebTransportCtor }).WebTransport;
  if (typeof ctor !== 'function') {
    throw new TransportError(
      TransportErrorCode.Unavailable,
      'WebTransport is not available in this environment',
    );
  }
  return ctor;
}

/**
 * Connect to `url` and return a ready-to-use {@link BrowserWebTransport}.
 *
 * Production path: standard CA-validated TLS, no certificate pinning.
 *
 * Dev path (only when the dev-trust build flag is `true`): if
 * `options.devCertificateHashes` is supplied, the hashes are forwarded to the
 * platform `WebTransport` to trust a self-signed cert. The fingerprint value is
 * the caller's runtime config. This whole branch is eliminated from production
 * bundles.
 *
 * The factory is intentionally NOT a method on `IWebTransport` — see the file
 * header. Awaiting `transport.ready` after this resolves observes the open
 * hook.
 */
export function connect(
  url: string,
  options?: WebTransportConnectOptions,
): BrowserWebTransport {
  const Ctor = resolveWebTransportCtor();

  if (__DEV_TRUST_FINGERPRINT__) {
    // DEV ONLY — dead-code-eliminated in production (flag literal is `false`).
    // The browser option key is constructed here at runtime so the literal
    // never appears in production executable code or in the public `.d.ts`.
    const hashes = options?.devCertificateHashes;
    if (hashes !== undefined && hashes.length > 0) {
      const devOptions = { serverCertificateHashes: hashes };
      return new BrowserWebTransport(new Ctor(url, devOptions));
    }
  }

  // Production path (and dev without pinned hashes): standard CA-validated TLS.
  return new BrowserWebTransport(new Ctor(url));
}
