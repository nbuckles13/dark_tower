// File: packages/web-app/src/lib/e2eBus.ts
//
// R-29: the `window.__darktower_test__` E2E event bus — the stable contract the
// test-owned Playwright specs (#18/#19) consume. It is a replay-buffered bus of
// BOUNDED, non-PII events projected from the SDK's session events.
//
// SECURITY (R-23 / @security / @semantic-guard):
//   - The ENTIRE active body is behind `if (__E2E_HOOKS__)` — a SEPARATE define
//     from __DEV_TRUST_FINGERPRINT__ that is `false` in prod, so the whole bus
//     (window attach + replay buffer) is dead-code-eliminated. A prod
//     bundle-content test asserts `__darktower_test__` is absent from the bundle.
//   - Every payload is an explicit WHITELIST projection — never a raw event or
//     Error. `joined` DROPS `bindingToken` (reconnection credential) and
//     `correlationId`; `userId` is stringified (bigint isn't structured-clone
//     safe for `page.evaluate` reads anyway). Errors cross via `SdkError.toJSON()`
//     (the R-23 non-secret allowlist), never a raw Error / `.cause` / `.stack`.

import type { MeetingSessionEventMap, RosterParticipant, SdkError } from '@darktower/sdk-core';

/** The minimal surface installE2EHooks needs: subscribe to typed session events. */
export interface SessionEvents {
  on<K extends keyof MeetingSessionEventMap>(
    type: K,
    listener: (payload: MeetingSessionEventMap[K]) => void,
  ): () => void;
}

type BusEvent = Readonly<Record<string, unknown>> & { readonly type: string };

interface InternalBus {
  events: BusEvent[];
  readonly listeners: Map<string, Set<(event: BusEvent) => void>>;
  on(type: string, listener: (event: BusEvent) => void): () => void;
  emit(event: BusEvent): void;
}

/**
 * Subscribe the `window.__darktower_test__` bus to `session`'s events. No-op (and
 * fully tree-shaken) in production builds. Safe to call once at startup.
 */
export function installE2EHooks(session: SessionEvents): void {
  if (__E2E_HOOKS__) {
    const roster = (p: RosterParticipant): Record<string, string> => ({
      participantId: p.participantId,
      name: p.name,
    });

    const projectError = (err: SdkError): Record<string, unknown> => {
      // toJSON() is the R-23 redaction boundary — a fixed non-secret allowlist.
      const json = err.toJSON();
      const out: Record<string, unknown> = { code: json.code, message: json.message };
      if (json.status !== undefined) out['status'] = json.status;
      if (json.serverCode !== undefined) out['serverCode'] = json.serverCode;
      return out;
    };

    const existing = window.__darktower_test__ as InternalBus | undefined;
    const bus: InternalBus = existing ?? {
      events: [],
      listeners: new Map<string, Set<(event: BusEvent) => void>>(),
      on(type, listener) {
        let set = this.listeners.get(type);
        if (set === undefined) {
          set = new Set();
          this.listeners.set(type, set);
        }
        set.add(listener);
        return () => {
          this.listeners.get(type)?.delete(listener);
        };
      },
      emit(event) {
        this.events.push(event);
        const set = this.listeners.get(event.type);
        if (set !== undefined) {
          for (const listener of [...set]) listener(event);
        }
      },
    };
    window.__darktower_test__ = bus;

    session.on('stateChange', (state) => bus.emit({ type: 'stateChange', state }));
    session.on('joined', (event) =>
      bus.emit({
        type: 'joined',
        participantId: event.participantId,
        userId: event.userId.toString(),
        participants: event.existingParticipants.map(roster),
        mediaServers: [...event.mediaServers],
        // NOTE: bindingToken + correlationId are intentionally NOT projected (R-23).
      }),
    );
    session.on('participantJoined', (event) =>
      bus.emit({
        type: 'participantJoined',
        participantId: event.participant.participantId,
        name: event.participant.name,
      }),
    );
    session.on('participantLeft', (event) =>
      bus.emit({
        type: 'participantLeft',
        participantId: event.participantId,
        reason: event.reason,
      }),
    );
    session.on('mediaConnected', (url) => bus.emit({ type: 'mediaConnected', mhUrl: url }));
    session.on('error', (err) => bus.emit({ type: 'error', ...projectError(err) }));
  }
}
