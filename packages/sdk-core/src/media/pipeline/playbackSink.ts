// File: packages/sdk-core/src/media/pipeline/playbackSink.ts
//
// HOT PATH. Per-frame code only: no logging, no metric names, no label
// construction. `enqueue` runs 50 times a second.
//
// A scheduled playback sink: each decoded `AudioData` is copied into an
// `AudioBuffer` and started at a monotonically advancing playhead, so frames
// play back-to-back rather than all at the same instant.
//
// ---------------------------------------------------------------------------
// THE PLAYHEAD RESETS WHEN IT FALLS BEHIND, AND THAT IS THE WHOLE DESIGN
// ---------------------------------------------------------------------------
//
// If the playhead is behind `currentTime` — the first frame of a session, or
// after a gap where nothing arrived — scheduling relative to it would queue
// audio in the past, which the platform plays immediately and all at once.
// Re-seating the playhead at `currentTime + lead` on every such occasion turns a
// gap into silence followed by resumed audio, which is what a listener expects,
// instead of a burst.
//
// The lead is one frame: enough that a frame arriving slightly late still lands
// in the future, small enough that it is not an added latency budget. This is
// deliberately NOT a jitter buffer — buffering strategy is a later story, and a
// number chosen here would be the thing that later has to be unpicked.

import type { PlaybackSink } from '../setup/seams.js';

/** The scheduling lead, in frames of the decoded audio's own duration. */
const LEAD_FRAMES = 1;

/**
 * Build a sink that plays decoded frames through `context`.
 *
 * @param channels expected channel count. A frame arriving with a different
 * count is played on the channels it has, up to this many — the decoder is
 * configured from the same config the context is, so a mismatch is a defect
 * rather than a case to accommodate.
 */
export function createScheduledPlaybackSink(
  context: BaseAudioContext & { readonly currentTime: number; readonly destination: AudioNode },
  channels: number,
): PlaybackSink {
  let playheadSeconds = 0;
  let closed = false;

  return {
    enqueue(data: AudioData): void {
      if (closed) {
        // Ownership was transferred to this sink, so a late frame must still be
        // released or the platform's audio buffers leak.
        data.close();
        return;
      }
      try {
        const frameChannels = Math.min(channels, data.numberOfChannels);
        const buffer = context.createBuffer(frameChannels, data.numberOfFrames, data.sampleRate);
        for (let channel = 0; channel < frameChannels; channel += 1) {
          // `f32-planar` is what WebCodecs produces for Opus, and it is what
          // `AudioBuffer` wants per channel — so this is a copy, never a format
          // conversion.
          const target = buffer.getChannelData(channel);
          data.copyTo(target, { planeIndex: channel, format: 'f32-planar' });
        }

        const source = context.createBufferSource();
        source.buffer = buffer;
        source.connect(context.destination);

        const frameSeconds = data.numberOfFrames / data.sampleRate;
        const lead = frameSeconds * LEAD_FRAMES;
        const now = context.currentTime;
        // Behind the clock means the previous run has ended: re-seat rather than
        // schedule in the past.
        if (playheadSeconds < now + lead) playheadSeconds = now + lead;
        source.start(playheadSeconds);
        playheadSeconds += frameSeconds;
      } finally {
        // ALWAYS released, including when the copy above throws. `AudioData`
        // holds a platform-side buffer that is not garbage collected on our
        // schedule, so a leaked frame is a real resource leak at 50/s.
        data.close();
      }
    },

    close(): void {
      closed = true;
      playheadSeconds = 0;
    },
  };
}
