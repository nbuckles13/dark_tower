// TRANSITIONAL (C-A1 shim b) — deleted by C-A2 with the shim.
//
// A legacy SOURCE_MUTED never maps to `active`: server-muted for the slot's kind
// wins; anything else (no update, unknown sender, unknown kind) is the
// fail-closed `avatar_client_muted` default. Covered pure, and end to end
// through `SignalingClient` including a mute update arriving AFTER the slot.

import { describe, expect, it } from 'vitest';
import { MockWebTransport } from '@darktower/test-utils';

import { SlotState } from '../../proto/dark_tower/signaling/v1/signaling_pb.js';
import { resolveLegacySourceMuted } from '../legacySourceMutedShim.js';
import { SignalingClient } from '../SignalingClient.js';
import type { StreamAssignmentsEvent } from '../events.js';
import {
  MediaKind,
  framedJoinResponse,
  framedParticipantMuteUpdate,
  framedStreamAssignments,
  waitFor,
} from './helpers.js';

const legacy = (senderId: number | undefined, mediaKind: 'audio' | 'video' | 'unknown') => ({
  slotState: SlotState.SOURCE_MUTED,
  senderId,
  mediaKind,
});

describe('resolveLegacySourceMuted (pure)', () => {
  it.each([
    [
      'audio slot, sender server-muted for audio',
      legacy(7, 'audio'),
      { audio: true, video: false },
      'avatar_server_muted',
    ],
    [
      'audio slot, sender server-muted for video only',
      legacy(7, 'audio'),
      { audio: false, video: true },
      'avatar_client_muted',
    ],
    [
      'video slot, sender server-muted for video',
      legacy(7, 'video'),
      { audio: false, video: true },
      'avatar_server_muted',
    ],
    [
      'video slot, both server-muted (server wins)',
      legacy(7, 'video'),
      { audio: true, video: true },
      'avatar_server_muted',
    ],
    [
      'no mute update yet (fail-closed default)',
      legacy(7, 'audio'),
      undefined,
      'avatar_client_muted',
    ],
    [
      'no sender id on the slot',
      legacy(undefined, 'audio'),
      { audio: true, video: true },
      'avatar_client_muted',
    ],
    [
      'an unknown slot kind',
      legacy(7, 'unknown'),
      { audio: true, video: true },
      'avatar_client_muted',
    ],
  ] as const)('%s -> %s', (_label, assignment, flags, expected) => {
    expect(resolveLegacySourceMuted(assignment, flags)).toBe(expected);
    expect(resolveLegacySourceMuted(assignment, flags)).not.toBe('active');
  });
});

describe('shim (b) through SignalingClient', () => {
  async function joined(): Promise<{
    deliver: (bytes: Uint8Array) => void;
    seen: StreamAssignmentsEvent[];
  }> {
    const transport = new MockWebTransport();
    const client = new SignalingClient({ connect: () => transport });
    const joining = client.join({
      webtransportEndpoint: 'https://mc',
      meetingId: 'm',
      joinToken: 'jwt',
      participantName: 'self',
      publishKinds: ['audio'],
      receiveSlots: [{ slotId: 0, mediaKind: 'audio' }],
    });
    transport.simulateReady();
    await waitFor(() => transport.getOpenedBidiStreams().length > 0);
    transport.simulateServerMessage(
      0,
      framedJoinResponse({
        senderId: 1,
        participants: [{ participantId: 'peer', name: 'Peer', senderId: 7 }],
      }),
    );
    await joining;
    const seen: StreamAssignmentsEvent[] = [];
    client.on('streamAssignments', (e) => seen.push(e));
    return { deliver: (b) => transport.simulateServerMessage(0, b), seen };
  }

  it('maps legacy SOURCE_MUTED to the client-muted default with no update', async () => {
    const rig = await joined();
    rig.deliver(
      framedStreamAssignments({ slotId: 0, senderId: 7, slotState: SlotState.SOURCE_MUTED }),
    );
    await waitFor(() => rig.seen.length === 1);
    expect(rig.seen[0]?.assignments[0]?.slotState).toBe('avatar_client_muted');
  });

  it('server wins when the update arrived first', async () => {
    const rig = await joined();
    rig.deliver(framedParticipantMuteUpdate({ participantId: 'peer', audioServerMuted: true }));
    rig.deliver(
      framedStreamAssignments({ slotId: 0, senderId: 7, slotState: SlotState.SOURCE_MUTED }),
    );
    await waitFor(() => rig.seen.length === 1);
    expect(rig.seen[0]?.assignments[0]?.slotState).toBe('avatar_server_muted');
  });

  it('a LATE server-mute update re-derives and re-emits the slot as server-muted', async () => {
    const rig = await joined();
    rig.deliver(
      framedStreamAssignments({ slotId: 0, senderId: 7, slotState: SlotState.SOURCE_MUTED }),
    );
    await waitFor(() => rig.seen.length === 1);
    rig.deliver(framedParticipantMuteUpdate({ participantId: 'peer', audioServerMuted: true }));
    await waitFor(() => rig.seen.length === 2);
    expect(rig.seen[1]?.assignments[0]?.slotState).toBe('avatar_server_muted');
  });

  it('a mute update for a sender holding NO legacy slot does not re-emit', async () => {
    const rig = await joined();
    rig.deliver(framedStreamAssignments({ slotId: 0, senderId: 7, slotState: SlotState.ACTIVE }));
    await waitFor(() => rig.seen.length === 1);
    rig.deliver(framedParticipantMuteUpdate({ participantId: 'peer', audioServerMuted: true }));
    // Drain point: a later ordered message.
    rig.deliver(framedStreamAssignments({ slotId: 0, senderId: 7, slotState: SlotState.ACTIVE }));
    await waitFor(() => rig.seen.length === 2);
    expect(rig.seen).toHaveLength(2);
  });

  it("uses the SLOT's kind: a video-only server mute leaves an audio slot client-muted", async () => {
    const rig = await joined();
    rig.deliver(
      framedParticipantMuteUpdate({
        participantId: 'peer',
        audioServerMuted: false,
        videoServerMuted: true,
      }),
    );
    rig.deliver(
      framedStreamAssignments({
        slotId: 0,
        senderId: 7,
        slotState: SlotState.SOURCE_MUTED,
        extraSlots: [
          {
            slotId: 1,
            senderId: 7,
            mediaHandlerUrl: '',
            mediaKind: MediaKind.VIDEO_CAMERA,
            slotState: SlotState.SOURCE_MUTED,
          },
        ],
      }),
    );
    await waitFor(() => rig.seen.length === 1);
    expect(rig.seen[0]?.assignments.map((a) => a.slotState)).toEqual([
      'avatar_client_muted',
      'avatar_server_muted',
    ]);
  });
});
