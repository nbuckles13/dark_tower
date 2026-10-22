// File: packages/sdk-core/src/media/setup/vp8Encoder.ts
//
// The production WebCodecs VP8 encoder factory (story 3 C-A2). Thin by
// construction — re-probe the DIRECTED profile, construct, configure, delegate —
// with every decision in the pure, scored builders of `./vp8.ts`. EXCLUDED from
// the node coverage gate for the same stated reason as `./opus.ts`
// (`VideoEncoder` does not exist under `environment: 'node'`).

import type { EncodedVideoFrame, VideoEncoderFactory, VideoEncoderSeam } from './seams.js';
import { buildVp8EncoderConfig, probeVp8Support, vp8ProfileOf } from './vp8.js';

/**
 * Production VP8 encoder factory. Thin: re-probe the DIRECTED profile, then
 * construct and configure. Every decision lives in the pure builders above.
 */
export const createVideoEncoder: VideoEncoderFactory = async (options) => {
  const profile = vp8ProfileOf(options.encoding);
  await probeVp8Support(profile);
  const encoder = new VideoEncoder({
    output: (chunk: EncodedVideoChunk) => {
      const data = new Uint8Array(chunk.byteLength);
      chunk.copyTo(data);
      const frame: EncodedVideoFrame = {
        data,
        timestampUs: chunk.timestamp,
        key: chunk.type === 'key',
      };
      options.onOutput(frame);
    },
    error: options.onError,
  });
  encoder.configure(buildVp8EncoderConfig(profile));
  const seam: VideoEncoderSeam = {
    encode(frame: VideoFrame, keyFrame: boolean): void {
      encoder.encode(frame, { keyFrame });
    },
    get encodeQueueSize(): number {
      return encoder.encodeQueueSize;
    },
    close(): void {
      if (encoder.state !== 'closed') encoder.close();
    },
  };
  return seam;
};
