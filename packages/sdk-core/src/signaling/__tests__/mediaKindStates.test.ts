// File: packages/sdk-core/src/signaling/__tests__/mediaKindStates.test.ts
//
// THE kind-keyed snapshot contract (docs/protocol/CONVENTIONS.md §6) at the
// SDK's decode boundary: the pure decoder, then the SAME malformed table driven
// through the real encode -> frame -> decode path on every carrier — the join
// response's roster, `ParticipantJoined`, and `ParticipantMuteUpdate` — plus
// `receive_slot_caps`.
//
// Every malformed row asserts the WHOLE disposition: the typed `ContractFault`
// error (`join()` rejecting with exactly it during the join), the connection
// closed, NOTHING applied (no event, no roster key, no KEK, no stored ids), and
// the bounded-reason WARN fired once.

import { create } from '@bufbuild/protobuf';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { MockWebTransport } from '@darktower/test-utils';

import { MEETING_KEK_BYTES } from '../../media/frame/sframe.js';
import { emptyKekHolder } from '../../media/__tests__/helpers.js';
import { RosterIdentityKeys } from '../../media/setup/rosterKeys.js';
import { SignalingError, SignalingErrorCode } from '../../errors/SignalingError.js';
import {
  MediaKindStateSchema,
  ReceiveSlotCapSchema,
} from '../../proto/dark_tower/signaling/v1/signaling_pb.js';
import { SignalingClient } from '../SignalingClient.js';
import type {
  ParticipantJoinedEvent,
  ParticipantMuteEvent,
  UnmuteRequestedEvent,
} from '../events.js';
import {
  CONTRACT_FAULT_REASONS as R,
  decodeMediaStates,
  decodeReceiveSlotCaps,
  decodeRelayedKinds,
  KNOWN_WIRE_MEDIA_KINDS,
  type ContractFaultReason,
} from '../mediaKindStates.js';
import {
  ClientMediaState,
  framedJoinResponse,
  framedParticipantJoined,
  framedParticipantMuteUpdate,
  framedUnmuteRequestRelay,
  LIVE_STATES,
  MediaKind,
  mediaStatesList,
  receiveSlotCapsList,
  waitFor,
  type RawMediaKindState,
  type RawReceiveSlotCap,
} from './helpers.js';

const ENDPOINT = 'https://mc.example:4433';
const UNKNOWN_KIND = 99 as MediaKind;
const KEK = new Uint8Array(MEETING_KEK_BYTES).fill(0x22);
/** Verification material a joiner carries, so a roster-key install is observable. */
const JOINER_KEY = { senderId: 9, identityPublicKey: new Uint8Array(32).fill(0x33) };

/** Test lists are init shapes; the decoder takes decoded messages. */
const states = (l: readonly RawMediaKindState[]) => l.map((e) => create(MediaKindStateSchema, e));
const caps = (l: readonly RawReceiveSlotCap[]) => l.map((e) => create(ReceiveSlotCapSchema, e));

const live = (mediaKind: MediaKind): RawMediaKindState => ({
  mediaKind,
  clientState: ClientMediaState.LIVE,
  serverMuted: false,
  serverMutedBy: '',
});

/** `mediaStatesList()` with one kind's entry replaced (or removed with `undefined`). */
function withEntry(kind: MediaKind, entry: RawMediaKindState | undefined): RawMediaKindState[] {
  return mediaStatesList().flatMap((e) => (e.mediaKind === kind ? (entry ? [entry] : []) : [e]));
}

/** The malformed table (session-fatal), shared by every carrier. */
const MALFORMED: readonly (readonly [string, RawMediaKindState[]])[] = [
  ['no entries at all', []],
  ['an UNSPECIFIED kind', [...mediaStatesList(), live(MediaKind.UNSPECIFIED)]],
  ['an unknown kind number', [...mediaStatesList(), live(UNKNOWN_KIND)]],
  // Same entry COUNT as a complete list: only the known-kind check catches it.
  [
    'an unknown kind number IN PLACE OF a known one',
    withEntry(MediaKind.AUDIO, live(UNKNOWN_KIND)),
  ],
  [
    'an UNSPECIFIED client state',
    withEntry(MediaKind.AUDIO, {
      ...live(MediaKind.AUDIO),
      clientState: ClientMediaState.UNSPECIFIED,
    }),
  ],
  [
    'an unknown client-state number',
    withEntry(MediaKind.VIDEO_CAMERA, {
      ...live(MediaKind.VIDEO_CAMERA),
      clientState: 42 as ClientMediaState,
    }),
  ],
  ['a missing known kind (camera)', withEntry(MediaKind.VIDEO_CAMERA, undefined)],
  [
    'a missing known kind (screen — required like any kind)',
    withEntry(MediaKind.VIDEO_SCREEN, undefined),
  ],
  ['a duplicate kind', [...mediaStatesList(), live(MediaKind.AUDIO)]],
];

afterEach(() => {
  vi.restoreAllMocks();
});

describe('decodeMediaStates (pure)', () => {
  it('derives the known kinds from the generated enum: every named kind but UNSPECIFIED', () => {
    expect([...KNOWN_WIRE_MEDIA_KINDS].sort()).toEqual(
      [MediaKind.AUDIO, MediaKind.VIDEO_CAMERA, MediaKind.VIDEO_SCREEN].sort(),
    );
  });

  it.each([
    [ClientMediaState.NOT_PUBLISHED, 'not_published'],
    [ClientMediaState.LIVE, 'live'],
    [ClientMediaState.CLIENT_MUTED, 'client_muted'],
  ] as const)('maps client state %s to %s, per kind, with no third "unknown"', (wire, token) => {
    const decoded = decodeMediaStates(
      states(mediaStatesList({ audio: { clientState: wire }, video: { clientState: wire } })),
    );
    expect(decoded?.audio.clientState).toBe(token);
    expect(decoded?.video.clientState).toBe(token);
  });

  it('keeps client and server mute independent per kind; serverMutedBy only while muted', () => {
    const decoded = decodeMediaStates(
      states(
        mediaStatesList({
          audio: {
            clientState: ClientMediaState.CLIENT_MUTED,
            serverMuted: true,
            serverMutedBy: 'h',
          },
          // A stale `by` on an unmuted kind is not projected.
          video: { clientState: ClientMediaState.LIVE, serverMuted: false, serverMutedBy: 'stale' },
        }),
      ),
    );
    expect(decoded).toEqual({
      audio: { clientState: 'client_muted', serverMuted: true, serverMutedBy: 'h' },
      video: { clientState: 'live', serverMuted: false },
    });
  });

  it.each(MALFORMED)('refuses %s', (_label, entries) => {
    expect(decodeMediaStates(states(entries))).toBeUndefined();
  });

  it('accepts a screen entry WHATEVER its value, and never folds it into the camera', () => {
    const decoded = decodeMediaStates(
      states(
        mediaStatesList({
          screen: { clientState: ClientMediaState.LIVE, serverMuted: true, serverMutedBy: 'h' },
        }),
      ),
    );
    expect(decoded).toEqual(LIVE_STATES);
  });

  it('decodes receive_slot_caps per kind (a 0 stays 0) and refuses empty, incomplete or unknown', () => {
    expect(decodeReceiveSlotCaps(caps(receiveSlotCapsList({ audio: 5, video: 0 })))).toEqual({
      audio: 5,
      video: 0,
    });
    expect(decodeReceiveSlotCaps(caps([]))).toBeUndefined();
    expect(
      decodeReceiveSlotCaps(
        caps(receiveSlotCapsList().filter((c) => c.mediaKind !== MediaKind.VIDEO_SCREEN)),
      ),
    ).toBeUndefined();
    expect(
      decodeReceiveSlotCaps(
        caps([...receiveSlotCapsList(), { mediaKind: UNKNOWN_KIND, maxSlots: 1 }]),
      ),
    ).toBeUndefined();
  });
});

// ---------------------------------------------------------------------------
// Through the real SignalingClient
// ---------------------------------------------------------------------------

interface Started {
  readonly client: SignalingClient;
  readonly transport: MockWebTransport;
  readonly errors: SignalingError[];
  readonly close: ReturnType<typeof vi.spyOn>;
  readonly upsert: ReturnType<typeof vi.spyOn>;
  readonly warn: ReturnType<typeof vi.spyOn>;
  readonly kek: ReturnType<typeof emptyKekHolder>;
  readonly joining: Promise<unknown>;
}

async function start(): Promise<Started> {
  const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});
  const transport = new MockWebTransport();
  const errors: SignalingError[] = [];
  const kek = emptyKekHolder();
  const roster = new RosterIdentityKeys(8);
  const client = new SignalingClient({
    connect: () => transport,
    kekSink: kek,
    rosterKeys: roster,
  });
  client.on('error', (e) => errors.push(e));
  const close = vi.spyOn(transport, 'close');
  const upsert = vi.spyOn(roster, 'upsert');
  const joining = client.join({
    webtransportEndpoint: ENDPOINT,
    meetingId: 'm',
    joinToken: 'jwt',
    participantName: 'self',
    publish: [{ kind: 'audio', clientMuted: false }],
    receiveSlots: [{ slotId: 0, mediaKind: 'audio' }],
  });
  joining.catch(() => {});
  transport.simulateReady();
  await waitFor(() => transport.getOpenedBidiStreams().length > 0);
  return { client, transport, errors, close, upsert, warn, kek, joining };
}

async function joined(): Promise<Started> {
  const s = await start();
  s.transport.simulateServerMessage(0, framedJoinResponse({ senderId: 258 }));
  await s.joining;
  return s;
}

/** The bounded-reason WARNs fired so far. */
function contractWarns(s: Started): unknown[][] {
  return s.warn.mock.calls.filter((c: unknown[]) => c[0] === '[dt-signaling-contract]');
}

function expectWarned(s: Started, reason: ContractFaultReason): void {
  expect(contractWarns(s)).toEqual([
    ['[dt-signaling-contract]', `malformed kind-keyed snapshot from MC: ${reason}`],
  ]);
}

async function expectJoinFault(s: Started, reason: ContractFaultReason): Promise<void> {
  const err = await s.joining.then(
    () => undefined,
    (e: unknown) => e,
  );
  // The EXACT typed error — not a generic transport failure.
  expect(err).toBeInstanceOf(SignalingError);
  expect((err as SignalingError).signalingCode).toBe(SignalingErrorCode.ContractFault);
  expect(s.close).toHaveBeenCalled();
  expectWarned(s, reason);
  // Nothing applied: no KEK installed, no roster key, no stored ids.
  expect(s.kek.isProvisioned).toBe(false);
  expect(s.upsert).not.toHaveBeenCalled();
  expect(s.client.correlationId).toBeUndefined();
}

describe('JoinResponse: receive_slot_caps is REQUIRED, one entry per known kind', () => {
  it.each([
    ['missing', []],
    [
      'incomplete (no screen entry)',
      receiveSlotCapsList().filter((c) => c.mediaKind !== MediaKind.VIDEO_SCREEN),
    ],
    ['duplicated', [...receiveSlotCapsList(), { mediaKind: MediaKind.AUDIO, maxSlots: 1 }]],
    [
      'an UNSPECIFIED kind',
      [...receiveSlotCapsList(), { mediaKind: MediaKind.UNSPECIFIED, maxSlots: 1 }],
    ],
    [
      'an unknown kind number',
      [...receiveSlotCapsList(), { mediaKind: UNKNOWN_KIND, maxSlots: 1 }],
    ],
  ] as const)('refuses the join when the caps are %s', async (_label, list) => {
    const s = await start();
    s.transport.simulateServerMessage(
      0,
      framedJoinResponse({
        senderId: 258,
        meetingKek: KEK,
        correlationId: 'corr',
        rawReceiveSlotCaps: list as RawReceiveSlotCap[],
      }),
    );
    await expectJoinFault(s, R.ReceiveSlotCapsMalformed);
  });
});

describe.each([
  ['the JoinResponse roster', 'join'],
  ['ParticipantJoined', 'participantJoined'],
  ['ParticipantMuteUpdate', 'participantMuteUpdate'],
] as const)('media_states on %s', (_carrier, carrier) => {
  it.each(MALFORMED)('ENDS THE SESSION on %s — nothing applied', async (_l, entries) => {
    if (carrier === 'join') {
      const s = await start();
      s.transport.simulateServerMessage(
        0,
        framedJoinResponse({
          senderId: 258,
          meetingKek: KEK,
          correlationId: 'corr',
          participants: [
            { participantId: 'ok', name: 'Ok', senderId: 7, identityPublicKey: new Uint8Array(32) },
            { participantId: 'bad', name: 'Bad', rawMediaStates: entries },
          ],
        }),
      );
      await expectJoinFault(s, R.MediaStatesMalformed);
      return;
    }
    const s = await joined();
    const joinedEvents: ParticipantJoinedEvent[] = [];
    const muteEvents: ParticipantMuteEvent[] = [];
    s.client.on('participantJoined', (e) => joinedEvents.push(e));
    s.client.on('participantMuteChanged', (e) => muteEvents.push(e));
    const upsertsBefore = s.upsert.mock.calls.length;
    s.transport.simulateServerMessage(
      0,
      carrier === 'participantJoined'
        ? framedParticipantJoined('bad', 'Bad', { raw: entries }, JOINER_KEY)
        : framedParticipantMuteUpdate({ participantId: 'bad', states: { raw: entries } }),
    );
    await waitFor(() => s.errors.length > 0);
    expect(s.errors[0]?.signalingCode).toBe(SignalingErrorCode.ContractFault);
    expect(s.close).toHaveBeenCalled();
    expectWarned(s, R.MediaStatesMalformed);
    // Never added half-known, never dropped silently: no event at all, the
    // session ends instead.
    expect(joinedEvents).toEqual([]);
    expect(muteEvents).toEqual([]);
    // A malformed joiner's identity key is never installed.
    expect(s.upsert.mock.calls.length).toBe(upsertsBefore);
  });
});

describe('a well-formed snapshot is never a fault (positive control)', () => {
  it('a join, a joiner and an update with complete lists apply and WARN nothing', async () => {
    const s = await joined();
    const seen: unknown[] = [];
    s.client.on('participantJoined', (e) => seen.push(e.participant.mediaStates));
    s.client.on('participantMuteChanged', (e) => seen.push(e.mediaStates));
    const upsertsBefore = s.upsert.mock.calls.length;
    s.transport.simulateServerMessage(0, framedParticipantJoined('n', 'N', {}, JOINER_KEY));
    s.transport.simulateServerMessage(0, framedParticipantMuteUpdate({ participantId: 'n' }));
    await waitFor(() => seen.length > 1);
    expect(seen).toEqual([LIVE_STATES, LIVE_STATES]);
    // Positive control for the no-upsert assertion: a WELL-FORMED joiner with
    // the same key material IS installed.
    expect(s.upsert.mock.calls.length).toBe(upsertsBefore + 1);
    expect(contractWarns(s)).toEqual([]);
    expect(s.errors).toEqual([]);
    expect(s.close).not.toHaveBeenCalled();
  });
});

describe("the host's UnmuteRequest relay carries the requested kinds (CONVENTIONS §6)", () => {
  it.each([
    [[MediaKind.AUDIO], ['audio']],
    [[MediaKind.VIDEO_CAMERA], ['video']],
    [
      [MediaKind.VIDEO_CAMERA, MediaKind.AUDIO],
      ['video', 'audio'],
    ],
  ] as const)('decodes %j to %j', (wire, tokens) => {
    expect(decodeRelayedKinds([...wire])).toEqual(tokens);
  });

  it.each([
    ['empty', []],
    ['a duplicate', [MediaKind.AUDIO, MediaKind.AUDIO]],
    ['screen (not a kind this SDK serves)', [MediaKind.VIDEO_SCREEN]],
    ['UNSPECIFIED', [MediaKind.UNSPECIFIED]],
    ['an unknown number', [UNKNOWN_KIND]],
  ] as const)('a relay with %s kinds ENDS THE SESSION, nothing emitted', async (_l, kinds) => {
    expect(decodeRelayedKinds([...kinds])).toBeUndefined();
    const s = await joined();
    const seen: UnmuteRequestedEvent[] = [];
    s.client.on('unmuteRequested', (e) => seen.push(e));
    s.transport.simulateServerMessage(0, framedUnmuteRequestRelay('b', [...kinds]));
    await waitFor(() => s.errors.length > 0);
    expect(s.errors[0]?.signalingCode).toBe(SignalingErrorCode.ContractFault);
    expect(s.close).toHaveBeenCalled();
    expectWarned(s, R.UnmuteRequestMalformed);
    expect(seen).toEqual([]);
  });

  it('a well-formed relay reaches the host with its kinds (positive control)', async () => {
    const s = await joined();
    const seen: UnmuteRequestedEvent[] = [];
    s.client.on('unmuteRequested', (e) => seen.push(e));
    s.transport.simulateServerMessage(
      0,
      framedUnmuteRequestRelay('b', [MediaKind.AUDIO, MediaKind.VIDEO_CAMERA]),
    );
    await waitFor(() => seen.length > 0);
    expect(seen).toEqual([{ participantId: 'b', kinds: ['audio', 'video'] }]);
    expect(contractWarns(s)).toEqual([]);
  });
});
