// File: packages/sdk-core/src/media/lifecycle/localStats.ts
//
// THE LOCAL STATISTICS SNAPSHOT (story 3 R-35): what a client-side debug
// overlay may read about this client's own media path. A TYPE-ONLY module.
//
// LOCAL ONLY, NEVER EXPORTED. These shapes carry per-slot and per-stream BYTE
// totals, from which a per-stream size series can be derived — exactly what
// ADR-0036 §11 forbids exporting. So:
//
//   * nothing here is, or may be, imported by `../setup/mediaMetrics.ts`, a
//     MetricsSink, the telemetry exporter or the web app's e2e bus (the bus
//     projects `VideoSlotEvidence` / `VideoSendCounts` / `MediaFrameCounts`,
//     which carry no sizes, and is deliberately NOT given these);
//   * every number is a CUMULATIVE counter maintained beside an existing
//     production counter (plain integer `++` on the forward path, read here on
//     a timer by the embedder) — rates are the embedder's job, from two
//     successive snapshots, so no time-ordered size series exists in the SDK;
//   * rows are labelled by kind and MC's slot id only — NO sender id, key id,
//     generation, timestamp or GoP value.
//
// Scanned by `packages/web-app/tests/debugNeverExported.test.ts`, which also
// denies this file any import of `mediaMetrics`, `telemetry/` or a sink name
// (so the keyframe-cause type is reached through `VideoSendCounts`).

import type { RejectReason } from '../frame/rejectReason.js';
import type { MediaKindToken } from '../mediaKind.js';
import type { VideoSendCounts } from './VideoPublisher.js';

/** One declared receive slot. Cumulative counts; sampled, never per frame. */
export interface LocalReceiveSlotStats {
  readonly kind: MediaKindToken;
  /** MC's slot id (the relay `stream_id` this client declared). */
  readonly slotId: number;
  /** Frames that arrived naming this slot. */
  readonly packets: number;
  /** Their wire bytes (a cumulative sum). */
  readonly bytes: number;
  /**
   * Frames decoded. Video: the slot decoder's in-bounds outputs. Audio: the
   * decoder outputs of the lane of the sender MC currently assigns here (a lane
   * is per sender, so this restarts when the slot's sender changes).
   */
  readonly framesDecoded: number;
  /**
   * Frames waiting before the decoder. Video: the slot's jitter buffer. Audio:
   * the lane's pre-decoder hold (audio has no jitter buffer by design —
   * `pipeline/playbackSink.ts`).
   */
  readonly queueDepth: number;
  /** Wire/policy drops attributed to this slot, by closed reject token. */
  readonly drops: Readonly<Record<RejectReason, number>>;
  /** Post-accept local drops (decode queue, expiry, never-render). Video only; `undefined` for audio. */
  readonly localDrops: number | undefined;
  /** Accepted keyframes that reached the slot. `undefined` for audio (no keyframes). */
  readonly keyframesSeen: number | undefined;
  /** Time in the unfulfilled state so far (video, R-30); `undefined` for audio. */
  readonly unfulfilledMs: number | undefined;
}

/** One stream this client sends. Cumulative counts; sampled, never per frame. */
export interface LocalSendStreamStats {
  readonly kind: MediaKindToken;
  /** Frames written, summed over target handlers. */
  readonly packets: number;
  /** Their wire bytes, summed over target handlers (a cumulative sum). */
  readonly bytes: number;
  /** Keyframes produced, by cause. `undefined` for audio. */
  readonly keyframes: VideoSendCounts['keyframes'] | undefined;
  /** Groups the publisher abandoned (backlog cap). `undefined` for audio. */
  readonly groupsAbandoned: number | undefined;
}

/** The whole local snapshot. A FRESH object per read; holds no live reference. */
export interface LocalMediaStats {
  readonly receive: readonly LocalReceiveSlotStats[];
  readonly send: readonly LocalSendStreamStats[];
}

/**
 * This participant's own live capture tracks, for the local R-7 self-check.
 * READ-ONLY references: the SDK owns their lifecycle (see
 * `localCaptureTracksOf(session)`). Never stop, clone, `applyConstraints`
 * or toggle `enabled`.
 */
export interface LocalCaptureTracks {
  readonly audio?: MediaStreamTrack;
  readonly video?: MediaStreamTrack;
}
