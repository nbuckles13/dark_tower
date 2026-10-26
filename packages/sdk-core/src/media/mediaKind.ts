// File: packages/sdk-core/src/media/mediaKind.ts
//
// THE canonical public media-kind tokens (story 3 C-A1). A dependency-free leaf that
// config, media, session and signaling all import, so the two-kind union is
// written once. `PublishKind`, `ReceiveSlotKind`, `ResumeKind` and the
// `mediaKind` fields of the public payloads are aliases of it.
//
// The wire enum is mapped onto these in ONE place,
// `signaling/slotStateMap.ts:mediaKindToken` (closed, fail-closed to
// `unknown`), and back in `wireMediaKind`. The metric label vocabulary
// (`setup/mediaMetrics.ts:MEDIA_KIND_LABEL_PROJECTION`, keyed by the wire enum)
// is a separate, telemetry-owned projection with its own rules.

/** The two media kinds this SDK publishes, declares and mutes. */
export type MediaKindToken = 'audio' | 'video';

/**
 * Every {@link MediaKindToken}, in the order the SDK and UI list them — DERIVED
 * from an exhaustive `Record` over the union, so a new kind fails to compile
 * here rather than silently missing from every per-kind loop that iterates
 * this. Iterate it instead of hand-pairing `audio` / `video`.
 */
const KIND_ORDER: Readonly<Record<MediaKindToken, number>> = { audio: 0, video: 1 };
export const MEDIA_KIND_TOKENS: readonly MediaKindToken[] = Object.freeze(
  (Object.keys(KIND_ORDER) as MediaKindToken[]).sort((a, b) => KIND_ORDER[a] - KIND_ORDER[b]),
);

/** Build a per-kind record by calling `f` once per kind, in {@link MEDIA_KIND_TOKENS} order. */
export function perMediaKind<T>(f: (kind: MediaKindToken) => T): Record<MediaKindToken, T> {
  return Object.fromEntries(MEDIA_KIND_TOKENS.map((k) => [k, f(k)])) as Record<MediaKindToken, T>;
}

/**
 * A kind as RECEIVED from the wire: one of {@link MediaKindToken}, or `unknown`
 * for screen share, unspecified and any number the SDK does not name. `unknown`
 * renders no media and is never sent (fail closed).
 */
export type OpenMediaKindToken = MediaKindToken | 'unknown';
