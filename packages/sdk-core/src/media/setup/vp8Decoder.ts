// File: packages/sdk-core/src/media/setup/vp8Decoder.ts
//
// The production WebCodecs VP8 DECODER factory (story 3 C-B). Thin by
// construction — construct, configure from the slot's configured bounds,
// delegate — with every decision in the pure, scored builders of `./vp8.ts` and
// the scored slot pipeline (`pipeline/videoSlotPipeline.ts`). EXCLUDED from the
// node coverage gate for the same stated reason as `./vp8Encoder.ts`
// (`VideoDecoder` does not exist under `environment: 'node'`).
//
// Native WebCodecs only: no third-party codec (R-6).

import type { VideoDecoderFactory, VideoDecoderSeam } from './seams.js';

/** Production VP8 decoder factory. */
export const createVideoDecoder: VideoDecoderFactory = (options) => {
  const decoder = new VideoDecoder({ output: options.onOutput, error: options.onError });
  decoder.configure(options.config);
  const seam: VideoDecoderSeam = {
    decode(chunk): void {
      decoder.decode(
        new EncodedVideoChunk({
          type: chunk.isKeyframe ? 'key' : 'delta',
          timestamp: chunk.timestampUs,
          data: chunk.data,
        }),
      );
    },
    get decodeQueueSize(): number {
      return decoder.decodeQueueSize;
    },
    close(): void {
      if (decoder.state !== 'closed') decoder.close();
    },
  };
  return seam;
};
