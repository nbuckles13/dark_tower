// File: packages/sdk-core/src/media/lifecycle/cameraGate.ts
//
// THE camera-light rule and the R-19 failure latch, as one PURE state machine
// (story 3 R-16, R-17, R-19). `VideoPublisher` feeds it events and performs the
// actions it returns; nothing here touches a device.
//
// ---------------------------------------------------------------------------
// THE PREDICATE
// ---------------------------------------------------------------------------
//
//   camera light ON  <=>  directed with NON-EMPTY targets
//                         AND NOT client-muted
//                         AND the camera is healthy (no latched failure)
//
// Every video withdrawal arrives as EMPTY targets (unwatched, or server-muted:
// MC withdraws the directive), so both turn the camera OFF; client video mute
// turns it OFF. The microphone is not this module's (audio mute keeps it open).
//
// ---------------------------------------------------------------------------
// RE-ACQUIRE ONLY ON AN EDGE, NEVER IN A LOOP (R-19, security S1)
// ---------------------------------------------------------------------------
//
// A track that ends, or an acquire that fails, LATCHES a failure: the
// participant is presented as video client-muted and nothing retries on a
// timer. Only two edges try again:
//
//   * a USER unmute (always);
//   * the next directive RESTORE edge (empty -> non-empty targets) — but NOT
//     after `permission_denied` / `platform_unsupported`. Directive edges are
//     driven remotely by other participants watching and unwatching, so
//     retrying a denied permission on them would turn watcher churn into a
//     stream of permission prompts.
//
// A restore retry ACQUIRES FIRST and lifts the failure-mute only once a live
// track exists — never presents as unmuted before the camera is actually on.
// A user mute clears the latch (user intent wins: a later restore does not
// unmute them).

import type { MediaCaptureFailureOutcome } from '../setup/mediaMetrics.js';

/** Failure outcomes after which ONLY a user unmute may retry. */
const USER_ONLY_RETRY: ReadonlySet<MediaCaptureFailureOutcome> = new Set([
  'permission_denied',
  'platform_unsupported',
]);

/** The gate's state. Immutable; every transition returns a new value. */
export interface CameraGateState {
  /** Directed with non-empty targets. */
  readonly directed: boolean;
  /** The presented client video mute (user intent, or a latched failure). */
  readonly muted: boolean;
  /** A latched capture failure, if any. */
  readonly failure: { readonly outcome: MediaCaptureFailureOutcome } | undefined;
  /** A restore retry is acquiring while still presented as failure-muted. */
  readonly retrying: boolean;
  /** Capture is running or being acquired. */
  readonly live: boolean;
}

/** What the publisher must do after a transition. */
export interface CameraGateActions {
  /** Acquire (start) the camera. */
  readonly acquire: boolean;
  /** Stop the camera (light off). */
  readonly stop: boolean;
  /** Change the presented client video mute (and report it), or leave it. */
  readonly presentMuted: boolean | undefined;
}

/** An input to the gate. */
export type CameraGateEvent =
  | { readonly type: 'directive'; readonly nonEmpty: boolean }
  | { readonly type: 'userMute'; readonly muted: boolean }
  | { readonly type: 'acquired' }
  | { readonly type: 'failed'; readonly outcome: MediaCaptureFailureOutcome };

/** The initial state: undirected, unmuted, healthy, off. */
export function initialCameraGate(muted = false): CameraGateState {
  return { directed: false, muted, failure: undefined, retrying: false, live: false };
}

/** Whether the camera light should be on. */
export function cameraWanted(s: CameraGateState): boolean {
  if (!s.directed) return false;
  if (s.retrying) return true;
  return !s.muted && s.failure === undefined;
}

/** Apply one event. Pure. */
export function stepCameraGate(
  state: CameraGateState,
  event: CameraGateEvent,
): { readonly state: CameraGateState; readonly actions: CameraGateActions } {
  let next: CameraGateState;
  let presentMuted: boolean | undefined;
  switch (event.type) {
    case 'directive': {
      const restore = !state.directed && event.nonEmpty;
      next = { ...state, directed: event.nonEmpty };
      if (!event.nonEmpty) next = { ...next, retrying: false };
      if (
        restore &&
        state.failure !== undefined &&
        !USER_ONLY_RETRY.has(state.failure.outcome) &&
        !state.live
      ) {
        next = { ...next, retrying: true };
      }
      break;
    }
    case 'userMute': {
      // User intent wins either way and clears the latch.
      next = { ...state, muted: event.muted, failure: undefined, retrying: false };
      break;
    }
    case 'acquired': {
      next = { ...state, live: true };
      if (state.retrying) {
        next = { ...next, retrying: false, failure: undefined, muted: false };
        presentMuted = false;
      }
      break;
    }
    case 'failed': {
      next = { ...state, live: false, retrying: false, failure: { outcome: event.outcome } };
      if (!state.muted) {
        next = { ...next, muted: true };
        presentMuted = true;
      }
      break;
    }
    default: {
      const exhaustive: never = event;
      return exhaustive;
    }
  }
  const wanted = cameraWanted(next);
  const acquire = wanted && !next.live;
  // A failure already stopped the track (`live` false); stop is for a running one.
  const stop = !wanted && state.live && next.live;
  if (acquire) next = { ...next, live: true };
  if (stop) next = { ...next, live: false };
  return { state: next, actions: { acquire, stop, presentMuted } };
}
