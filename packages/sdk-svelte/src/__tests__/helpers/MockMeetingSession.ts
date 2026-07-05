// A lightweight `MeetingSession` test double for the component tier (R-43).
//
// It reuses sdk-core's exported `TypedEventEmitter` (no re-implemented emitter)
// and exposes a public `fire` that calls the protected `emit`, so tests drive
// REAL typed events through the same dispatch path the real facade uses. It
// structurally satisfies `BoundMeetingSession` (a `state` field + `on`), so the
// adapter binds to it without constructing the heavy real `MeetingSession`.

import { MeetingSessionState, TypedEventEmitter } from '@darktower/sdk-core';
import type { MeetingSessionEventMap } from '@darktower/sdk-core';
import type { BoundMeetingSession } from '../../stores/bindMeetingSession.js';

export class MockMeetingSession
  extends TypedEventEmitter<MeetingSessionEventMap>
  implements BoundMeetingSession
{
  state: MeetingSessionState = MeetingSessionState.Idle;

  /** Dispatch a typed event to subscribers (test-only handle on `emit`). */
  fire<K extends keyof MeetingSessionEventMap>(type: K, payload: MeetingSessionEventMap[K]): void {
    this.emit(type, payload);
  }
}
