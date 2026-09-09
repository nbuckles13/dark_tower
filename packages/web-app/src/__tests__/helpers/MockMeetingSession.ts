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
  JoinedEvent,
  MediaFrameCounts,
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
  /** Make the next `setCaptureDevice` reject, as an unavailable device would. */
  failNextDeviceChange = false;

  #audioMuted = false;
  readonly #echo: (snapshot: MuteSnapshot) => void;

  constructor(echo: (snapshot: MuteSnapshot) => void) {
    this.#echo = echo;
  }

  setAudioMuted(muted: boolean): void {
    this.muteCalls.push(muted);
    if (this.#audioMuted === muted) return;
    this.#audioMuted = muted;
    this.#echo({ audioMuted: muted });
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

  async join(): Promise<JoinedEvent> {
    this.joinCalls += 1;
    return {
      participantId: 'self',
      senderId: 1,
      existingParticipants: [],
      mediaServers: [],
      correlationId: 'c',
      bindingToken: 'b',
    };
  }

  disconnect(): void {
    this.disconnectCalls += 1;
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
    const pipeline = new FakeAudioPipeline((snapshot) => this.fire('muteChanged', snapshot));
    this.media = pipeline;
    return pipeline;
  }

  /** Dispatch a typed event to subscribers (test-only handle on `emit`). */
  fire<K extends keyof MeetingSessionEventMap>(type: K, payload: MeetingSessionEventMap[K]): void {
    this.emit(type, payload);
  }
}
