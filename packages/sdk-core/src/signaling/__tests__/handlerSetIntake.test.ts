// File: packages/sdk-core/src/signaling/__tests__/handlerSetIntake.test.ts
//
// The ONE decode site of a `MediaHandlerSet` (ADR-0039 §4, §5) and the
// membership rule every `SendTarget`/`StreamAssignment` ref is checked against.
//   * validation drops faulted entries (fail-safe: removed, never redirected);
//   * the dial tokens go ONLY to the private sink and are BLANKED on the decoded
//     message; no public event carries one;
//   * a non-member ref is ignored and surfaced ONCE (`[dt-signaling-contract]`);
//   * the superseded URL fields are never read: they are POISONED with values
//     that disagree with the set, and behaviour follows the refs.

import { afterEach, describe, expect, it, vi } from 'vitest';
import { create } from '@bufbuild/protobuf';
import { MockWebTransport } from '@darktower/test-utils';

import { SignalingClient } from '../SignalingClient.js';
import type { HandlerSetIntake } from '../../media/handlerSet.js';
import { HANDLER_SET_FAULTS, takeHandlerSet, type HandlerSetFault } from '../handlerSetIntake.js';
import type { JoinedEvent, SendDirectiveEvent, StreamAssignmentsEvent } from '../events.js';
import {
  MediaHandlerSetSchema,
  MediaServerInfoSchema,
  SlotState,
} from '../../proto/dark_tower/signaling/v1/signaling_pb.js';
import { MAX_HANDLER_SET_SIZE, MAX_MEDIA_DIAL_TOKEN_LENGTH } from '../../validation/limits.js';
import {
  framedJoinResponse,
  framedMediaHandlerSet,
  framedSendDirective,
  framedStreamAssignments,
  waitFor,
  type HandlerSetInit,
} from './helpers.js';

afterEach(() => {
  vi.restoreAllMocks();
});

function wireSet(entries: { ref: number; url: string; token: string }[], version = 5n) {
  return create(MediaHandlerSetSchema, {
    handlerSetVersion: version,
    handlers: entries.map((e) =>
      create(MediaServerInfoSchema, {
        handlerRef: e.ref,
        mediaHandlerUrl: e.url,
        mediaDialToken: e.token,
      }),
    ),
  });
}

function take(
  wire: ReturnType<typeof wireSet> | undefined,
  known = new Map<number, string>(),
): { intake: HandlerSetIntake; faults: HandlerSetFault[] } {
  const faults: HandlerSetFault[] = [];
  const intake = takeHandlerSet(wire, known as never, (f) => faults.push(f));
  return { intake, faults };
}

describe('takeHandlerSet — validate, project, blank', () => {
  it('projects a valid set: token-free view + ref->token map, wire order kept; version echoed exactly', () => {
    const wire = wireSet(
      [
        { ref: 7, url: 'https://b', token: 't7' },
        { ref: 3, url: 'https://a', token: 't3' },
      ],
      2n ** 64n - 1n,
    );
    const { intake, faults } = take(wire);
    expect(faults).toEqual([]);
    expect(intake.view).toEqual({
      version: '18446744073709551615',
      handlers: [
        { ref: 7, url: 'https://b' },
        { ref: 3, url: 'https://a' },
      ],
    });
    expect(intake.version).toBe(2n ** 64n - 1n);
    expect([...intake.dialTokens]).toEqual([
      [7, 't7'],
      [3, 't3'],
    ]);
    // BLANKED on the decoded message: nothing can print a token from it now.
    expect(wire.handlers.map((h) => h.mediaDialToken)).toEqual(['', '']);
  });

  it.each([
    ['ref 0', [{ ref: 0, url: 'https://z', token: 't' }], HANDLER_SET_FAULTS.ZeroRef],
    ['empty url', [{ ref: 1, url: '', token: 't' }], HANDLER_SET_FAULTS.BadUrl],
    [
      'oversized url',
      [{ ref: 1, url: `https://${'u'.repeat(300)}`, token: 't' }],
      HANDLER_SET_FAULTS.BadUrl,
    ],
    ['empty token', [{ ref: 1, url: 'https://a', token: '' }], HANDLER_SET_FAULTS.BadToken],
    [
      'oversized token',
      [{ ref: 1, url: 'https://a', token: 'x'.repeat(MAX_MEDIA_DIAL_TOKEN_LENGTH + 1) }],
      HANDLER_SET_FAULTS.BadToken,
    ],
  ])(
    '%s: a counted fault; the entry is DROPPED (a non-member), the rest kept',
    (_n, bad, fault) => {
      const wire = wireSet([...bad, { ref: 9, url: 'https://ok', token: 'ok' }]);
      const { intake, faults } = take(wire);
      expect(faults).toEqual([fault]);
      expect(intake.view.handlers).toEqual([{ ref: 9, url: 'https://ok' }]);
      expect([...intake.dialTokens.keys()]).toEqual([9]);
      // Even the dropped entry's token is blanked.
      expect(wire.handlers.every((h) => h.mediaDialToken === '')).toBe(true);
    },
  );

  it('a DUPLICATE ref drops EVERY entry involved (no wire order picks a winner)', () => {
    const { intake, faults } = take(
      wireSet([
        { ref: 1, url: 'https://a', token: 'first' },
        { ref: 1, url: 'https://b', token: 'second' },
        { ref: 2, url: 'https://c', token: 'ok' },
      ]),
    );
    expect(faults).toContain(HANDLER_SET_FAULTS.DuplicateRef);
    expect(intake.view.handlers).toEqual([{ ref: 2, url: 'https://c' }]);
  });

  it('a DUPLICATE url drops EVERY entry involved', () => {
    const { intake, faults } = take(
      wireSet([
        { ref: 1, url: 'https://same', token: 'a' },
        { ref: 2, url: 'https://same', token: 'b' },
        { ref: 3, url: 'https://other', token: 'c' },
      ]),
    );
    expect(faults).toContain(HANDLER_SET_FAULTS.DuplicateUrl);
    expect(intake.view.handlers).toEqual([{ ref: 3, url: 'https://other' }]);
  });

  it('a KNOWN ref at a different url is dropped (removed, never redirected); same url is fine', () => {
    const known = new Map([[1, 'https://a']]);
    const moved = take(wireSet([{ ref: 1, url: 'https://elsewhere', token: 't' }]), known);
    expect(moved.faults).toEqual([HANDLER_SET_FAULTS.UrlChanged]);
    expect(moved.intake.view.handlers).toEqual([]);
    // Positive arm: same ref, same url.
    const kept = take(wireSet([{ ref: 1, url: 'https://a', token: 't' }]), known);
    expect(kept.faults).toEqual([]);
    expect(kept.intake.view.handlers).toEqual([{ ref: 1, url: 'https://a' }]);
  });

  it('an oversized set keeps the first MAX_HANDLER_SET_SIZE entries and faults', () => {
    const many = Array.from({ length: MAX_HANDLER_SET_SIZE + 3 }, (_v, i) => ({
      ref: i + 1,
      url: `https://mh-${i + 1}`,
      token: `t${i + 1}`,
    }));
    const { intake, faults } = take(wireSet(many));
    expect(faults).toEqual([HANDLER_SET_FAULTS.Oversized]);
    expect(intake.view.handlers).toHaveLength(MAX_HANDLER_SET_SIZE);
  });

  it('a join response with NO set is EXACTLY an empty set — never a fault (proto rule)', () => {
    const absent = take(undefined);
    const empty = take(wireSet([], 0n));
    expect(absent.faults).toEqual([]);
    expect(absent.intake.view).toEqual({ version: '0', handlers: [] });
    expect(absent.intake.view).toEqual(empty.intake.view);
    expect([...absent.intake.dialTokens]).toEqual([...empty.intake.dialTokens]);
  });
});

// ---------------------------------------------------------------------------
// Through SignalingClient
// ---------------------------------------------------------------------------

interface Rig {
  readonly client: SignalingClient;
  readonly transport: MockWebTransport;
  readonly intakes: HandlerSetIntake[];
  readonly joined: JoinedEvent;
  /** Every `onNonMemberRef` call (story 4 C2's count), in order. */
  readonly nonMembers: string[];
  deliver(bytes: Uint8Array): void;
}

const SET: HandlerSetInit = {
  version: 4n,
  handlers: [
    { ref: 1, url: 'https://mh-1', token: 'SECRET-TOKEN-1' },
    { ref: 2, url: 'https://mh-2', token: 'SECRET-TOKEN-2' },
  ],
};

async function joinedRig(joinResponse: Uint8Array): Promise<Rig> {
  const transport = new MockWebTransport();
  const intakes: HandlerSetIntake[] = [];
  const nonMembers: string[] = [];
  const client = new SignalingClient({
    connect: () => transport,
    handlerSetSink: (intake) => intakes.push(intake),
    onNonMemberRef: (source) => nonMembers.push(source),
  });
  const joining = client.join({
    webtransportEndpoint: 'https://mc',
    meetingId: 'm',
    joinToken: 'jwt',
    participantName: 'self',
    publish: [{ kind: 'audio', clientMuted: false }],
    receiveSlots: [{ slotId: 0, mediaKind: 'audio' }],
  });
  transport.simulateReady();
  await waitFor(() => transport.getOpenedBidiStreams().length > 0);
  transport.simulateServerMessage(0, joinResponse);
  const joined = await joining;
  return {
    client,
    transport,
    intakes,
    joined,
    nonMembers,
    deliver: (bytes) => transport.simulateServerMessage(0, bytes),
  };
}

describe('SignalingClient — the set reaches the transport with tokens, the public surface without', () => {
  it('join-time and pushed sets go to the sink with tokens; events carry refs+urls only', async () => {
    const rig = await joinedRig(framedJoinResponse({ handlerSet: SET }));
    const views: unknown[] = [];
    rig.client.on('mediaHandlerSet', (v) => views.push(v));
    rig.deliver(
      framedMediaHandlerSet({
        version: 4n,
        handlers: [{ ref: 1, url: 'https://mh-1', token: 'SECRET-TOKEN-1b' }],
      }),
    );
    await waitFor(() => views.length > 0);

    // Positive control: the tokens DID arrive, at the sink.
    expect(rig.intakes.map((i) => [...i.dialTokens.values()])).toEqual([
      ['SECRET-TOKEN-1', 'SECRET-TOKEN-2'],
      ['SECRET-TOKEN-1b'],
    ]);
    expect(rig.joined.handlerSet.handlers.map((h) => h.ref)).toEqual([1, 2]);
    expect(views).toEqual([{ version: '4', handlers: [{ ref: 1, url: 'https://mh-1' }] }]);
    // ...and nowhere public.
    const publicSurface = JSON.stringify({ joined: rig.joined, views });
    expect(publicSurface).toContain('https://mh-1');
    expect(publicSurface).not.toContain('SECRET-TOKEN');
    rig.client.close();
  });
});

describe('membership — a non-member ref is ignored and surfaced ONCE, never dialed', () => {
  it('SendTarget: members kept, a non-member (and 0) dropped, one WARN per session', async () => {
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => undefined);
    const rig = await joinedRig(framedJoinResponse({ handlerSet: SET }));
    const seen: SendDirectiveEvent[] = [];
    rig.client.on('sendDirective', (e) => seen.push(e));
    rig.deliver(framedSendDirective({ targets: [1, 9, 0, 2] }));
    rig.deliver(framedSendDirective({ targets: [8] }));
    await waitFor(() => seen.length > 1);

    expect(seen[0]!.streams[0]!.targets).toEqual([1, 2]);
    expect(seen[1]!.streams[0]!.targets).toEqual([]);
    const notices = warn.mock.calls.filter((c) => c[0] === '[dt-signaling-contract]');
    expect(notices).toHaveLength(1);
    // Bounded: no ref in the text.
    expect(String(notices[0]![1])).not.toMatch(/\b[89]\b/);
    rig.client.close();
  });

  it('StreamAssignment: 0 on a no-edge state is SILENT; 0 on ACTIVE or a non-member is undefined + one WARN', async () => {
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => undefined);
    const rig = await joinedRig(framedJoinResponse({ handlerSet: SET }));
    const seen: StreamAssignmentsEvent[] = [];
    rig.client.on('streamAssignments', (e) => seen.push(e));

    for (const state of [
      SlotState.UNAVAILABLE,
      SlotState.FEWER_SOURCES_THAN_SLOTS,
      SlotState.AVATAR_NO_VIDEO,
    ]) {
      rig.deliver(framedStreamAssignments({ slotId: 0, handlerRef: 0, slotState: state }));
    }
    await waitFor(() => seen.length === 3);
    expect(seen.every((e) => e.assignments[0]!.handlerRef === undefined)).toBe(true);
    expect(warn.mock.calls.filter((c) => c[0] === '[dt-signaling-contract]')).toHaveLength(0);

    // Positive arm: 0 on an edge-carrying state IS a fault...
    rig.deliver(framedStreamAssignments({ slotId: 0, senderId: 300, handlerRef: 0 }));
    // ...as is a non-member; the member is kept.
    rig.deliver(
      framedStreamAssignments({
        slotId: 0,
        senderId: 300,
        handlerRef: 77,
        extraSlots: [{ slotId: 1, senderId: 301, handlerRef: 2 }],
      }),
    );
    await waitFor(() => seen.length === 5);
    expect(seen[3]!.assignments[0]!.handlerRef).toBeUndefined();
    expect(seen[4]!.assignments.map((a) => a.handlerRef)).toEqual([undefined, 2]);
    expect(warn.mock.calls.filter((c) => c[0] === '[dt-signaling-contract]')).toHaveLength(1);
    rig.client.close();
  });

  it('a faulted set entry is a non-member in BOTH the sink and the membership check', async () => {
    vi.spyOn(console, 'warn').mockImplementation(() => undefined);
    const rig = await joinedRig(
      framedJoinResponse({
        handlerSet: {
          version: 1n,
          handlers: [
            { ref: 1, url: 'https://dup' },
            { ref: 2, url: 'https://dup' },
            { ref: 3, url: 'https://ok' },
          ],
        },
      }),
    );
    expect([...rig.intakes[0]!.dialTokens.keys()]).toEqual([3]);
    const seen: SendDirectiveEvent[] = [];
    rig.client.on('sendDirective', (e) => seen.push(e));
    rig.deliver(framedSendDirective({ targets: [1, 2, 3] }));
    await waitFor(() => seen.length > 0);
    expect(seen[0]!.streams[0]!.targets).toEqual([3]);
    rig.client.close();
  });

  it('a REMOVAL re-check resolves stale assignments SILENTLY; a later non-member arrival still WARNs once', async () => {
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => undefined);
    const rig = await joinedRig(framedJoinResponse({ handlerSet: SET }));
    const seen: StreamAssignmentsEvent[] = [];
    rig.client.on('streamAssignments', (e) => seen.push(e));
    rig.deliver(framedStreamAssignments({ slotId: 0, senderId: 300, handlerRef: 2 }));
    await waitFor(() => seen.length === 1);
    expect(seen[0]!.assignments[0]!.handlerRef).toBe(2);

    // MC sends the set that removes ref 2 BEFORE re-routing: the held
    // assignment is stale, not a contract violation.
    rig.deliver(framedMediaHandlerSet({ version: 5n, handlers: [SET.handlers[0]!] }));
    await waitFor(() => seen.length === 2);
    expect(seen[1]!.assignments[0]!.handlerRef).toBeUndefined();
    const notices = () => warn.mock.calls.filter((c) => c[0] === '[dt-signaling-contract]');
    expect(notices()).toHaveLength(0);

    // Positive arm: a non-member arriving on an assignment IS surfaced, once.
    rig.deliver(framedStreamAssignments({ slotId: 0, senderId: 300, handlerRef: 2 }));
    await waitFor(() => seen.length === 3);
    expect(notices()).toHaveLength(1);
    rig.client.close();
  });

  it('a membership change re-checks the held assignments against the NEW set', async () => {
    vi.spyOn(console, 'warn').mockImplementation(() => undefined);
    const rig = await joinedRig(framedJoinResponse({ handlerSet: SET }));
    const seen: StreamAssignmentsEvent[] = [];
    rig.client.on('streamAssignments', (e) => seen.push(e));
    rig.deliver(framedStreamAssignments({ slotId: 0, senderId: 300, handlerRef: 5 }));
    await waitFor(() => seen.length === 1);
    expect(seen[0]!.assignments[0]!.handlerRef).toBeUndefined();

    // A token-only refresh (same refs) re-emits nothing.
    rig.deliver(framedMediaHandlerSet({ ...SET }));
    // The set gains ref 5: the held assignment now resolves.
    rig.deliver(
      framedMediaHandlerSet({
        version: 5n,
        handlers: [...SET.handlers, { ref: 5, url: 'https://mh-5' }],
      }),
    );
    await waitFor(() => seen.length === 2);
    expect(seen[1]!.assignments[0]!.handlerRef).toBe(5);
    rig.client.close();
  });
});

describe('story 4 C2 — the non-member COUNT (one per occurrence in a FRESH message)', () => {
  it('send directive: a member counts 0, each non-member (0 included) counts 1, every time', async () => {
    vi.spyOn(console, 'warn').mockImplementation(() => undefined);
    const rig = await joinedRig(framedJoinResponse({ handlerSet: SET }));
    const seen: SendDirectiveEvent[] = [];
    rig.client.on('sendDirective', (e) => seen.push(e));
    rig.deliver(framedSendDirective({ targets: [1, 2] }));
    await waitFor(() => seen.length === 1);
    expect(rig.nonMembers).toEqual([]); // members only: 0
    rig.deliver(framedSendDirective({ targets: [1, 9, 0] }));
    rig.deliver(framedSendDirective({ targets: [9] }));
    await waitFor(() => seen.length === 3);
    // Repeats COUNT AGAIN (the WARN stays once; the count does not).
    expect(rig.nonMembers).toEqual(['send_directive', 'send_directive', 'send_directive']);
    rig.client.close();
  });

  it('stream assignment: 0 on a no-edge state and a stale re-check count 0; a FRESH non-member counts 1', async () => {
    vi.spyOn(console, 'warn').mockImplementation(() => undefined);
    const rig = await joinedRig(framedJoinResponse({ handlerSet: SET }));
    const seen: StreamAssignmentsEvent[] = [];
    rig.client.on('streamAssignments', (e) => seen.push(e));
    rig.deliver(
      framedStreamAssignments({ slotId: 0, handlerRef: 0, slotState: SlotState.UNAVAILABLE }),
    );
    rig.deliver(framedStreamAssignments({ slotId: 0, senderId: 300, handlerRef: 2 }));
    await waitFor(() => seen.length === 2);
    // The set removes ref 2: the re-check resolves the held assignment STALE.
    rig.deliver(framedMediaHandlerSet({ version: 5n, handlers: [SET.handlers[0]!] }));
    await waitFor(() => seen.length === 3);
    expect(rig.nonMembers).toEqual([]);
    // Positive arm, same fixture: the same ref in a FRESH message counts 1.
    rig.deliver(framedStreamAssignments({ slotId: 0, senderId: 300, handlerRef: 2 }));
    await waitFor(() => seen.length === 4);
    expect(rig.nonMembers).toEqual(['stream_assignment']);
    rig.client.close();
  });
});
