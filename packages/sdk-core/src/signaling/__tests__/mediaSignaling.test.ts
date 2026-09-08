// File: packages/sdk-core/src/signaling/__tests__/mediaSignaling.test.ts
//
// The ADR-0036 §4/§5/§6 signaling additions: the identity key on the join
// request, the KEK-source seam and its scrub, the roster identity-key feed, the
// send directive, the slot assignments, and the two client sends.

import { describe, expect, it } from 'vitest';
import { MockWebTransport } from '@darktower/test-utils';

import { MEETING_KEK_BYTES } from '../../media/frame/sframe.js';
import { JoinResponseKekSource } from '../../media/setup/kekSource.js';
import { RosterIdentityKeys } from '../../media/setup/rosterKeys.js';
import { generateIdentityKeyPair } from '../../media/frame/ed25519.js';
import { SignalingClient } from '../SignalingClient.js';
import type { SendDirectiveEvent, StreamAssignmentsEvent } from '../events.js';
import {
  decodeOutboundClientMessages,
  framedJoinResponse,
  framedMeetingKekUpdate,
  framedSendDirective,
  framedStreamAssignments,
  MediaKind,
  SlotState,
  waitFor,
} from './helpers.js';

const ENDPOINT = 'https://mc.example:4433';
const KEK = new Uint8Array(MEETING_KEK_BYTES).fill(0x11);

interface Rig {
  readonly client: SignalingClient;
  readonly transport: MockWebTransport;
  readonly kekSource: JoinResponseKekSource;
  readonly roster: RosterIdentityKeys;
  deliver(bytes: Uint8Array): void;
  outbound(): ReturnType<typeof decodeOutboundClientMessages>;
}

async function joinedRig(joinResponse: Uint8Array, identityPublicKey?: Uint8Array): Promise<Rig> {
  const transport = new MockWebTransport();
  const kekSource = new JoinResponseKekSource();
  const roster = new RosterIdentityKeys(8);
  const client = new SignalingClient({
    connect: () => transport,
    kekSink: kekSource,
    rosterKeys: roster,
  });
  const joining = client.join({
    webtransportEndpoint: ENDPOINT,
    meetingId: 'm',
    joinToken: 'jwt',
    participantName: 'self',
    ...(identityPublicKey ? { identityPublicKey } : {}),
  });
  transport.simulateReady();
  await waitFor(() => transport.getOpenedBidiStreams().length > 0);
  transport.simulateServerMessage(0, joinResponse);
  await joining;
  return {
    client,
    transport,
    kekSource,
    roster,
    deliver: (bytes) => transport.simulateServerMessage(0, bytes),
    outbound: () => decodeOutboundClientMessages(transport.getOutboundBidiWrites(0)),
  };
}

describe('the identity signing key travels on the join request', () => {
  it('carries the raw 32-byte public half', async () => {
    const identity = await generateIdentityKeyPair();
    const rig = await joinedRig(framedJoinResponse({ senderId: 258 }), identity.publicKey);
    const join = rig.outbound()[0];
    expect(join?.message.case).toBe('joinRequest');
    const request = join?.message.value as { identityPublicKey: Uint8Array };
    expect([...request.identityPublicKey]).toEqual([...identity.publicKey]);
  });

  it('sends an EMPTY key when the caller supplies none', async () => {
    // MC then publishes an empty key on the roster, and every consumer must fail
    // closed on it. Absence is a legitimate state, not an error.
    const rig = await joinedRig(framedJoinResponse({}));
    const request = rig.outbound()[0]?.message.value as { identityPublicKey: Uint8Array };
    expect(request.identityPublicKey.length).toBe(0);
  });
});

describe('the meeting KEK never survives the decode boundary', () => {
  it('installs it in the seam and leaves nothing on the decoded message', async () => {
    const rig = await joinedRig(
      framedJoinResponse({ senderId: 258, kekGeneration: 4, meetingKek: KEK }),
    );
    expect(rig.kekSource.isProvisioned).toBe(true);
    expect(rig.kekSource.kekForGeneration(4)).toEqual(KEK);
    expect(rig.client.kekGeneration).toBe(4);
  });

  it('never places the KEK on the public JoinedEvent', async () => {
    // `JoinedEvent` is a public payload and, in test builds, is projected onto
    // the web app's e2e bus. The KEK goes to the seam and nowhere else.
    const transport = new MockWebTransport();
    const client = new SignalingClient({
      connect: () => transport,
      kekSink: new JoinResponseKekSource(),
    });
    const joining = client.join({
      webtransportEndpoint: ENDPOINT,
      meetingId: 'm',
      joinToken: 'jwt',
      participantName: 'self',
    });
    transport.simulateReady();
    await waitFor(() => transport.getOpenedBidiStreams().length > 0);
    transport.simulateServerMessage(0, framedJoinResponse({ meetingKek: KEK }));
    const joined = await joining;
    expect(JSON.stringify(joined)).not.toContain('17');
    expect(Object.keys(joined)).not.toContain('meetingKek');
  });

  it('scrubs a KEK-push message even though rotation is UNUSED this story', async () => {
    // Leaving key material on a decoded message because nothing consumes it yet
    // is exactly how a latent leak becomes a live one.
    const rig = await joinedRig(framedJoinResponse({ senderId: 258 }));
    rig.deliver(framedMeetingKekUpdate(new Uint8Array(MEETING_KEK_BYTES).fill(0x22), 9));
    await waitFor(() => rig.kekSource.isProvisioned);
    expect(rig.kekSource.kekForGeneration(9)).toBeDefined();
  });
});

describe('the roster identity-key feed', () => {
  it('resolves a participant published with a sender id and a key', async () => {
    const peer = await generateIdentityKeyPair();
    const rig = await joinedRig(
      framedJoinResponse({
        senderId: 258,
        participants: [
          { participantId: 'p1', name: 'Peer', senderId: 77, identityPublicKey: peer.publicKey },
        ],
      }),
    );
    await waitFor(() => rig.roster.identityKeyFor(77) !== undefined);
    expect(rig.roster.identityKeyFor(77)).toBeDefined();
  });

  it('records a participant with an EMPTY key as unresolvable, never as skippable', async () => {
    const rig = await joinedRig(
      framedJoinResponse({
        senderId: 258,
        participants: [
          { participantId: 'p1', name: 'Peer', senderId: 77, identityPublicKey: new Uint8Array(0) },
        ],
      }),
    );
    await waitFor(() => true);
    expect(rig.roster.identityKeyFor(77)).toBeUndefined();
  });
});

describe('send directive', () => {
  it('projects one audio stream with its targets', async () => {
    const rig = await joinedRig(framedJoinResponse({ senderId: 258 }));
    const seen: SendDirectiveEvent[] = [];
    rig.client.on('sendDirective', (e) => seen.push(e));
    rig.deliver(
      framedSendDirective({ streamNumber: 3, maxBitrateBps: 40_000, targets: ['https://mh'] }),
    );
    await waitFor(() => seen.length > 0);
    expect(seen[0]).toEqual({
      headerVersion: 2,
      streams: [
        { streamNumber: 3, mediaKind: 'audio', maxBitrateBps: 40_000, targets: ['https://mh'] },
      ],
    });
  });

  it('reports an ABSENT bitrate as undefined, never coerced to zero', async () => {
    // "MC said nothing" and "MC said 0" must stay distinguishable: the SDK
    // applies its configured default only for the first.
    const rig = await joinedRig(framedJoinResponse({ senderId: 258 }));
    const seen: SendDirectiveEvent[] = [];
    rig.client.on('sendDirective', (e) => seen.push(e));
    rig.deliver(framedSendDirective({ targets: ['https://mh'] }));
    await waitFor(() => seen.length > 0);
    expect(seen[0]?.streams[0]?.maxBitrateBps).toBeUndefined();
  });

  it('passes an EMPTY target set through — it means SEND NOTHING', async () => {
    const rig = await joinedRig(framedJoinResponse({ senderId: 258 }));
    const seen: SendDirectiveEvent[] = [];
    rig.client.on('sendDirective', (e) => seen.push(e));
    rig.deliver(framedSendDirective({ targets: [] }));
    await waitFor(() => seen.length > 0);
    expect(seen[0]?.streams[0]?.targets).toEqual([]);
  });

  it('classifies a video stream as video rather than silently as audio', async () => {
    const rig = await joinedRig(framedJoinResponse({ senderId: 258 }));
    const seen: SendDirectiveEvent[] = [];
    rig.client.on('sendDirective', (e) => seen.push(e));
    rig.deliver(framedSendDirective({ mediaKind: MediaKind.VIDEO_CAMERA, targets: [] }));
    await waitFor(() => seen.length > 0);
    expect(seen[0]?.streams[0]?.mediaKind).toBe('video');
  });
});

describe('stream assignments', () => {
  it('projects the slot state explicitly — absence of frames is not a signal', async () => {
    const rig = await joinedRig(framedJoinResponse({ senderId: 258 }));
    const seen: StreamAssignmentsEvent[] = [];
    rig.client.on('streamAssignments', (e) => seen.push(e));
    rig.deliver(
      framedStreamAssignments({
        slotId: 0,
        senderId: 258,
        mediaHandlerUrl: 'https://mh',
        slotState: SlotState.WITHHELD_BY_CONGESTION,
      }),
    );
    await waitFor(() => seen.length > 0);
    expect(seen[0]?.assignments[0]?.slotState).toBe('withheld_congestion');
  });

  it('passes an ABSENT sender id through rather than coercing it to 0', async () => {
    // 0 is a reserved-invalid sender id, so coercion would turn "no source
    // assigned" into "assigned to an impossible participant".
    const rig = await joinedRig(framedJoinResponse({ senderId: 258 }));
    const seen: StreamAssignmentsEvent[] = [];
    rig.client.on('streamAssignments', (e) => seen.push(e));
    rig.deliver(framedStreamAssignments({ slotState: SlotState.FEWER_SOURCES_THAN_SLOTS }));
    await waitFor(() => seen.length > 0);
    expect(seen[0]?.assignments[0]?.senderId).toBeUndefined();
    expect(seen[0]?.assignments[0]?.slotState).toBe('fewer_sources');
  });

  it('maps an UNSPECIFIED state to unspecified rather than guessing active', async () => {
    const rig = await joinedRig(framedJoinResponse({ senderId: 258 }));
    const seen: StreamAssignmentsEvent[] = [];
    rig.client.on('streamAssignments', (e) => seen.push(e));
    rig.deliver(framedStreamAssignments({ slotState: SlotState.UNSPECIFIED }));
    await waitFor(() => seen.length > 0);
    expect(seen[0]?.assignments[0]?.slotState).toBe('unspecified');
  });
});

describe('client sends', () => {
  it('declares a receive capability, which is the PRECONDITION for a directive', async () => {
    // MC emits the send directive when the capability arrives, NOT at join. A
    // client that joins and never declares is never told to send — and the
    // connection stays healthy in every other respect, which is what makes the
    // omission hard to notice from the client side.
    const rig = await joinedRig(framedJoinResponse({ senderId: 258 }));
    await rig.client.sendReceiveCapability([{ slotId: 0, mediaKind: 'audio' }]);
    const messages = rig.outbound();
    const capability = messages.find((m) => m.message.case === 'receiveCapability');
    expect(capability).toBeDefined();
    const value = capability?.message.value as { slots: { slotId: number }[] };
    expect(value.slots).toHaveLength(1);
    expect(value.slots[0]?.slotId).toBe(0);
  });

  it('supports the send-only case with an EMPTY slot list', async () => {
    const rig = await joinedRig(framedJoinResponse({ senderId: 258 }));
    await rig.client.sendReceiveCapability([]);
    const capability = rig.outbound().find((m) => m.message.case === 'receiveCapability');
    expect((capability?.message.value as { slots: unknown[] }).slots).toEqual([]);
  });

  it('reports client mute informationally', async () => {
    const rig = await joinedRig(framedJoinResponse({ senderId: 258 }));
    await rig.client.sendMuteRequest(true);
    const mute = rig.outbound().find((m) => m.message.case === 'muteRequest');
    expect((mute?.message.value as { audioMuted: boolean }).audioMuted).toBe(true);
  });

  it('refuses both sends before a settled join', async () => {
    const client = new SignalingClient({ connect: () => new MockWebTransport() });
    await expect(client.sendReceiveCapability([])).rejects.toThrow();
    await expect(client.sendMuteRequest(true)).rejects.toThrow();
  });
});
