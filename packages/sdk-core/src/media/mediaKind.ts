// File: packages/sdk-core/src/media/mediaKind.ts
//
// THE canonical public media-kind tokens (story 3 C-A1). A type-only leaf that
// config, media, session and signaling all import, so the two-kind union is
// written once. `PublishKind`, `ReceiveSlotKind`, `ResumeKind` and the
// `mediaKind` fields of the public payloads are aliases of it.
//
// The wire enum is mapped onto these in ONE place,
// `signaling/slotStateMap.ts:mediaKindToken` (closed, fail-closed to
// `unknown`), and back in `wireMediaKind`. The metric label vocabulary
// (`setup/mediaMetrics.ts:MEDIA_KIND_LABELS`) is a separate, telemetry-owned
// projection with its own rules.

/** The two media kinds this SDK publishes, declares and mutes. */
export type MediaKindToken = 'audio' | 'video';

/**
 * A kind as RECEIVED from the wire: one of {@link MediaKindToken}, or `unknown`
 * for screen share, unspecified and any number the SDK does not name. `unknown`
 * renders no media and is never sent (fail closed).
 */
export type OpenMediaKindToken = MediaKindToken | 'unknown';
