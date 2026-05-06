// File: packages/test-utils/src/index.ts
//
// Public barrel for `@darktower/test-utils`. Test-only package — never
// publishable (`private: true` in package.json), never bundled into a
// production runtime artifact.
//
// `TestTokenSigner` is INTENTIONALLY NOT re-exported here. It lives at the
// `@darktower/test-utils/test-only/signer` sub-path with its own
// production-NODE_ENV import-time guard. The closed `exports` map in
// package.json (no `./*` wildcard) plus this barrel's omission are the two
// boundaries that keep the signer off the main import path.
//
// No metric / trace / log emissions originate from this package.

export type {
  IWebTransport,
  WebTransportBidirectionalStream,
  WebTransportCloseInfo,
} from './contracts/IWebTransport.js';

export type { MetricLabels, MetricsSink } from './contracts/MetricsSink.js';

export type {
  MeetingClaims,
  MeetingRole,
  ParticipantType,
  UserClaims,
} from './token-claims.js';

export {
  InMemoryMetricsSink,
  type RecordedKind,
  type RecordedMetric,
} from './InMemoryMetricsSink.js';

export {
  MockOTLPExporter,
  type MockOTLPResponseSpec,
  type OTLPExportPayload,
  type OTLPExportPayloadKind,
  type OTLPExportResult,
} from './MockOTLPExporter.js';

export { MockWebTransport } from './MockWebTransport.js';

export {
  TestTokenBuilder,
  type MeetingClaimsOverrides,
  type UserClaimsOverrides,
} from './TestTokenBuilder.js';

export {
  createIdFactory,
  createSeededRng,
  createSeededUuid,
} from './deterministic-ids.js';
