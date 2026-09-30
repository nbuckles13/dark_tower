// File: packages/web-app/src/lib/testLevers.ts
//
// Story 2 task 15: the per-BROWSING-CONTEXT test levers, behind the opt-in build
// define `__DT_TEST_LEVERS__` (`DT_TEST_LEVERS=1`; THROWS in production —
// `vite/testDefines.ts`). One dev server serves every Playwright context, so a
// build define can only switch the levers ON; which context gets what comes from
// ONE init-script global, `window.__dt_test_levers__`, read at ONE site
// (`readTestLevers`, inside the gate) ONCE at session build into a validated,
// frozen copy. Nothing keeps a live reference, so later page script cannot
// change a lever mid-session.
//
// The levers:
//   * `blockHandlers` — S10a partial connectivity. Exact-match media-handler URLs
//     this context REFUSES to dial. It can only NARROW: it wraps the injected
//     WebTransport `connect`, and every dial target still comes from MC's
//     `JoinResponse.media_servers` through the SDK's one dial site. A lever string
//     is never passed to `connect`, so it cannot become a dial target (and the MH
//     join token is only written after a connect succeeds). A refused dial reports
//     `failed`, exactly as an unreachable handler does — MC derives visibility
//     from connections the handlers OBSERVE, so this is the real-world cause.
//   * `receiveSlots` — S2's per-context N, parsed by the SDK's own strict
//     `parseReceiveSlots`. It does not bypass MC's cap: the SDK's loud refusal
//     above the advertised cap applies unchanged.
//   * `forceHostControls` — renders the host server-mute affordance in a NON-host
//     context, so the browser suite exercises MC's refusal (the authority), not a
//     hidden button (@security M2).
//
// LOUD ON MISUSE (@security L3): an unknown key, a wrong type or an empty entry
// THROWS at session build; after join, `checkLeversApplied` refuses a block list
// naming a handler MC did not offer, or one leaving no offered handler, so a
// mistyped lever can never pass S10a vacuously as "connected to everything".

import { parseReceiveSlots } from '@darktower/sdk-core';
import type { ParsedReceiveSlots, WebTransportConnectFn } from '@darktower/sdk-core';

/** The validated, frozen levers for this browsing context. */
export interface TestLevers {
  readonly blockHandlers: readonly string[];
  readonly receiveSlots: ParsedReceiveSlots | undefined;
  readonly forceHostControls: boolean;
}

/** The only keys the init-script global may carry. */
const LEVER_KEYS: readonly string[] = ['blockHandlers', 'receiveSlots', 'forceHostControls'];

/** Prefix on every lever error, so a failure names its cause. */
const LEVER_ERROR = 'test lever:';

/**
 * Validate the init-script value. `undefined` (no global set) means "no levers":
 * the context behaves exactly as a production one. Anything else must be a plain
 * object carrying only known keys of the right type, or this THROWS.
 */
export function parseTestLevers(raw: unknown): TestLevers | undefined {
  if (raw === undefined) return undefined;
  if (typeof raw !== 'object' || raw === null || Array.isArray(raw)) {
    throw new Error(`${LEVER_ERROR} __dt_test_levers__ must be an object`);
  }
  const record = raw as Record<string, unknown>;
  for (const key of Object.keys(record)) {
    if (!LEVER_KEYS.includes(key)) {
      throw new Error(`${LEVER_ERROR} unknown key ${JSON.stringify(key.slice(0, 32))}`);
    }
  }
  const block = record['blockHandlers'] ?? [];
  if (!Array.isArray(block) || !block.every((u) => typeof u === 'string' && u !== '')) {
    throw new Error(`${LEVER_ERROR} blockHandlers must be an array of non-empty strings`);
  }
  const slots = record['receiveSlots'];
  if (slots !== undefined && typeof slots !== 'string') {
    throw new Error(
      `${LEVER_ERROR} receiveSlots must be a string (the VITE_DT_RECEIVE_SLOTS form)`,
    );
  }
  const force = record['forceHostControls'] ?? false;
  if (typeof force !== 'boolean') {
    throw new Error(`${LEVER_ERROR} forceHostControls must be a boolean`);
  }
  return Object.freeze({
    blockHandlers: Object.freeze([...(block as string[])]),
    receiveSlots: slots === undefined ? undefined : parseReceiveSlots(slots),
    forceHostControls: force,
  });
}

/**
 * THE one read of the init-script global. In any build without
 * `DT_TEST_LEVERS=1` the branch is statically dead and this returns `undefined`.
 */
export function readTestLevers(): TestLevers | undefined {
  if (__DT_TEST_LEVERS__) {
    return parseTestLevers((window as { __dt_test_levers__?: unknown }).__dt_test_levers__);
  }
  return undefined;
}

/**
 * Wrap `connect` so an exact-match blocked URL is REFUSED. Never supplies a URL:
 * `url` always comes from the SDK's own dial site.
 */
export function blockingConnect(
  connect: WebTransportConnectFn,
  levers: TestLevers,
): WebTransportConnectFn {
  if (levers.blockHandlers.length === 0) return connect;
  return (url, options) => {
    if (levers.blockHandlers.includes(url)) {
      throw new Error(`${LEVER_ERROR} dial refused by blockHandlers`);
    }
    return connect(url, options);
  };
}

/**
 * After join: every blocked URL must be one MC offered, and at least one offered
 * handler must remain. Returns the violation text, or `undefined` when applied.
 * The caller disconnects on a violation — never a silent connect-all/none.
 */
export function checkLeversApplied(
  levers: TestLevers,
  offered: readonly string[],
): string | undefined {
  if (levers.blockHandlers.length === 0) return undefined;
  const notOffered = levers.blockHandlers.filter((u) => !offered.includes(u));
  if (notOffered.length > 0) {
    return (
      `${LEVER_ERROR} blockHandlers names ${notOffered.length} handler(s) MC did not offer, ` +
      `so the block was vacuous`
    );
  }
  if (offered.every((u) => levers.blockHandlers.includes(u))) {
    return `${LEVER_ERROR} blockHandlers blocks every offered handler`;
  }
  return undefined;
}

// ----------------------------------------------------------------------------
// The view-facing seam. Each helper tests the define ITSELF, so a component
// never names the define and every lever branch stays statically dead — and
// tree-shaken — in a build without `DT_TEST_LEVERS=1`.
// ----------------------------------------------------------------------------

/**
 * Read this context's levers for a new session. A malformed lever is returned
 * as `error` (the view refuses to join and shows it), never a silent fallback.
 */
export function readTestLeversForSession(): {
  readonly levers: TestLevers | undefined;
  readonly error: string | undefined;
} {
  if (__DT_TEST_LEVERS__) {
    try {
      return { levers: readTestLevers(), error: undefined };
    } catch (err) {
      return { levers: undefined, error: err instanceof Error ? err.message : String(err) };
    }
  }
  return { levers: undefined, error: undefined };
}

/** After join: the lever violation to refuse the session with, if any. */
export function leverViolationAfterJoin(
  levers: TestLevers | undefined,
  offered: readonly string[],
): string | undefined {
  if (__DT_TEST_LEVERS__ && levers) return checkLeversApplied(levers, offered);
  return undefined;
}

/** Whether host controls are FORCED on for this (non-host) context. */
export function hostControlsForced(levers: TestLevers | undefined): boolean {
  return __DT_TEST_LEVERS__ && levers !== undefined && levers.forceHostControls;
}
