// A `MeetingSession` test double for the demo view tests (R-43). Reuses
// sdk-core's exported `TypedEventEmitter`; exposes `fire` (over the protected
// `emit`) plus the `join`/`disconnect`/`state` surface the join view drives.
// Local to web-app tests (mock-home adjudicated with @test/@dry at review).

import { MeetingSessionState, TypedEventEmitter } from '@darktower/sdk-core';
import type { JoinedEvent, MeetingSessionEventMap } from '@darktower/sdk-core';

export class MockMeetingSession extends TypedEventEmitter<MeetingSessionEventMap> {
  state: MeetingSessionState = MeetingSessionState.Idle;
  joinCalls = 0;
  disconnectCalls = 0;

  async join(): Promise<JoinedEvent> {
    this.joinCalls += 1;
    return {
      participantId: 'self',
      userId: 1n,
      existingParticipants: [],
      mediaServers: [],
      correlationId: 'c',
      bindingToken: 'b',
    };
  }

  disconnect(): void {
    this.disconnectCalls += 1;
  }

  /** Dispatch a typed event to subscribers (test-only handle on `emit`). */
  fire<K extends keyof MeetingSessionEventMap>(type: K, payload: MeetingSessionEventMap[K]): void {
    this.emit(type, payload);
  }
}
