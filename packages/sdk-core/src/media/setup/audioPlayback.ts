// File: packages/sdk-core/src/media/setup/audioPlayback.ts
//
// `AudioContext` construction for playback, and the suspended-context check.
//
// ---------------------------------------------------------------------------
// A SUSPENDED CONTEXT IS THE SILENT-NO-AUDIO CASE, SO IT IS LOUD
// ---------------------------------------------------------------------------
//
// A context created without a user gesture is suspended by the autoplay policy.
// Frames then decode successfully, the sink accepts them, every counter moves,
// and NOTHING IS AUDIBLE. That is precisely the failure the loopback story exists
// to localise, and it is the one where every green signal is genuinely green.
//
// So: after construction, `resume()` is attempted and the state is checked. A
// context that is not `running` is a typed error, not a warning and not a value
// the pipeline plays into.
//
// The SINK ITSELF lives in `../pipeline/playbackSink.ts` because its `enqueue`
// is per-frame code. This module is setup: it builds the context, checks it, and
// hands it over.

import { createScheduledPlaybackSink } from '../pipeline/playbackSink.js';
import type { PlaybackSink, PlaybackSinkFactory } from './seams.js';

/** Bounded playback failure reasons. */
export const PlaybackFailure = {
  /** The platform has no `AudioContext`. */
  PlatformUnsupported: 'platform_unsupported',
  /**
   * The context exists but is not `running` — almost always the autoplay policy
   * suspending a context created without a user gesture.
   */
  ContextNotRunning: 'context_not_running',
} as const;

/** One of {@link PlaybackFailure}. */
export type PlaybackFailure = (typeof PlaybackFailure)[keyof typeof PlaybackFailure];

/** A typed playback failure. Bounded reason; never a platform string. */
export class MediaPlaybackError extends Error {
  readonly reason: PlaybackFailure;

  constructor(reason: PlaybackFailure, message: string) {
    super(message);
    this.name = 'MediaPlaybackError';
    this.reason = reason;
  }
}

/**
 * Production playback sink factory.
 *
 * Thin by construction: build, resume, check, delegate. The scheduling logic —
 * the part with decisions in it — lives in the pipeline sink and is unit-tested
 * against an injected context.
 */
export const createAudioContextPlaybackSink: PlaybackSinkFactory = async (options) => {
  const Ctor = (globalThis as { AudioContext?: typeof AudioContext }).AudioContext;
  if (!Ctor) {
    throw new MediaPlaybackError(
      PlaybackFailure.PlatformUnsupported,
      'this platform does not expose AudioContext, so decoded audio cannot be played',
    );
  }
  const context = new Ctor({ sampleRate: options.sampleRateHz });
  // Attempted, not assumed. `resume()` succeeds when a user gesture has already
  // unlocked audio and is a no-op when the context is already running.
  await context.resume().catch(() => {
    // The rejection carries nothing actionable; the STATE check below is the
    // control, and it fires identically whether resume rejected or silently
    // left the context suspended.
  });
  if (context.state !== 'running') {
    await context.close().catch(() => {
      // Closing a context that never started can reject; the caller is already
      // being told the real failure and a close error would displace it.
    });
    throw new MediaPlaybackError(
      PlaybackFailure.ContextNotRunning,
      'the audio context is not running, so decoded audio would be silently discarded. ' +
        'This is normally the browser autoplay policy suspending a context created without a ' +
        'user gesture: start media from a click or keypress handler.',
    );
  }
  const sink: PlaybackSink = createScheduledPlaybackSink(context, options.channels);
  return sink;
};
