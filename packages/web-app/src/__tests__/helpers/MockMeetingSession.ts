// A `MeetingSession` test double for the demo view tests (R-43). Reuses
// sdk-core's exported `TypedEventEmitter`; exposes `fire` (over the protected
// `emit`) plus the `join`/`disconnect`/`state`/`startMedia` surface the demo
// views drive. Local to web-app tests (mock-home adjudicated with @test/@dry at
// review).
//
// NOT a near-clone of `sdk-svelte`'s `MockMeetingSession`, and the divergence is
// load-bearing rather than incidental: that one implements `BoundMeetingSession`
// (a `state` field plus `on`) because the adapter only ever SUBSCRIBES, while
// this one models the surface a VIEW DRIVES — join, disconnect, and now a media
// pipeline it can mute and re-point. The shared mechanism (`TypedEventEmitter`)
// is already imported from sdk-core by both rather than reimplemented in either.

import { MeetingSessionState, TypedEventEmitter } from '@darktower/sdk-core';
import type {
  CaptureSourceInfo,
  LocalStatsSource,
  VideoCaptureSourceInfo,
  VideoReceiveView,
  VideoSendCounts,
  VideoSink,
  JoinedEvent,
  MediaFrameCounts,
  ReceiveSlotsDiagnostics,
  MeetingSessionEventMap,
  MuteSnapshot,
} from '@darktower/sdk-core';

/**
 * A stand-in for the running `AudioPipeline`.
 *
 * **It echoes `muteChanged` back through the session**, exactly as the real
 * `MeetingSession.startMedia` bridge does. That echo is the whole point: the
 * view is asserted to render the indicator from the SDK's report rather than
 * from the click, and a double that did not echo would let a view which set its
 * own local boolean pass.
 */
export class FakeAudioPipeline {
  readonly muteCalls: boolean[] = [];
  readonly deviceCalls: Array<string | undefined> = [];
  /** Sampled by the E2E bus. Mutable so a test can advance it. */
  frameCounts: MediaFrameCounts = {
    framesSent: 0,
    framesReceived: 0,
    framesAccepted: 0,
    framesDropped: 0,
    lastDropReason: undefined,
  };
  /** Sampled by the E2E bus's `sendTracks` (story 3). */
  captureOpen = true;
  /** Story 3 R-7: the read-only microphone track (`localCaptureTracksOf`). Mutable. */
  captureTrack: MediaStreamTrack | undefined;
  /**
   * Story 3 R-35: the local statistics snapshot (`localStatsOf` reads it).
   * `localStatsCalls` counts reads, so a test can prove the overlay samples only
   * while open. Mutable.
   */
  localStatsValue: ReturnType<NonNullable<LocalStatsSource['media']>['localStats']> = {
    receive: [],
    send: { kind: 'audio', packets: 0, bytes: 0, keyframes: undefined, groupsAbandoned: undefined },
  };
  localStatsCalls = 0;
  localStats(): ReturnType<NonNullable<LocalStatsSource['media']>['localStats']> {
    this.localStatsCalls += 1;
    return this.localStatsValue;
  }
  /** Make the next `setCaptureDevice` reject, as an unavailable device would. */
  failNextDeviceChange = false;

  #audioMuted = false;
  readonly #echo: (snapshot: MuteSnapshot) => void;

  /** The pipeline's current audio mute (for the session's combined echo). */
  get audioMuted(): boolean {
    return this.#audioMuted;
  }

  constructor(echo: (snapshot: MuteSnapshot) => void) {
    this.#echo = echo;
  }

  setAudioMuted(muted: boolean): void {
    this.muteCalls.push(muted);
    if (this.#audioMuted === muted) return;
    this.#audioMuted = muted;
    this.#echo({ audioMuted: muted, videoMuted: false });
  }

  async setCaptureDevice(deviceId: string | undefined): Promise<void> {
    if (this.failNextDeviceChange) {
      this.failNextDeviceChange = false;
      throw new Error('device unavailable');
    }
    this.deviceCalls.push(deviceId);
  }
}

export class MockMeetingSession extends TypedEventEmitter<MeetingSessionEventMap> {
  state: MeetingSessionState = MeetingSessionState.Idle;
  joinCalls = 0;
  disconnectCalls = 0;

  /** Story 3 C-C2: observe the instant `join()` runs (ordering assertions). */
  onJoin: (() => void) | undefined;

  async join(): Promise<JoinedEvent> {
    this.joinCalls += 1;
    this.onJoin?.();
    return {
      participantId: 'self',
      senderId: 1,
      receiveSlotCaps: { audio: 8, video: 8 },
      existingParticipants: [],
      mediaServers: [],
      correlationId: 'c',
      bindingToken: 'b',
    };
  }

  disconnect(): void {
    this.disconnectCalls += 1;
  }

  /** The meeting-token host hint, as the real facade reports it. Mutable. */
  isHost = false;
  /** Current KEK generation, as the real facade reports it. Mutable. */
  currentKekGeneration: number | undefined;
  /** `setServerMute` calls, in order. */
  readonly serverMuteCalls: Array<{
    participantId: string;
    kind: 'audio' | 'video';
    muted: boolean;
  }> = [];
  /** `requestUnmute` calls. */
  unmuteRequestCalls = 0;

  async setServerMute(
    participantId: string,
    kind: 'audio' | 'video',
    muted: boolean,
  ): Promise<void> {
    this.serverMuteCalls.push({ participantId, kind, muted });
  }

  async requestUnmute(): Promise<void> {
    this.unmuteRequestCalls += 1;
  }

  /** Effective publish set, N, M and per-kind caps, as the real facade reports them. Mutable. */
  receiveSlots: ReceiveSlotsDiagnostics = {
    publish: ['audio'],
    audio: { declared: 1, slotIds: [0], source: 'default', serverCap: { state: 'unknown' } },
    video: { declared: 0, slotIds: [], source: 'default', serverCap: { state: 'unknown' } },
  };
  /** Set by `startMedia()` to the microphone, as the real facade's default. */
  captureSource: CaptureSourceInfo | undefined;
  /** The video capture source (story 3), `undefined` unless a test sets it. */
  videoCaptureSource: VideoCaptureSourceInfo | undefined;
  /** The camera light (story 3). */
  cameraTrackLive = false;
  /** Story 3 C-C1: cumulative video send counts, `undefined` unless a test sets them. */
  videoSendCounts: VideoSendCounts | undefined;
  /** The video publisher as the bus samples it (`sendTotals`), present once counts are set. */
  get video():
    | {
        sendTotals(): VideoSendCounts;
        localSendStats(): ReturnType<NonNullable<LocalStatsSource['video']>['localSendStats']>;
        readonly captureTrack: MediaStreamTrack | undefined;
      }
    | undefined {
    const counts = this.videoSendCounts;
    return counts === undefined
      ? undefined
      : {
          sendTotals: () => counts,
          localSendStats: () => ({
            kind: 'video',
            packets: counts.framesSent,
            bytes: 0,
            keyframes: counts.keyframes,
            groupsAbandoned: 0,
          }),
          captureTrack: undefined,
        };
  }
  /** The sinks a view attached, by slot (story 3 C-C1: the VideoTile binding). */
  readonly sinks = new Map<number, VideoSink | undefined>();
  /** Per-slot evidence the bus samples; mutable by a test. */
  videoEvidence: ReturnType<VideoReceiveView['evidence']> = [];
  /** The video receive view; set by `startMedia()` when M > 0, as the real facade. */
  videoReceive: VideoReceiveView | undefined;
  /** `setAudioMuted` / `setVideoMuted` calls, in order (story 3: the session-level mute API). */
  readonly sessionMuteCalls: Array<{ kind: 'audio' | 'video'; muted: boolean }> = [];
  #videoMuted = false;

  setAudioMuted(muted: boolean): void {
    this.sessionMuteCalls.push({ kind: 'audio', muted });
    this.media?.setAudioMuted(muted);
  }

  setVideoMuted(muted: boolean): void {
    this.sessionMuteCalls.push({ kind: 'video', muted });
    if (this.#videoMuted === muted) return;
    this.#videoMuted = muted;
    this.fire('muteChanged', { audioMuted: this.media?.audioMuted ?? false, videoMuted: muted });
  }

  /** The running pipeline, or `undefined` before `startMedia()` — as the real facade. */
  media: FakeAudioPipeline | undefined;
  /** Device ids `startMedia` was called with, in order. */
  readonly startMediaDeviceIds: Array<string | undefined> = [];
  /** Make the next `startMedia` reject (permission denied, no device, ...). */
  failNextStartMedia = false;

  async startMedia(options: { deviceId?: string } = {}): Promise<FakeAudioPipeline> {
    if (this.failNextStartMedia) {
      this.failNextStartMedia = false;
      throw new Error('capture unavailable');
    }
    this.startMediaDeviceIds.push(options.deviceId);
    const existing = this.media;
    if (existing) return existing;
    const pipeline = new FakeAudioPipeline((snapshot) =>
      this.fire('muteChanged', { ...snapshot, videoMuted: this.#videoMuted }),
    );
    this.media = pipeline;
    if (this.receiveSlots.video.declared > 0) {
      this.videoReceive = {
        setVideoSink: (slotId, sink) => this.sinks.set(slotId, sink),
        evidence: () => this.videoEvidence,
        unfulfilledSnapshot: () => [],
        localStats: () => [],
      };
    }
    this.captureSource ??= { mode: 'microphone' };
    return pipeline;
  }

  /** Dispatch a typed event to subscribers (test-only handle on `emit`). */
  fire<K extends keyof MeetingSessionEventMap>(type: K, payload: MeetingSessionEventMap[K]): void {
    this.emit(type, payload);
  }
}
