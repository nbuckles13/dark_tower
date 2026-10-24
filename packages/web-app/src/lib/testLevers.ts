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
//   * `audioReceiveSlots` / `videoReceiveSlots` — S2's per-context N and M (the
//     `VITE_DT_AUDIO_RECEIVE_SLOTS` / `VITE_DT_VIDEO_RECEIVE_SLOTS` form), parsed by
//     the SDK's own strict `parseReceiveSlots`. They do not bypass MC's caps: MC
//     truncates an over-cap declaration exactly as for a configured one.
//   * `forceHostControls` — renders the host server-mute affordance in a NON-host
//     context, so the browser suite exercises MC's refusal (the authority), not a
//     hidden button (@security M2).
//   * `publish` — SEEDS the join view's publish-set chooser (story 3 C-A2/C-C1);
//     the chooser's result is still what the join uses.
//   * `forceCameraCapture` — story 3 C-C1: this context captures from the REAL
//     camera (Chromium's fake device under the suite's launch flags) instead of
//     the build's test pattern, so a spec can assert the real track's
//     `readyState`. It can only swap the capture source; nothing else.
//
// LOUD ON MISUSE (@security L3): an unknown key, a wrong type or an empty entry
// THROWS at session build; after join, `checkLeversApplied` refuses a block list
// naming a handler MC did not offer, or one leaving no offered handler, so a
// mistyped lever can never pass S10a vacuously as "connected to everything".

import { parseReceiveSlots } from '@darktower/sdk-core';
import type { ParsedReceiveSlots, PublishKind, WebTransportConnectFn } from '@darktower/sdk-core';

/** The validated, frozen levers for this browsing context. */
export interface TestLevers {
  readonly blockHandlers: readonly string[];
  readonly audioReceiveSlots: ParsedReceiveSlots | undefined;
  readonly videoReceiveSlots: ParsedReceiveSlots | undefined;
  readonly forceHostControls: boolean;
  /**
   * Story 3 C-A2: this context's publish set, from the e2e cohort contract's
   * `audio | video | both | none` (`e2e/cohort.ts`). `undefined`: the app
   * default (audio). Frozen at session build like every lever.
   */
  readonly publish: readonly PublishKind[] | undefined;
  /** Story 3 C-C1: capture from the real camera, not the test pattern. */
  readonly forceCameraCapture: boolean;
}

/** The cohort contract's publish-set spellings -> the SDK's publish set. */
const PUBLISH_SETS: Readonly<Record<string, readonly PublishKind[]>> = {
  audio: ['audio'],
  video: ['video'],
  both: ['audio', 'video'],
  none: [],
};

/** The only keys the init-script global may carry. */
const LEVER_KEYS: readonly string[] = [
  'blockHandlers',
  'audioReceiveSlots',
  'videoReceiveSlots',
  'forceHostControls',
  'publish',
  'forceCameraCapture',
];

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
  const audioSlots = record['audioReceiveSlots'];
  if (audioSlots !== undefined && typeof audioSlots !== 'string') {
    throw new Error(
      `${LEVER_ERROR} audioReceiveSlots must be a string (the VITE_DT_AUDIO_RECEIVE_SLOTS form)`,
    );
  }
  const videoSlots = record['videoReceiveSlots'];
  if (videoSlots !== undefined && typeof videoSlots !== 'string') {
    throw new Error(
      `${LEVER_ERROR} videoReceiveSlots must be a string (the VITE_DT_VIDEO_RECEIVE_SLOTS form)`,
    );
  }
  const force = record['forceHostControls'] ?? false;
  if (typeof force !== 'boolean') {
    throw new Error(`${LEVER_ERROR} forceHostControls must be a boolean`);
  }
  const forceCamera = record['forceCameraCapture'] ?? false;
  if (typeof forceCamera !== 'boolean') {
    throw new Error(`${LEVER_ERROR} forceCameraCapture must be a boolean`);
  }
  const publish = record['publish'];
  if (
    publish !== undefined &&
    (typeof publish !== 'string' || !Object.hasOwn(PUBLISH_SETS, publish))
  ) {
    throw new Error(`${LEVER_ERROR} publish must be one of audio, video, both, none`);
  }
  return Object.freeze({
    blockHandlers: Object.freeze([...(block as string[])]),
    publish: publish === undefined ? undefined : Object.freeze([...PUBLISH_SETS[publish]!]),
    audioReceiveSlots:
      audioSlots === undefined ? undefined : parseReceiveSlots(audioSlots, 'audio'),
    videoReceiveSlots:
      videoSlots === undefined ? undefined : parseReceiveSlots(videoSlots, 'video'),
    forceHostControls: force,
    forceCameraCapture: forceCamera,
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

/** The publish set this context's lever SEEDS the chooser with, if any (test-levers build only). */
export function publishSeed(levers: TestLevers | undefined): readonly PublishKind[] | undefined {
  return __DT_TEST_LEVERS__ ? levers?.publish : undefined;
}
