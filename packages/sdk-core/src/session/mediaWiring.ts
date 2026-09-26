// File: packages/sdk-core/src/session/mediaWiring.ts
//
// The two pieces of session-level wiring between the key-material seams and the
// media pipeline, in one module so the session and its integration tests share
// the SAME code. A test that re-implemented this wiring would prove its own copy
// rather than the product's.

import type { KekHolderObserver } from '../media/setup/kekSource.js';
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
