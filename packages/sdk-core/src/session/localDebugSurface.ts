// File: packages/sdk-core/src/session/localDebugSurface.ts
//
// The session's LOCAL debug surface (story 3 R-7, R-35): one statistics
// snapshot for an overlay and the read-only capture tracks for a loopback
// self-check, assembled from the session's EXISTING public media getters
// (`media`, `video`, `videoReceive`) — so `MeetingSession` itself gains no
// member. LOCAL ONLY, NEVER EXPORTED: see `media/lifecycle/localStats.ts`.

import type { AudioPipeline } from '../media/lifecycle/AudioPipeline.js';
import type { LocalCaptureTracks, LocalMediaStats } from '../media/lifecycle/localStats.js';
import type { VideoPublisher } from '../media/lifecycle/VideoPublisher.js';
import type { VideoReceiveView } from '../media/lifecycle/VideoReceiver.js';

/** What the snapshot is read from: a `MeetingSession` satisfies it structurally. */
export interface LocalStatsSource {
  readonly media: Pick<AudioPipeline, 'localStats'> | undefined;
  readonly video: Pick<VideoPublisher, 'localSendStats'> | undefined;
  readonly videoReceive: Pick<VideoReceiveView, 'localStats'> | undefined;
}

/** What the capture tracks are read from: a `MeetingSession` satisfies it structurally. */
export interface LocalTracksSource {
  readonly media: Pick<AudioPipeline, 'captureTrack'> | undefined;
  readonly video: Pick<VideoPublisher, 'captureTrack'> | undefined;
}

/**
 * One FRESH snapshot: every declared receive slot (audio, then video) and every
 * sent stream. Cumulative counters labelled by kind and slot id only; sample it
 * on a timer, never per frame. `undefined` before media starts and once the
 * pipeline stopped (leave / disconnect).
 */
export function localStatsOf(session: LocalStatsSource): LocalMediaStats | undefined {
  const audio = session.media?.localStats();
  if (audio === undefined) return undefined;
  const video = session.video?.localSendStats();
  return {
    receive: [...audio.receive, ...(session.videoReceive?.localStats() ?? [])],
    send: video === undefined ? [audio.send] : [audio.send, video],
  };
}

/**
 * This participant's OWN live capture tracks — audio while the microphone is
 * open, video only while the camera is lit (directed, not client-muted,
 * healthy). READ-ONLY references: the SDK owns them, and the publish path owns
 * the camera-light rule. Never `stop()`, `clone()`, `applyConstraints()` or
 * toggle `enabled`; a stopped track reads as a capture failure (R-19).
 */
export function localCaptureTracksOf(session: LocalTracksSource): LocalCaptureTracks {
  const audio = session.media?.captureTrack;
  const video = session.video?.captureTrack;
  return {
    ...(audio !== undefined ? { audio } : {}),
    ...(video !== undefined ? { video } : {}),
  };
}
