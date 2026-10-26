// File: packages/sdk-core/src/signaling/__tests__/mediaSignaling.test.ts
//
// The ADR-0036 §4/§5/§6 signaling additions: the identity key on the join
// request, the KEK-source seam and its scrub, the roster identity-key feed, the
// send directive, the slot assignments, and the two client sends.

import { describe, expect, it } from 'vitest';
import { MockWebTransport } from '@darktower/test-utils';

import { MEETING_KEK_BYTES } from '../../media/frame/sframe.js';
import { emptyKekHolder } from '../../media/__tests__/helpers.js';
import type { MeetingKekHolder } from '../../media/setup/kekSource.js';
import { RosterIdentityKeys } from '../../media/setup/rosterKeys.js';
import { generateIdentityKeyPair } from '../../media/frame/ed25519.js';
import { HEADER_VERSION } from '../../media/frame/wireConstants.js';
import { SignalingClient } from '../SignalingClient.js';
import type {
  JoinedEvent,
  ParticipantJoinedEvent,
  ParticipantMuteEvent,
  PublishedKindDeclaration,
  SendDirectiveEvent,
  StreamAssignmentsEvent,
  UnmuteRequestedEvent,
} from '../events.js';
import {
  decodeOutboundClientMessages,
  framedJoinResponse,
  framedMeetingKekUpdate,
  framedParticipantJoined,
  framedParticipantLeft,
  framedSendDirective,
  LeaveReason,
  framedStreamAssignments,
  framedParticipantMuteUpdate,
  framedUnmuteRequestRelay,
  MediaKind,
  SlotState,
  ClientMediaState,
  LIVE_STATES,
  waitFor,
} from './helpers.js';

const ENDPOINT = 'https://mc.example:4433';
const KEK = new Uint8Array(MEETING_KEK_BYTES).fill(0x11);

interface Rig {
  readonly client: SignalingClient;
  readonly transport: MockWebTransport;
  readonly kekSource: MeetingKekHolder;
  readonly roster: RosterIdentityKeys;
  readonly joined: JoinedEvent;
  deliver(bytes: Uint8Array): void;
  outbound(): ReturnType<typeof decodeOutboundClientMessages>;
}

async function joinedRig(joinResponse: Uint8Array, identityPublicKey?: Uint8Array): Promise<Rig> {
  const transport = new MockWebTransport();
  const kekSource = emptyKekHolder();
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
    publish: [{ kind: 'audio', clientMuted: false }],
    receiveSlots: [{ slotId: 0, mediaKind: 'audio' }],
    ...(identityPublicKey ? { identityPublicKey } : {}),
  });
  transport.simulateReady();
  await waitFor(() => transport.getOpenedBidiStreams().length > 0);
  transport.simulateServerMessage(0, joinResponse);
  const joined = await joining;
  return {
    client,
    transport,
    kekSource,
    roster,
    joined,
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
      kekSink: emptyKekHolder(),
    });
    const joining = client.join({
      webtransportEndpoint: ENDPOINT,
      meetingId: 'm',
      joinToken: 'jwt',
      participantName: 'self',
      publish: [{ kind: 'audio', clientMuted: false }],
      receiveSlots: [],
    });
    transport.simulateReady();
    await waitFor(() => transport.getOpenedBidiStreams().length > 0);
    transport.simulateServerMessage(0, framedJoinResponse({ meetingKek: KEK }));
    const joined = await joining;
    expect(JSON.stringify(joined)).not.toContain('17');
    expect(Object.keys(joined)).not.toContain('meetingKek');
  });

  it('routes a KEK-push message through the holder, as the push source', async () => {
    // A rotation push lands in the SAME holder the join
    // response fed, through the same scrubbing intake.
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
      headerVersion: HEADER_VERSION,
      streams: [
        {
          streamNumber: 3,
          mediaKind: 'audio',
          maxBitrateBps: 40_000,
          encoding: {
            codec: null,
            width: 0,
            height: 0,
            frameRate: 0,
            maxBitrateBps: 40_000,
            minKeyframeIntervalMs: undefined,
          },
          targets: ['https://mh'],
        },
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
    expect(seen[0]?.assignments[0]?.slotState).toBe('withheld_by_congestion');
  });

  it('carries unreachable_sender_ids through, and an EMPTY set as empty', async () => {
    // ADR-0036 §9 / `signaling.proto`: the per-subscriber set of roster
    // participants this client shares no connected handler with. It was received
    // and DROPPED here until this task — the field was mapped nowhere, so the
    // obligation to mark those roster entries distinctly had no data to act on.
    //
    // A message may legitimately carry zero assignments and a non-empty set (every
    // other participant is elsewhere), so the two are asserted independently.
    const rig = await joinedRig(framedJoinResponse({ senderId: 258 }));
    const seen: StreamAssignmentsEvent[] = [];
    rig.client.on('streamAssignments', (e) => seen.push(e));

    rig.deliver(
      framedStreamAssignments({
        slotId: 0,
        senderId: 258,
        mediaHandlerUrl: 'https://mh-0',
        unreachableSenderIds: [259, 260],
      }),
    );
    await waitFor(() => seen.length > 0);
    expect(seen[0]?.unreachableSenderIds).toEqual([259, 260]);

    // Absent on the wire is EMPTY, not undefined: a `repeated` field has no
    // "unset", so a client must not be able to tell them apart.
    rig.deliver(framedStreamAssignments({ slotId: 0, senderId: 258 }));
    await waitFor(() => seen.length > 1);
    expect(seen[1]?.unreachableSenderIds).toEqual([]);
  });

  it('reads each slot on the handler that OWNS it, across several handlers', async () => {
    // The canonical case seen from A: B's edge on one handler, C's on the other.
    // Nothing here depends on which handler MC chose for which edge — only that
    // each slot's own `media_handler_url` is carried through verbatim.
    const rig = await joinedRig(framedJoinResponse({ senderId: 258 }));
    const seen: StreamAssignmentsEvent[] = [];
    rig.client.on('streamAssignments', (e) => seen.push(e));
    rig.deliver(
      framedStreamAssignments({
        slotId: 0,
        senderId: 259,
        mediaHandlerUrl: 'https://mh-0:4433',
        extraSlots: [{ slotId: 1, senderId: 260, mediaHandlerUrl: 'https://mh-1:4433' }],
      }),
    );
    await waitFor(() => seen.length > 0);
    expect(seen[0]?.assignments.map((a) => a.mediaHandlerUrl)).toEqual([
      'https://mh-0:4433',
      'https://mh-1:4433',
    ]);
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
    expect(seen[0]?.assignments[0]?.slotState).toBe('fewer_sources_than_slots');
  });

  it('maps an UNSPECIFIED state to unspecified rather than guessing active', async () => {
    const rig = await joinedRig(framedJoinResponse({ senderId: 258 }));
    const seen: StreamAssignmentsEvent[] = [];
    rig.client.on('streamAssignments', (e) => seen.push(e));
    rig.deliver(framedStreamAssignments({ slotState: SlotState.UNSPECIFIED }));
    await waitFor(() => seen.length > 0);
    expect(seen[0]?.assignments[0]?.slotState).toBe('unspecified');
  });

  // The reserved former ZERO_REQUESTED number, and any number the SDK does not
  // name, fail closed.
  it.each([
    ['reserved 5 (former ZERO_REQUESTED)', 5 as SlotState],
    ['an unknown future value', 99 as SlotState],
  ])('maps %s to unspecified, never active', async (_label, state) => {
    const rig = await joinedRig(framedJoinResponse({ senderId: 258 }));
    const seen: StreamAssignmentsEvent[] = [];
    rig.client.on('streamAssignments', (e) => seen.push(e));
    rig.deliver(framedStreamAssignments({ slotState: state }));
    await waitFor(() => seen.length > 0);
    expect(seen[0]?.assignments[0]?.slotState).toBe('unspecified');
  });

  it('carries the slot kind; a live VIDEO slot names no sender', async () => {
    const rig = await joinedRig(framedJoinResponse({ senderId: 258 }));
    const seen: StreamAssignmentsEvent[] = [];
    rig.client.on('streamAssignments', (e) => seen.push(e));
    rig.deliver(
      framedStreamAssignments({
        slotId: 0,
        senderId: 259,
        mediaHandlerUrl: 'https://mh',
        extraSlots: [
          { slotId: 1, mediaHandlerUrl: 'https://mh', mediaKind: MediaKind.VIDEO_CAMERA },
          {
            slotId: 2,
            mediaHandlerUrl: '',
            mediaKind: MediaKind.VIDEO_SCREEN,
            slotState: SlotState.ACTIVE,
          },
        ],
      }),
    );
    await waitFor(() => seen.length > 0);
    expect(seen[0]?.assignments.map((a) => [a.mediaKind, a.senderId, a.slotState])).toEqual([
      ['audio', 259, 'active'],
      ['video', undefined, 'active'],
      ['unknown', undefined, 'active'],
    ]);
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

  it('reports client mute as ONE MuteChange for the changed kind', async () => {
    const rig = await joinedRig(framedJoinResponse({ senderId: 258 }));
    await rig.client.sendMuteRequest('audio', true);
    const mute = rig.outbound().find((m) => m.message.case === 'muteRequest');
    if (mute?.message.case !== 'muteRequest') throw new Error('no MuteRequest sent');
    expect(mute.message.value.changes.map((c) => [c.mediaKind, c.muted])).toEqual([
      [MediaKind.AUDIO, true],
    ]);
  });

  it('a server-mute request carries TARGET AND ACTION ONLY — no requester, no reason', async () => {
    // Story 2 R-8 / @security M1: MC identifies the requester from the
    // authenticated connection; the message has no field for it and none is sent.
    const rig = await joinedRig(framedJoinResponse({ senderId: 258 }));
    await rig.client.sendServerMuteRequest('participant-b', 'audio', true);
    await rig.client.sendServerMuteRequest('participant-b', 'audio', false);
    const sent = rig
      .outbound()
      .flatMap((m) => (m.message.case === 'serverMuteRequest' ? [m.message.value] : []));
    expect(
      sent.map((r) => [r.participantId, r.reason, r.changes.map((c) => [c.mediaKind, c.muted])]),
    ).toEqual([
      ['participant-b', '', [[MediaKind.AUDIO, true]]],
      ['participant-b', '', [[MediaKind.AUDIO, false]]],
    ]);
  });

  it.each([
    [['audio'], [MediaKind.AUDIO]],
    [['video'], [MediaKind.VIDEO_CAMERA]],
    [
      ['audio', 'video'],
      [MediaKind.AUDIO, MediaKind.VIDEO_CAMERA],
    ],
  ] as const)(
    'an unmute request names exactly the kinds %j and nobody (MC stamps the requester)',
    async (kinds, onWire) => {
      const rig = await joinedRig(framedJoinResponse({ senderId: 258 }));
      await rig.client.sendUnmuteRequest(kinds);
      const request = rig.outbound().find((m) => m.message.case === 'unmuteRequest');
      expect(request?.message.value).toMatchObject({ mediaKinds: onWire, participantId: '' });
    },
  );

  it('refuses an EMPTY or duplicated unmute request locally, sending nothing', async () => {
    const rig = await joinedRig(framedJoinResponse({ senderId: 258 }));
    const before = rig.outbound().length;
    await expect(rig.client.sendUnmuteRequest([])).rejects.toMatchObject({
      signalingCode: 'INVALID_REQUEST',
    });
    await expect(rig.client.sendUnmuteRequest(['audio', 'audio'])).rejects.toMatchObject({
      signalingCode: 'INVALID_REQUEST',
    });
    expect(rig.outbound()).toHaveLength(before);
  });

  it('refuses every send before a settled join', async () => {
    const client = new SignalingClient({ connect: () => new MockWebTransport() });
    await expect(client.sendReceiveCapability([])).rejects.toThrow();
    await expect(client.sendMuteRequest('audio', true)).rejects.toThrow();
    await expect(client.sendServerMuteRequest('p', 'audio', true)).rejects.toThrow();
    await expect(client.sendUnmuteRequest(['audio'])).rejects.toThrow();
  });
});

describe("a leave removes the leaver's roster key — a second cutoff beside the slot gate", () => {
  it("forgets the leaver's identity key, resolved from the roster the join carried", async () => {
    // `ParticipantLeft` names only the participant; the sender id comes from the
    // roster this client already saw. Once forgotten, the leaver's frames drop at
    // `no_roster_entry` BEFORE verify, independently of the slot-edge gate.
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

    const forgotten: number[] = [];
    rig.roster.setTransmitKeyInvalidationListener((id, cause) => {
      if (cause === 'forgotten') forgotten.push(id);
    });
    rig.deliver(framedParticipantLeft('p1', LeaveReason.VOLUNTARY));
    await waitFor(() => rig.roster.identityKeyFor(77) === undefined);
    // Its transmit keys are invalidated with it (the pipeline purges on this).
    expect(forgotten).toEqual([77]);
  });

  it('ignores a leave for a participant it never saw, and leaves others untouched', async () => {
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
    const seen: unknown[] = [];
    rig.client.on('streamAssignments', (e) => seen.push(e));
    rig.deliver(framedParticipantLeft('stranger', LeaveReason.VOLUNTARY));
    // DRAIN POINT: a later message on the same ordered stream. Once it has been
    // dispatched, the leave before it has been handled.
    rig.deliver(framedStreamAssignments({ slotId: 0, senderId: 77 }));
    await waitFor(() => seen.length === 1);
    expect(rig.roster.identityKeyFor(77)).toBeDefined();
  });
});

describe('per-kind state of others from the wire (story 3 R-15; CONVENTIONS §6)', () => {
  // Through the real encode -> frame -> decode path. The contract table
  // (missing / malformed / unknown kind) is `mediaKindStates.test.ts`.
  it('roster entries carry every kind state: not publishing, live, client-muted', async () => {
    const rig = await joinedRig(
      framedJoinResponse({
        senderId: 258,
        participants: [
          {
            participantId: 'm',
            name: 'Muted',
            mediaStates: {
              audio: { clientState: ClientMediaState.CLIENT_MUTED },
              video: { clientState: ClientMediaState.NOT_PUBLISHED },
            },
          },
          { participantId: 'f', name: 'Fresh' },
        ],
      }),
    );
    expect(rig.joined.existingParticipants).toStrictEqual([
      {
        participantId: 'm',
        name: 'Muted',
        senderId: undefined,
        mediaStates: {
          audio: { clientState: 'client_muted', serverMuted: false },
          video: { clientState: 'not_published', serverMuted: false },
        },
      },
      { participantId: 'f', name: 'Fresh', senderId: undefined, mediaStates: LIVE_STATES },
    ]);
  });

  it('a ParticipantJoined of a participant who joined muted is muted at once — no report needed', async () => {
    const rig = await joinedRig(framedJoinResponse({ senderId: 258 }));
    const seen: ParticipantJoinedEvent[] = [];
    rig.client.on('participantJoined', (e) => seen.push(e));
    rig.deliver(
      framedParticipantJoined('g', 'Gil', {
        audio: { clientState: ClientMediaState.CLIENT_MUTED },
      }),
    );
    await waitFor(() => seen.length > 0);
    expect(seen[0]?.participant.mediaStates.audio.clientState).toBe('client_muted');
    expect(seen[0]?.participant.mediaStates.video.clientState).toBe('live');
  });

  it('ParticipantMuteUpdate is a full per-kind snapshot: client and server independent', async () => {
    const rig = await joinedRig(framedJoinResponse({ senderId: 258 }));
    const seen: ParticipantMuteEvent[] = [];
    rig.client.on('participantMuteChanged', (e) => seen.push(e));
    rig.deliver(
      framedParticipantMuteUpdate({
        participantId: 'b',
        states: {
          audio: {
            clientState: ClientMediaState.CLIENT_MUTED,
            serverMuted: true,
            serverMutedBy: 'host',
          },
          video: { clientState: ClientMediaState.LIVE },
        },
      }),
    );
    await waitFor(() => seen.length > 0);
    expect(seen[0]).toStrictEqual({
      participantId: 'b',
      mediaStates: {
        audio: { clientState: 'client_muted', serverMuted: true, serverMutedBy: 'host' },
        video: { clientState: 'live', serverMuted: false },
      },
    });
  });
});

describe('server mute state from the wire (story 2 R-10/R-11)', () => {
  it('projects SERVER mute and who applied it, PER KIND', async () => {
    const rig = await joinedRig(framedJoinResponse({ senderId: 258 }));
    const seen: ParticipantMuteEvent[] = [];
    rig.client.on('participantMuteChanged', (e) => seen.push(e));
    rig.deliver(
      framedParticipantMuteUpdate({
        participantId: 'b',
        states: {
          audio: { serverMuted: true, serverMutedBy: 'host-1' },
          video: { serverMuted: true, serverMutedBy: 'host-2' },
        },
      }),
    );
    // Unmuted: no `serverMutedBy`, even if MC were to send a stale one.
    rig.deliver(
      framedParticipantMuteUpdate({
        participantId: 'b',
        states: { audio: { serverMuted: false, serverMutedBy: 'stale' } },
      }),
    );
    await waitFor(() => seen.length > 1);
    expect(seen[0]?.mediaStates).toEqual({
      audio: { clientState: 'live', serverMuted: true, serverMutedBy: 'host-1' },
      video: { clientState: 'live', serverMuted: true, serverMutedBy: 'host-2' },
    });
    expect(seen[1]?.mediaStates).toEqual(LIVE_STATES);
    expect('serverMutedBy' in seen[1]!.mediaStates.audio).toBe(false);
  });

  it('bounds MC-relayed ids to 256 before they leave the decode boundary', async () => {
    const rig = await joinedRig(framedJoinResponse({ senderId: 258 }));
    const seen: ParticipantMuteEvent[] = [];
    rig.client.on('participantMuteChanged', (e) => seen.push(e));
    rig.deliver(
      framedParticipantMuteUpdate({
        participantId: 'p'.repeat(300),
        states: { audio: { serverMuted: true, serverMutedBy: 'h'.repeat(300) } },
      }),
    );
    await waitFor(() => seen.length > 0);
    expect(seen[0]?.participantId).toHaveLength(256);
    expect(seen[0]?.mediaStates.audio.serverMutedBy).toHaveLength(256);
  });

  it('drops a mute update with an EMPTY participant id (the same rule as the unmute relay)', async () => {
    const rig = await joinedRig(framedJoinResponse({ senderId: 258 }));
    const seen: ParticipantMuteEvent[] = [];
    rig.client.on('participantMuteChanged', (e) => seen.push(e));
    rig.deliver(framedParticipantMuteUpdate({ participantId: '' }));
    rig.deliver(framedParticipantMuteUpdate({ participantId: 'b' }));
    await waitFor(() => seen.length > 0);
    expect(seen.map((e) => e.participantId)).toEqual(['b']);
  });

  it("relays the host's unmute-request notification; an empty requester is dropped", async () => {
    const rig = await joinedRig(framedJoinResponse({ senderId: 258 }));
    const seen: UnmuteRequestedEvent[] = [];
    rig.client.on('unmuteRequested', (e) => seen.push(e));
    rig.deliver(framedUnmuteRequestRelay(''));
    rig.deliver(framedUnmuteRequestRelay('b'));
    await waitFor(() => seen.length > 0);
    expect(seen).toEqual([{ participantId: 'b', kinds: ['audio'] }]);
  });
});

describe('story 3 C-A1: join contract and per-kind mute requests', () => {
  it.each([
    ['receive-only: EMPTY', [], []],
    ['audio-only', [{ kind: 'audio', clientMuted: false }], [[MediaKind.AUDIO, false]]],
    [
      'audio + camera',
      [
        { kind: 'audio', clientMuted: false },
        { kind: 'video', clientMuted: false },
      ],
      [
        [MediaKind.AUDIO, false],
        [MediaKind.VIDEO_CAMERA, false],
      ],
    ],
    [
      'joined muted (each kind its own value)',
      [
        { kind: 'audio', clientMuted: true },
        { kind: 'video', clientMuted: false },
      ],
      [
        [MediaKind.AUDIO, true],
        [MediaKind.VIDEO_CAMERA, false],
      ],
    ],
  ] as const)(
    'the JoinRequest carries the publish DECLARATION (%s) and the full receive_capability',
    async (_label, declared, onWire) => {
      const transport = new MockWebTransport();
      const client = new SignalingClient({ connect: () => transport });
      const publish: PublishedKindDeclaration[] = declared.map((d) => ({ ...d }));
      const joining = client.join({
        webtransportEndpoint: ENDPOINT,
        meetingId: 'm',
        joinToken: 'jwt',
        participantName: 'self',
        publish,
        receiveSlots: [
          { slotId: 0, mediaKind: 'audio' },
          { slotId: 1, mediaKind: 'video' },
        ],
      });
      transport.simulateReady();
      await waitFor(() => transport.getOpenedBidiStreams().length > 0);
      transport.simulateServerMessage(0, framedJoinResponse({ senderId: 258 }));
      await joining;
      const join = decodeOutboundClientMessages(transport.getOutboundBidiWrites(0))[0];
      if (join?.message.case !== 'joinRequest') throw new Error('first message is not the join');
      const request = join.message.value;
      expect(request.publishedKinds.map((k) => [k.mediaKind, k.clientMuted])).toEqual(onWire);
      expect(request.receiveCapability?.slots.map((s) => [s.slotId, s.mediaKind])).toEqual([
        [0, MediaKind.AUDIO],
        [1, MediaKind.VIDEO_CAMERA],
      ]);
      // Stored frozen beside correlation/binding; the caller's array cannot change it.
      publish.push({ kind: 'video', clientMuted: true });
      expect(client.publishKinds).toEqual(declared.map((d) => d.kind));
      expect(Object.isFrozen(client.publishKinds)).toBe(true);
    },
  );

  // Decoded from the bytes actually written: exactly ONE MuteChange naming the
  // changed kind; every other kind is absent from `changes` (= unchanged at MC).
  it.each([
    ['audio', true, MediaKind.AUDIO],
    ['audio', false, MediaKind.AUDIO],
    ['video', true, MediaKind.VIDEO_CAMERA],
    ['video', false, MediaKind.VIDEO_CAMERA],
  ] as const)(
    'a MuteRequest names ONLY the changed kind (%s=%s)',
    async (kind, muted, wireKind) => {
      const rig = await joinedRig(framedJoinResponse({ senderId: 258 }));
      await rig.client.sendMuteRequest(kind, muted);
      const mute = rig.outbound().find((m) => m.message.case === 'muteRequest');
      if (mute?.message.case !== 'muteRequest') throw new Error('no MuteRequest sent');
      // An explicit `false` is sent as an entry, never omitted.
      expect(mute.message.value.changes.map((c) => [c.mediaKind, c.muted])).toEqual([
        [wireKind, muted],
      ]);
    },
  );

  it.each([
    ['audio', true, MediaKind.AUDIO],
    ['audio', false, MediaKind.AUDIO],
    ['video', true, MediaKind.VIDEO_CAMERA],
    ['video', false, MediaKind.VIDEO_CAMERA],
  ] as const)(
    'a ServerMuteRequest names ONLY the changed kind (%s=%s)',
    async (kind, muted, wireKind) => {
      const rig = await joinedRig(framedJoinResponse({ senderId: 258 }));
      await rig.client.sendServerMuteRequest('b', kind, muted);
      const sent = rig.outbound().find((m) => m.message.case === 'serverMuteRequest');
      if (sent?.message.case !== 'serverMuteRequest') throw new Error('no ServerMuteRequest sent');
      expect(sent.message.value.changes.map((c) => [c.mediaKind, c.muted])).toEqual([
        [wireKind, muted],
      ]);
    },
  );

  it('a directive stream for screen share is `unknown` — never sent', async () => {
    const rig = await joinedRig(framedJoinResponse({ senderId: 258 }));
    const seen: SendDirectiveEvent[] = [];
    rig.client.on('sendDirective', (e) => seen.push(e));
    rig.deliver(
      framedSendDirective({ mediaKind: MediaKind.VIDEO_SCREEN, targets: ['https://mh'] }),
    );
    await waitFor(() => seen.length > 0);
    expect(seen[0]?.streams[0]?.mediaKind).toBe('unknown');
  });
});
