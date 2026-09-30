// File: packages/sdk-core/src/session/mediaWiring.ts
//
// The two pieces of session-level wiring between the key-material seams and the
// media pipeline, in one module so the session and its integration tests share
// the SAME code. A test that re-implemented this wiring would prove its own copy
// rather than the product's.

import type { KekHolderObserver } from '../media/setup/kekSource.js';
import type { AudioPipeline, AudioSendDirective } from '../media/lifecycle/AudioPipeline.js';
import type { SendDirectiveEvent } from '../signaling/events.js';
import { MEDIA_ROSTER_KEY_CHANGES, type MediaMetrics } from '../media/setup/mediaMetrics.js';
import type { TransmitKeyInvalidationListener } from '../media/setup/rosterKeys.js';

/**
 * Route the KEK holder's reports to the media metric handles and the console.
 *
 * The WARN line carries DURATIONS ONLY — `kekSource.ts` builds every message
 * from the received W, the derived retention and T, never from key bytes, a
 * generation or a sender id. It reaches only the browser console; the counters
 * are what an operator can see, which is why every warned condition is also
 * counted.
 */
export function kekObserver(
  metrics: MediaMetrics,
  warn: (message: string) => void = (message) => console.warn('[dt-media-kek]', message),
): KekHolderObserver {
  return {
    kekArrived: (source) => metrics.kekUpdate(source),
    retentionAnomaly: (outcome) => metrics.kekRetentionAnomaly(outcome),
    installRefused: (outcome) => metrics.kekInstallRefused(outcome),
    generationRetained: () => metrics.kekGenerationRetained(),
    retentionViolation: () => metrics.kekRetentionViolation(),
    warn,
  };
}

/** The pipeline operation a roster invalidation drives. */
export interface TransmitKeyPurger {
  purgeTransmitKeys(senderId: number): void;
}

/**
 * Story 2 R-18: a roster change to a sender's key purges ITS cached transmit
 * keys — never its replay state, which no path here can reach.
 *
 * A REBIND or a DOWNGRADE is counted (separately); a FORGET (a leave, or LRU
 * eviction) is memory hygiene and is not. Both getters are read at event time rather than captured, because the
 * roster is wired at signaling setup, before media starts: a rebind before then
 * is still counted, and simply has no transmit-key cache to purge.
 */
export function rosterInvalidationListener(
  metrics: () => MediaMetrics | undefined,
  pipeline: () => TransmitKeyPurger | undefined,
): TransmitKeyInvalidationListener {
  return (senderId, cause) => {
    if (cause === 'rebind') metrics()?.rosterKeyRebind(MEDIA_ROSTER_KEY_CHANGES.Rebind);
    if (cause === 'downgrade') metrics()?.rosterKeyRebind(MEDIA_ROSTER_KEY_CHANGES.Downgrade);
    pipeline()?.purgeTransmitKeys(senderId);
  };
}

/**
 * The session's `sendDirective` listener: MC's instruction for what this client
 * SENDS, applied to the pipeline. Holds the last audio instruction so a
 * withdrawal can be applied to it.
 *
 * Moved here from `MeetingSession.startMedia` verbatim (story 2 task 13, to keep
 * the session class inside `dt-guard`'s declaration-size bound); the rule it
 * carries is unchanged.
 */
export function sendDirectiveListener(
  pipeline: Pick<AudioPipeline, 'setSendDirective'>,
  defaultBitrateBps: number,
): (directive: SendDirectiveEvent) => void {
  let lastAudio: AudioSendDirective | undefined;
  return (directive) => {
    const audio = directive.streams.find((s) => s.mediaKind === 'audio');
    if (!audio) {
      // A directive with NO audio stream is MC directing this client to
      // produce no audio (`SendDirective`: "What MC directs this client to
      // produce"). MC sends exactly this — `streams: []` — to a publisher
      // nobody holds: a solo participant, or one whose last holder left.
      //
      // It must NOT be ignored, and the reason is FORWARD SECRECY, not tidy
      // bookkeeping — do not "simplify" this back to an early return.
      //
      // Ignoring it kept the STALE instruction, so the pipeline's target set
      // never reached zero. `AudioPipeline.setSendDirective` rotates the
      // transmit key on the empty -> non-empty EDGE, and resume-from-empty is
      // one of only four `rotate()` call sites (see `lifecycle/transmitKeys.ts`,
      // "the bound is the interval between two consecutive rotations from any
      // trigger"). Never seeing "empty" made that call site unreachable on this
      // path. So a publisher whose last holder left, then picked up by a NEW
      // holder, resumed under the SAME transmit key generation — which the
      // DEPARTED holder still has, and can therefore decrypt the resumed media
      // with. That is the window ADR-0036 §4 rotation exists to close.
      //
      // Applied as an empty target set, which is exactly §5's "send nothing" —
      // so MC's two spellings of it (`streams: []`, and a stream with empty
      // targets) land identically and both arm the resume rotation. A reader
      // must not have to know which spelling MC happens to emit.
      //
      // With no earlier instruction there is nothing to withdraw: egress was
      // never started, and no placeholder stream number or bitrate is invented.
      if (lastAudio && lastAudio.targets.length > 0) {
        lastAudio = { ...lastAudio, targets: [] };
        pipeline.setSendDirective(lastAudio);
      }
      return;
    }
    lastAudio = {
      streamNumber: audio.streamNumber,
      // MC's directed value when present; the configured DEFAULT otherwise.
      // Never a local ceiling applied on top — see `clientConfig.ts`.
      bitrateBps: audio.maxBitrateBps ?? defaultBitrateBps,
      targets: audio.targets,
    };
    pipeline.setSendDirective(lastAudio);
  };
}
