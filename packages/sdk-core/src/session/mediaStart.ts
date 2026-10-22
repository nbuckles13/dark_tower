// File: packages/sdk-core/src/session/mediaStart.ts
//
// What `MeetingSession.startMedia()` BUILDS, kept out of the session class (the
// same reason `mediaSelection.ts` and `mediaWiring.ts` exist: one small,
// separately testable unit per decision, and a session class that stays a
// coordinator):
//
//   * the session's ONE `TransmitKeyManager` (one per sender — the nonce
//     boundary, `media/lifecycle/transmitKeys.ts`) and the ONE KEK-rotation
//     subscription beside it (R-13; a video-only session rotates too);
//   * the audio pipeline and, when video is published, the video publisher,
//     both on that manager;
//   * the release that runs on EVERY exit — teardown and a start failure —
//     stopping both paths and only then clearing the keys;
//   * the join-time VP8 probe and the fault a failed one raises (R-8).

import type { ConfigValueSource, MediaConfig } from '../config/clientConfig.js';
import {
  AudioPipeline,
  MediaFaultStage,
  type MediaFault,
} from '../media/lifecycle/AudioPipeline.js';
import type {
  AudioPipelineOptions,
  AudioPipelineStartOptions,
  PipelineKekSource,
} from '../media/lifecycle/AudioPipeline.js';
import type { MuteSnapshot, MuteState } from '../media/lifecycle/muteState.js';
import { TransmitKeyManager, rotateOnKekChange } from '../media/lifecycle/transmitKeys.js';
import { VideoPublisher } from '../media/lifecycle/VideoPublisher.js';
import type { MediaTransport } from '../media/MediaTransport.js';
import type { ReceiveVerificationRecorder } from '../media/pipeline/receiveVerification.js';
import type { MeetingIdentity } from '../media/setup/identity.js';
import type { MediaMetrics } from '../media/setup/mediaMetrics.js';
import type { RosterIdentityKeys } from '../media/setup/rosterKeys.js';
import type {
  AudioDecoderFactory,
  AudioEncoderFactory,
  CaptureSourceFactory,
  PlaybackSinkFactory,
  VideoCaptureSourceFactory,
  VideoDecoderFactory,
  VideoEncoderFactory,
} from '../media/setup/seams.js';
import { VideoReceiver, type VideoReceiveView } from '../media/lifecycle/VideoReceiver.js';
import { createVideoDecoder } from '../media/setup/vp8Decoder.js';
import { VideoCodecUnsupportedError, type Vp8Profile } from '../media/setup/vp8.js';
import { createVideoEncoder } from '../media/setup/vp8Encoder.js';
import { createAudioDecoder, createAudioEncoder } from '../media/setup/opus.js';
import { createAudioContextPlaybackSink } from '../media/setup/audioPlayback.js';
import type { PublishKind, ReceiveSlotDeclaration } from '../signaling/events.js';
import { getMetricExportIntervalMs } from '../telemetry/telemetryConfig.js';
import type {
  CaptureSourceInfo,
  MeetingSessionOptions,
  VideoCaptureSourceInfo,
  VideoSlotRenderChangedEvent,
} from './events.js';
import {
  declaredSlotIds,
  selectCaptureSource,
  selectVideoCaptureSource,
} from './mediaSelection.js';
import { pipelineListeners } from './mediaWiring.js';
import type { SignalingMediaState } from './signalingMediaState.js';

/** The injectable media seams a session was constructed with. */
export interface SessionMediaSeams {
  readonly captureFactory: CaptureSourceFactory | undefined;
  readonly encoderFactory: AudioEncoderFactory;
  readonly decoderFactory: AudioDecoderFactory;
  readonly playbackFactory: PlaybackSinkFactory;
  readonly videoCaptureFactory: VideoCaptureSourceFactory | undefined;
  readonly videoEncoderFactory: VideoEncoderFactory;
  readonly videoDecoderFactory: VideoDecoderFactory;
  readonly timers: Pick<
    MeetingSessionOptions,
    'setTimeout' | 'clearTimeout' | 'setInterval' | 'clearInterval'
  >;
}

/** The seams from the session options, production defaults filled in. */
export function sessionMediaSeams(options: MeetingSessionOptions): SessionMediaSeams {
  return {
    captureFactory: options.captureFactory,
    encoderFactory: options.encoderFactory ?? createAudioEncoder,
    decoderFactory: options.decoderFactory ?? createAudioDecoder,
    playbackFactory: options.playbackFactory ?? createAudioContextPlaybackSink,
    videoCaptureFactory: options.videoCaptureFactory,
    videoEncoderFactory: options.videoEncoderFactory ?? createVideoEncoder,
    videoDecoderFactory: options.videoDecoderFactory ?? createVideoDecoder,
    timers: {
      ...(options.setTimeout ? { setTimeout: options.setTimeout } : {}),
      ...(options.clearTimeout ? { clearTimeout: options.clearTimeout } : {}),
      ...(options.setInterval ? { setInterval: options.setInterval } : {}),
      ...(options.clearInterval ? { clearInterval: options.clearInterval } : {}),
    },
  };
}

/** Everything {@link buildSessionMedia} needs from the session. */
export interface SessionMediaDeps {
  readonly config: MediaConfig;
  readonly configSource: ConfigValueSource;
  readonly metrics: MediaMetrics;
  readonly kekSource: PipelineKekSource;
  readonly roster: RosterIdentityKeys;
  readonly senderId: number;
  readonly identity: MeetingIdentity;
  readonly publish: readonly PublishKind[];
  readonly declaration: readonly ReceiveSlotDeclaration[];
  readonly media: MediaTransport | undefined;
  readonly mute: MuteState;
  readonly reportMute: (state: MuteSnapshot) => void;
  readonly seams: SessionMediaSeams;
  readonly verification: ReceiveVerificationRecorder | undefined;
  readonly clock: () => number;
  /** The session's event outlets, forwarded verbatim (never recomputed). */
  readonly emit: {
    readonly muteChanged: (state: MuteSnapshot) => void;
    readonly firstMediaFrame: (elapsedMs: number) => void;
    readonly mediaFault: (fault: MediaFault) => void;
    readonly videoSlotRenderChanged: (event: VideoSlotRenderChangedEvent) => void;
  };
  /** The signalling-derived media state the pipelines attach to (E3 replay). */
  readonly mediaState: Pick<
    SignalingMediaState,
    'attach' | 'effectiveTargets' | 'videoMuteState' | 'onVideoServerMuteLifted'
  >;
}

/** What {@link buildSessionMedia} built. */
export interface SessionMedia {
  readonly pipeline: AudioPipeline;
  readonly video: VideoPublisher | undefined;
  /** The video receive path, when this client declared video slots (story 3 C-B). Internal. */
  readonly videoReceiver: VideoReceiver | undefined;
  /** The narrow embedder view of it (security F4) — what the session hands out. */
  readonly videoView: VideoReceiveView | undefined;
  readonly captureInfo: CaptureSourceInfo;
  readonly videoCaptureInfo: VideoCaptureSourceInfo | undefined;
  /**
   * Start audio, then video. On failure, {@link release} runs (every exit
   * clears the keys) and the error is rethrown.
   */
  start(options: AudioPipelineStartOptions): Promise<void>;
  /**
   * Stop both paths, wait for audio AND video to halt their submits, then drop the KEK
   * subscription and CLEAR the transmit keys (`clear()` then races no
   * in-flight seal). Idempotent; never rejects. Runs on EVERY exit.
   */
  release(): Promise<void>;
  /**
   * Story 2 R-18, EVERY kind: drop `senderId`'s cached transmit keys in the
   * audio pipeline AND the video receiver. Replay state is kept in both.
   */
  purgeTransmitKeys(senderId: number): void;
}

/** Build the session's media (not started). See the module header. */
export function buildSessionMedia(deps: SessionMediaDeps): SessionMedia {
  const { seams, media } = deps;
  const transmitKeys = new TransmitKeyManager(deps.senderId);
  let unsubscribe: (() => void) | undefined;

  const capture = selectCaptureSource(seams.captureFactory, deps.senderId);
  const options: AudioPipelineOptions = {
    config: deps.config,
    metrics: deps.metrics,
    kekSource: deps.kekSource,
    roster: deps.roster,
    senderId: deps.senderId,
    transmitKeys,
    identity: deps.identity,
    // Receive side only: the audio ids of the ONE declaration.
    declaredSlotIds: declaredSlotIds(deps.declaration, 'audio'),
    senderFor: (url) => media?.getDatagramChannel(url),
    readableFor: (url) => media?.getDatagramChannel(url)?.readable,
    mute: deps.mute,
    reportMute: deps.reportMute,
    captureFactory: capture.factory,
    // Not publishing audio (video-only or receive-only): no microphone at all.
    publishesAudio: deps.publish.includes('audio'),
    captureSource: capture.info.mode,
    metricExportIntervalMs: getMetricExportIntervalMs(),
    encoderFactory: seams.encoderFactory,
    decoderFactory: seams.decoderFactory,
    playbackFactory: seams.playbackFactory,
    ...(deps.verification ? { verification: deps.verification } : {}),
    clock: deps.clock,
    ...(seams.timers.setInterval ? { setInterval: seams.timers.setInterval } : {}),
    ...(seams.timers.clearInterval ? { clearInterval: seams.timers.clearInterval } : {}),
  };
  const pipeline = new AudioPipeline(options);

  let video: VideoPublisher | undefined;
  let videoCaptureInfo: VideoCaptureSourceInfo | undefined;
  if (deps.publish.includes('video')) {
    const videoCapture = selectVideoCaptureSource(seams.videoCaptureFactory, deps.senderId);
    videoCaptureInfo = videoCapture.info;
    video = new VideoPublisher({
      config: deps.config,
      configSource: deps.configSource,
      metrics: deps.metrics.video,
      transmitKeys,
      kek: deps.kekSource,
      identity: deps.identity,
      mute: deps.mute,
      reportMute: deps.reportMute,
      onMuteChanged: deps.emit.muteChanged,
      captureFactory: videoCapture.factory,
      encoderFactory: seams.videoEncoderFactory,
      openStream: (url) => media?.openUniStream(url),
      fault: deps.emit.mediaFault,
      clock: deps.clock,
      ...seams.timers,
    });
  }

  // The video RECEIVE path (story 3 C-B): only when video slots were declared.
  const videoSlotIds = declaredSlotIds(deps.declaration, 'video');
  let videoReceiver: VideoReceiver | undefined;
  if (videoSlotIds.length > 0) {
    videoReceiver = new VideoReceiver({
      config: deps.config,
      configSource: deps.configSource,
      metrics: deps.metrics.video,
      kekSource: deps.kekSource,
      roster: deps.roster,
      selfSenderId: deps.senderId,
      videoMuteState: (senderId) => deps.mediaState.videoMuteState(senderId),
      declaredSlotIds: videoSlotIds,
      incomingStreamsFor: (url) => media?.incomingUniStreamsFor(url),
      sendRecoveryRequest: (url, request) =>
        media ? media.sendRecoveryRequest(url, request) : Promise.reject(new Error('no media')),
      decoderFactory: seams.videoDecoderFactory,
      metricExportIntervalMs: getMetricExportIntervalMs(),
      fault: deps.emit.mediaFault,
      onRenderChanged: (slotId, state) => deps.emit.videoSlotRenderChanged({ slotId, state }),
      ...(deps.verification ? { verification: deps.verification } : {}),
      clock: deps.clock,
      ...(seams.timers.setInterval ? { setInterval: seams.timers.setInterval } : {}),
      ...(seams.timers.clearInterval ? { clearInterval: seams.timers.clearInterval } : {}),
    });
    const receiver = videoReceiver;
    deps.mediaState.onVideoServerMuteLifted((senderId) => receiver.videoServerMuteLifted(senderId));
  }

  let released: Promise<void> | undefined;
  const release = (): Promise<void> => {
    released ??= (async () => {
      const videoDrained = video?.stop();
      deps.mediaState.onVideoServerMuteLifted(undefined);
      const receiverStopped = videoReceiver?.stop();
      try {
        await pipeline.stop();
        await receiverStopped;
        // Video's in-flight seals too (its submit chain never rejects).
        await videoDrained;
      } catch {
        // Teardown is already complete by the time a fault can be raised —
        // `dispose()` has run — so resources are released either way.
      }
      unsubscribe?.();
      unsubscribe = undefined;
      transmitKeys.clear();
    })();
    return released;
  };

  // The remaining absence-signals (§5/§10), forwarded verbatim (see
  // `session/events.ts`) — subscribed before `start()`, so nothing the pipeline
  // emits is missed (`firstMediaFrame` is armed inside `start()`). Nothing is
  // recomputed: `muteChanged` is the ONLY mute source a UI may render from.
  pipeline.on('muteChanged', deps.emit.muteChanged);
  pipeline.on('firstMediaFrame', deps.emit.firstMediaFrame);
  pipeline.on('fault', deps.emit.mediaFault);

  // What MC directs this client to SEND (per published kind) and which slots it
  // RECEIVES (`mediaWiring.ts:pipelineListeners`); E3: what MC already sent
  // (from the join-carried declaration) is replayed ONCE as they attach.
  deps.mediaState.attach(
    pipelineListeners({
      publish: deps.publish,
      pipeline,
      ...(video ? { video } : {}),
      ...(videoReceiver ? { videoReceiver } : {}),
      defaultBitrateBps: deps.config.audio.defaultBitrateBps,
      metrics: deps.metrics,
      fault: deps.emit.mediaFault,
      effectiveTargets: (kind, nonEmpty) => deps.mediaState.effectiveTargets(kind, nonEmpty),
    }),
  );

  const start = async (startOptions: AudioPipelineStartOptions): Promise<void> => {
    try {
      await pipeline.start(startOptions);
      video?.start();
      videoReceiver?.start();
    } catch (err) {
      await release();
      throw err;
    }
  };

  // THE ONE KEK-rotation subscription, beside the manager it rotates (R-13; a
  // video-only session rotates too). Registered LAST — after every synchronous
  // constructor above, so nothing can throw with it left unregistered — and
  // still before `start()`, so before any capture can mint.
  unsubscribe = rotateOnKekChange(deps.kekSource, transmitKeys);

  const purgeTransmitKeys = (senderId: number): void => {
    pipeline.purgeTransmitKeys(senderId);
    videoReceiver?.purgeTransmitKeys(senderId);
  };

  return {
    pipeline,
    video,
    videoReceiver,
    videoView: videoReceiver?.view(),
    captureInfo: capture.info,
    videoCaptureInfo,
    start,
    release,
    purgeTransmitKeys,
  };
}

/** The join-time VP8 probe result: `undefined` when video is not in use. */
export type Vp8ProbeResult = { readonly ok: true } | { readonly ok: false; readonly error: Error };

/**
 * Run the VP8 probe when video is published or received (story 3 R-8), at the
 * capability-probe profile (`clientConfig.ts:VideoConfig.capabilityProbe`).
 * Never rejects: a failure is held and fails `startMedia()` loudly.
 */
export async function probeVideoCodec(
  usesVideo: boolean,
  profile: Vp8Profile,
  probe: (profile: Vp8Profile) => Promise<void>,
): Promise<Vp8ProbeResult | undefined> {
  if (!usesVideo) return undefined;
  try {
    await probe(profile);
    return { ok: true };
  } catch (err) {
    return {
      ok: false,
      error:
        err instanceof VideoCodecUnsupportedError
          ? err
          : new VideoCodecUnsupportedError(
              'platform',
              'the VP8 capability probe failed, so this client cannot send or receive video',
            ),
    };
  }
}

/** The fatal fault for a failed VP8 probe at media start. Bounded literal text. */
export function vp8StartFault(error: Error): MediaFault {
  return {
    stage:
      error instanceof VideoCodecUnsupportedError && error.stage === 'decoder'
        ? MediaFaultStage.Decoder
        : MediaFaultStage.Encoder,
    message: 'this browser cannot encode and decode VP8 video, so media cannot start',
    fatal: true,
  };
}
