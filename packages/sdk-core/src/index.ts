// File: packages/sdk-core/src/index.ts
//
// Public barrel for `@darktower/sdk-core`. The package `exports` map is closed
// (no `./*` wildcard) — this barrel is the only entry point. Production browser
// SDK core: WebTransport transport abstraction (R-13/R-14) and length-prefix
// wire framing (R-16). The full SignalingClient (proto encode/decode, join
// flow) is a later task and intentionally absent here.
//
// No metric / trace / log emissions originate from this package yet
// (observability sinks land in task #12).

export {
  encodeFrame,
  FrameDecoder,
  FramingError,
  FramingErrorCode,
  MAX_MESSAGE_SIZE,
} from './framing/length-prefix.js';

export type {
  IWebTransport,
  WebTransportBidirectionalStream,
  WebTransportCloseInfo,
} from './transport/IWebTransport.js';

export type {
  DevCertificateHash,
  WebTransportConnectOptions,
} from './transport/types.js';

export { BrowserWebTransport, connect } from './transport/BrowserWebTransport.js';

export { TransportError, TransportErrorCode } from './transport/errors.js';
