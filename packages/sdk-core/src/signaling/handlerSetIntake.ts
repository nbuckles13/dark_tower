// File: packages/sdk-core/src/signaling/handlerSetIntake.ts
//
// THE ONE decode site of a `MediaHandlerSet` (ADR-0039 §4, §5), for the
// join-time set (`JoinResponse.media_handler_set`) and every pushed set
// (`ServerMessage.media_handler_set`) alike.
//
// The set is UNTRUSTED wire input and carries a CREDENTIAL on every member
// (`media_dial_token`). So, in this order:
//   0. An ABSENT join-time set is an EMPTY set, silently (proto rule).
//   1. VALIDATE, entry by entry. Every violation is a non-fatal contract fault
//      (one bounded event-once `[dt-signaling-contract]` WARN per message kind,
//      at the caller) and the offending entries are DROPPED, so they are
//      non-members — never dialed, never silently redirected:
//        - more than MAX_HANDLER_SET_SIZE entries (the excess);
//        - handler_ref 0 (never a member, proto);
//        - an empty url, or one over MAX_MH_URL_BYTES (it would be dialed
//          truncated);
//        - an empty token, or one over MAX_MEDIA_DIAL_TOKEN_LENGTH;
//        - a ref or a url appearing more than once: EVERY entry involved is
//          dropped, so no wire order picks a winner (a token the server meant
//          for one url never rides to another);
//        - a ref already known under a DIFFERENT url (the address of an
//          incarnation never changes, ADR-0039 §7): dropped, so the ref leaves
//          the set and is removed rather than redirected.
//   2. PROJECT into the internal `HandlerSetIntake` — a token-free public view
//      plus a ref -> token map — which goes ONLY to the private transport sink.
//   3. BLANK every `media_dial_token` on the decoded message, immediately and in
//      a `finally`, so no later stringify, clone or interpolation of the decoded
//      `ServerMessage` can print a token (the `takeMeetingKek` pattern).
//
// The token is OPAQUE: nothing here (or anywhere in the SDK) splits, decodes or
// parses it.

import { handlerRef, type HandlerRef, type HandlerSetIntake } from '../media/handlerSet.js';
import {
  MAX_HANDLER_SET_SIZE,
  MAX_MEDIA_DIAL_TOKEN_LENGTH,
  MAX_MH_URL_BYTES,
} from '../validation/limits.js';

/** The decoded wire shape, structurally (generated `MediaHandlerSet`). */
interface WireHandlerSet {
  handlerSetVersion: bigint;
  handlers: {
    mediaHandlerUrl: string;
    handlerRef: number;
    mediaDialToken: string;
  }[];
}

/**
 * The closed vocabulary of handler-set contract faults. Static literals: they
 * are the event-once dedupe key and the WARN text, and carry no ref, url or
 * token.
 */
export const HANDLER_SET_FAULTS = {
  Oversized: 'a media handler set exceeded the client set-size bound',
  ZeroRef: 'a media handler set entry carried handler_ref 0',
  BadUrl: 'a media handler set entry carried an empty or oversized url',
  BadToken: 'a media handler set entry carried an empty or oversized dial token',
  DuplicateRef: 'a media handler set named one handler_ref more than once',
  DuplicateUrl: 'a media handler set named one url more than once',
  UrlChanged: 'a media handler set moved a known handler_ref to a different url',
} as const;

export type HandlerSetFault = (typeof HANDLER_SET_FAULTS)[keyof typeof HANDLER_SET_FAULTS];

/** What `takeHandlerSet` needs to know about the previous set. */
export type KnownHandlerUrls = ReadonlyMap<HandlerRef, string>;

const UTF8 = /*@__PURE__*/ new TextEncoder();

/**
 * Validate, project and blank one decoded set. See the module header.
 *
 * @param wire the decoded message's set, or `undefined` when a join response
 *   carried none (treated as EMPTY, never a fault — proto rule).
 * @param known ref -> url of the previous set (empty for the join-time set).
 * @param fault reports one contract fault; the caller dedupes and WARNs.
 */
export function takeHandlerSet(
  wire: WireHandlerSet | undefined,
  known: KnownHandlerUrls,
  fault: (reason: HandlerSetFault) => void,
): HandlerSetIntake {
  // signaling.proto `JoinResponse.media_handler_set`: an ABSENT set is treated
  // exactly as an empty one ("media connecting"), NEVER as a fault.
  if (wire === undefined) {
    return { view: { version: '0', handlers: [] }, version: 0n, dialTokens: new Map() };
  }
  try {
    let entries = wire.handlers;
    if (entries.length > MAX_HANDLER_SET_SIZE) {
      fault(HANDLER_SET_FAULTS.Oversized);
      entries = entries.slice(0, MAX_HANDLER_SET_SIZE);
    }
    const shaped = entries.filter((e) => {
      if (e.handlerRef === 0) {
        fault(HANDLER_SET_FAULTS.ZeroRef);
        return false;
      }
      if (e.mediaHandlerUrl === '' || UTF8.encode(e.mediaHandlerUrl).length > MAX_MH_URL_BYTES) {
        fault(HANDLER_SET_FAULTS.BadUrl);
        return false;
      }
      if (e.mediaDialToken === '' || e.mediaDialToken.length > MAX_MEDIA_DIAL_TOKEN_LENGTH) {
        fault(HANDLER_SET_FAULTS.BadToken);
        return false;
      }
      return true;
    });
    const refCount = countBy(shaped.map((e) => e.handlerRef));
    const urlCount = countBy(shaped.map((e) => e.mediaHandlerUrl));
    const handlers: { ref: HandlerRef; url: string }[] = [];
    const dialTokens = new Map<HandlerRef, string>();
    for (const e of shaped) {
      if ((refCount.get(e.handlerRef) ?? 0) > 1) {
        fault(HANDLER_SET_FAULTS.DuplicateRef);
        continue;
      }
      if ((urlCount.get(e.mediaHandlerUrl) ?? 0) > 1) {
        fault(HANDLER_SET_FAULTS.DuplicateUrl);
        continue;
      }
      const ref = handlerRef(e.handlerRef);
      const knownUrl = known.get(ref);
      if (knownUrl !== undefined && knownUrl !== e.mediaHandlerUrl) {
        fault(HANDLER_SET_FAULTS.UrlChanged);
        continue;
      }
      handlers.push({ ref, url: e.mediaHandlerUrl });
      dialTokens.set(ref, e.mediaDialToken);
    }
    const version = wire.handlerSetVersion;
    return { view: { version: version.toString(), handlers }, version, dialTokens };
  } finally {
    // Blank EVERY token on the decoded message — dropped entries included: a
    // malformed token is still credential-shaped bytes that would print.
    for (const e of wire.handlers) e.mediaDialToken = '';
  }
}

function countBy<K>(keys: readonly K[]): Map<K, number> {
  const counts = new Map<K, number>();
  for (const k of keys) counts.set(k, (counts.get(k) ?? 0) + 1);
  return counts;
}
