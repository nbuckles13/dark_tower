// File: packages/sdk-core/src/media/setup/kekSource.ts
//
// THE KEK-SOURCE SEAM (ADR-0036 §4).
//
// "The client obtains the KEK through a seam, as §10's transport and measurement
// seams; today its one implementation is the join response and the KEK-push
// message. Nothing in the frame format or the wrap depends on where the KEK came
// from, so a key server outside MC — should operator exclusion ever be required
// — changes the source and nothing else."
//
// ---------------------------------------------------------------------------
// WHERE THE KEK LIVES, AND WHERE IT DOES NOT
// ---------------------------------------------------------------------------
//
// It lives in private fields on ONE object, for the life of one meeting. It is
// NOT:
//
//   * on `JoinedEvent` or any other public event payload — those are projected
//     to embedders and, in test builds, onto the web app's e2e bus;
//   * on `TransmitKeyCache` — the codec layer deliberately keeps the KEK a
//     per-call parameter to `unwrapTransmitKey` so that cache never becomes a
//     KEK home, and that property is preserved here;
//   * on any error, log, span attribute, or metric label;
//   * persisted anywhere.
//
// ---------------------------------------------------------------------------
// WHAT THIS SEAM DOES NOT CLAIM
// ---------------------------------------------------------------------------
//
// ADR-0036 §4 records the trust decision in its own words: media is encrypted
// between clients; MH, transport and storage cannot read it; MC can. This is
// accepted operator custody. Nothing here may be described as end-to-end against
// the operator, or as zero-trust; telemetry carries `key_custody=operator` in
// place of any such boolean.
//
// ---------------------------------------------------------------------------
// CURRENT PLUS AT MOST ONE PREVIOUS GENERATION (story 2, R-14)
// ---------------------------------------------------------------------------
//
// When MC rotates the KEK, frames already in flight were wrapped under the old
// one. So a superseded generation is RETAINED — exactly one of them, for a
// window derived from W (`kek_rotation_debounce_seconds`, which MC carries on
// every KEK message). The rule for that window has ONE home, the
// `JoinResponse.kek_rotation_debounce_seconds` field comment in
// `signaling.proto`; it is applied by `deriveKekRetention` in
// `config/clientConfig.ts` and is not restated here.
//
// A frame under the previous generation opens while it is retained. A frame
// under any other generation the receiver does not hold is dropped, split by
// DIRECTION — newer than the current is `no_kek_for_generation` (MC's push has
// not landed), older than retention keeps is `kek_generation_stale`. That split
// is answered by `isOlderThanRetained` below and applied in
// `frame/receivePath.ts`.
//
// ---------------------------------------------------------------------------
// THE SEND SIDE CAN ONLY REACH THE CURRENT GENERATION
// ---------------------------------------------------------------------------
//
// `currentForWrapping` returns the current KEK and nothing else. There is
// deliberately no send-side path that takes a generation: a sender able to wrap
// under a retained PREVIOUS KEK would keep wrapping under the one a departed
// member still holds, which is precisely what R-13's rotation-on-KEK-receipt
// exists to end.

import { MEETING_KEK_BYTES } from '../frame/sframe.js';
import { deriveKekRetention, type KekRetentionOutcome } from '../../config/clientConfig.js';
import type {
  MediaKekInstallRefusal,
  MediaKekRetentionAnomaly,
  MediaKekSource,
} from './mediaMetrics.js';

/**
 * The largest generation the wire can carry.
 *
 * `kek_generation` is a `uint32` on the proto but has u16 semantics (the frame's
 * wrapped-key block carries it in two bytes), so a value above this could never
 * be announced by a frame and is refused at the boundary. Generations are
 * compared as plain integers: MC never wraps a meeting's generation.
 */
export const MAX_KEK_GENERATION = 0xffff;

/**
 * Read side of the seam, as the receive path consumes it.
 *
 * Deliberately the exact shape `openVerifiedFrame`'s `ReceiverKeys` needs, so no
 * adapter sits between the seam and the crypto.
 */
export interface MeetingKekSource {
  /** The KEK for `generation`, or `undefined` if this client does not hold it. */
  kekForGeneration(generation: number): Uint8Array | undefined;
  /**
   * Whether `generation` is OLDER than anything this client still retains.
   *
   * `true` for a generation below the current that is not the retained previous
   * (including a previous whose retention has expired, and a gap between the
   * two). `false` for a held generation, for one newer than the current, and
   * when nothing is held at all — those are `no_kek_for_generation`.
   */
  isOlderThanRetained(generation: number): boolean;
}

/** The current KEK, for wrapping. Never a retained previous generation. */
export interface KekForWrapping {
  readonly kek: Uint8Array;
  readonly generation: number;
}

/** Send side of the seam. */
export interface KekWrapSource {
  /** The CURRENT KEK and its generation, or `undefined` before one arrives. */
  currentForWrapping(): KekForWrapping | undefined;
}

/**
 * Where a KEK arrived from. DERIVED from the metric's bounded `source`
 * vocabulary, never retyped: `mediaMetrics.ts`'s `MEDIA_KEK_SOURCES` is the one
 * home, so an added source cannot land in one copy and not the other.
 */
export type KekArrival = MediaKekSource;

/** What an install did. Never the key, never its generation. */
export type KekInstallResult =
  /** The first KEK this session held. Nothing to retain. */
  | 'installed'
  /** A newer generation became current; the old current is retained. */
  | 'installed_retaining'
  /** The same generation with the same bytes was redelivered. A no-op. */
  | 'already_held'
  /** Refused; see {@link MediaKekInstallRefusal}. The held state is unchanged. */
  | 'refused';

/**
 * Everything the holder reports. Injected by the session so this module names no
 * metric (only `mediaMetrics.ts` may, under `media/**`) and holds no logger.
 */
export interface KekHolderObserver {
  /** A KEK message carrying key material arrived. */
  kekArrived(source: KekArrival): void;
  /** That message's W produced a non-nominal retention window. */
  retentionAnomaly(outcome: MediaKekRetentionAnomaly): void;
  /** A delivered KEK was refused. */
  installRefused(outcome: MediaKekInstallRefusal): void;
  /** A previous generation was demoted and retained. */
  generationRetained(): void;
  /** The retention bound was exceeded (a tripwire; see `enforceRetentionBound`). */
  retentionViolation(): void;
  /**
   * A loud, bounded condition. The message carries DURATIONS ONLY — never key
   * bytes, never a generation, never a sender id.
   */
  warn(message: string): void;
}

/** One held generation. Newest first in the holder's list. */
export interface HeldKek {
  readonly generation: number;
  readonly kek: Uint8Array;
  /** When this generation stops opening frames. Absent for the current one. */
  readonly expiresAtMs?: number;
}

/** The at-most-one-previous bound: the current generation plus one. */
const MAX_HELD_GENERATIONS = 2;

/**
 * The at-most-one-previous-generation TRIPWIRE.
 *
 * Should never fire: every production path keeps the held list at two entries
 * or fewer by construction. It watches for the refactor that stops doing so —
 * the change most likely to ALSO delete the call to it. If the bound is ever
 * exceeded it reports, then FAILS SAFE: the excess generations are zeroized and
 * dropped, so an over-retaining defect degrades to "the invariant holds" rather
 * than to a superseded KEK lingering in memory.
 *
 * ---------------------------------------------------------------------------
 * WHY THE COUNT AND THE CHECK ARE ONE FUNCTION
 * ---------------------------------------------------------------------------
 *
 * `evaluations` is what proves the check runs on every live install. It is
 * incremented INSIDE `evaluate`, in the same function as the enforcement,
 * deliberately. An earlier draft counted beside the call instead, and deleting
 * only the enforcement call left the count advancing and the proving test
 * green — the exact silent removal the count exists to catch. Fused, the two
 * pinned halves cover each other: delete the CALL SITE and `evaluations` stops
 * advancing (the live-path test reds); gut the ENFORCEMENT here and the
 * fabricated-state test reds, because it goes through this same `evaluate`.
 * Do not split them back apart.
 *
 * Exported so the unit test can hand it a fabricated over-retained list — it
 * operates only on the list it is given and is not a door into any holder. Not
 * exported from the package.
 */
export class RetentionGuard {
  readonly #onViolation: () => void;
  #evaluations = 0;

  constructor(onViolation: () => void) {
    this.#onViolation = onViolation;
  }

  /** Times the bound has been evaluated. Monotone. */
  get evaluations(): number {
    return this.#evaluations;
  }

  /** Check the bound on `held` and fail safe if it is exceeded. */
  evaluate(held: HeldKek[]): void {
    this.#evaluations += 1;
    if (held.length <= MAX_HELD_GENERATIONS) return;
    this.#onViolation();
    for (const excess of held.splice(MAX_HELD_GENERATIONS)) excess.kek.fill(0);
  }
}

/**
 * Compare two KEKs in time independent of where they differ.
 *
 * Both sides are secret, so an early-exit comparison would leak the length of
 * the matching prefix. Lengths are checked first; KEKs are fixed-width, so a
 * length mismatch reveals nothing about either key.
 */
function sameKeyFixedTime(a: Uint8Array, b: Uint8Array): boolean {
  if (a.length !== b.length) return false;
  let diff = 0;
  for (let i = 0; i < a.length; i += 1) diff |= (a[i] ?? 0) ^ (b[i] ?? 0);
  return diff === 0;
}

/** Construction options for {@link MeetingKekHolder}. */
export interface MeetingKekHolderOptions {
  /**
   * T: this client's transmit-key re-wrap latency, from
   * `transmitRewrapLatencyMs(config.media)`. Passed in rather than read from
   * config here so this module stays a leaf of the config it applies.
   */
  readonly rewrapLatencyMs: number;
  readonly observer?: KekHolderObserver;
  readonly clock?: () => number;
  /** Timer seam, so expiry is testable without wall-clock waits. */
  readonly setTimeout?: (fn: () => void, ms: number) => ReturnType<typeof setTimeout>;
  readonly clearTimeout?: (handle: ReturnType<typeof setTimeout>) => void;
}

const ANOMALY_WARNINGS: Record<
  MediaKekRetentionAnomaly,
  (w: number, ms: number, t: number) => string
> = {
  floor_substituted: (_w, ms) =>
    `the meeting controller sent no KEK rotation debounce window; retaining the previous KEK ` +
    `for the client floor of ${ms}ms instead. Expected during a one-version controller rollback`,
  ceiling_clamped: (w, ms) =>
    `the KEK rotation debounce window (${w}s) would retain a previous KEK for longer than the ` +
    `client ceiling; retaining for ${ms}ms instead. A configuration state, not a fault`,
  below_rewrap_latency: (w, ms, t) =>
    `the KEK retention derived from the debounce window (${w}s, so ${ms}ms) does not exceed this ` +
    `client's transmit-key re-wrap latency (${t}ms); frames from senders that have not yet ` +
    `re-wrapped will drop at each rotation. Raise the controller's rotation debounce window`,
};

/**
 * The KEK holder: the current generation plus at most one previous, the previous
 * retained for a window derived from W.
 *
 * Constructed empty. `install()` is called by the signaling layer's KEK intake
 * for both the join response and every `MeetingKekUpdate`; `clear()` by teardown.
 */
export class MeetingKekHolder implements MeetingKekSource, KekWrapSource {
  readonly #rewrapLatencyMs: number;
  readonly #clock: () => number;
  readonly #setTimeout: (fn: () => void, ms: number) => ReturnType<typeof setTimeout>;
  readonly #clearTimeout: (handle: ReturnType<typeof setTimeout>) => void;
  #observer: KekHolderObserver | undefined;

  /**
   * Newest first: `[current]` or `[current, previous]`. An ARRAY rather than two
   * fields on purpose — two fields would make the bound a property of the type
   * and `enforceRetentionBound` vacuous, and the bound is exactly what a refactor
   * of this class could break.
   */
  readonly #held: HeldKek[] = [];
  #expiryTimer: ReturnType<typeof setTimeout> | undefined;
  readonly #listeners = new Set<() => void>();
  readonly #retentionGuard = new RetentionGuard(() => this.#observer?.retentionViolation());

  constructor(options: MeetingKekHolderOptions) {
    this.#rewrapLatencyMs = options.rewrapLatencyMs;
    this.#observer = options.observer;
    this.#clock = options.clock ?? Date.now;
    this.#setTimeout = options.setTimeout ?? ((fn, ms) => setTimeout(fn, ms));
    this.#clearTimeout = options.clearTimeout ?? ((h) => clearTimeout(h));
  }

  /**
   * Attach the observer. Separate from construction because the session builds
   * the holder before the media label set (which needs the org) exists.
   */
  setObserver(observer: KekHolderObserver): void {
    this.#observer = observer;
  }

  /** Whether a usable KEK is held. Never exposes the key or its generation. */
  get isProvisioned(): boolean {
    return this.#held.length > 0;
  }

  /**
   * How many times the retention bound has been evaluated. Monotone.
   *
   * Read straight from the guard, whose count is fused with its check (see
   * `RetentionGuard`), so a test can prove the check runs on EVERY live install
   * — demoting, idempotent and refused alike — without an injection seam. A
   * count, not a door.
   */
  get retentionChecks(): number {
    return this.#retentionGuard.evaluations;
  }

  /**
   * Subscribe to "the current generation advanced". Fired AFTER the new KEK is
   * installed, only on a demoting install — never on a redelivery of the same
   * generation, which would otherwise make a reconnect a rotation-storm lever.
   * Returns the unsubscribe.
   */
  onCurrentGenerationChanged(listener: () => void): () => void {
    this.#listeners.add(listener);
    return () => this.#listeners.delete(listener);
  }

  /**
   * Install a delivered KEK.
   *
   * TAKES A COPY. The caller's buffer is a slice of a decoded protobuf message
   * whose bytes the intake path zeroes immediately afterwards.
   *
   * Never throws on wire input: a refusal is counted and reported, and the held
   * state is left exactly as it was.
   *
   * @param debounceSeconds W from THIS message. The retention a demotion arms is
   * derived from the W on the message that caused it, never from a W learned
   * earlier, so a long-lived member follows MC's current configuration.
   */
  install(
    kek: Uint8Array,
    generation: number,
    debounceSeconds: number,
    source: KekArrival,
  ): KekInstallResult {
    try {
      return this.#install(kek, generation, debounceSeconds, source);
    } finally {
      // EVERY path, including the refusals and the idempotent redelivery. This
      // is the live call site the violations tripwire depends on.
      this.#retentionGuard.evaluate(this.#held);
    }
  }

  #install(
    kek: Uint8Array,
    generation: number,
    debounceSeconds: number,
    source: KekArrival,
  ): KekInstallResult {
    this.#observer?.kekArrived(source);
    const retention = deriveKekRetention(debounceSeconds, this.#rewrapLatencyMs);
    this.#reportRetention(debounceSeconds, retention.retentionMs, retention.outcome);

    // FAIL CLOSED at the boundary, on BOTH arrival paths. `signaling.proto`:
    // "never wrap or unwrap under a KEK of any length other than 32; an empty or
    // all-zero KEK is never a usable key". A zero key "works" cryptographically
    // and is catastrophically wrong.
    if (
      kek.length !== MEETING_KEK_BYTES ||
      kek.every((b) => b === 0) ||
      !Number.isInteger(generation) ||
      generation < 0 ||
      generation > MAX_KEK_GENERATION
    ) {
      this.#observer?.installRefused('malformed');
      return 'refused';
    }

    const current = this.#held[0];
    if (!current) {
      this.#held.push({ generation, kek: Uint8Array.from(kek) });
      return 'installed';
    }

    if (generation === current.generation) {
      if (sameKeyFixedTime(kek, current.kek)) {
        // A reconnect redelivering the current generation. NEVER demotes on
        // equal, never re-arms or extends the previous's retention, never
        // rotates transmit keys.
        return 'already_held';
      }
      // Same generation, different bytes: an MC defect. Replacing would break
      // every in-flight wrap; keeping silently would mask it. Keep, and say so.
      //
      // WHY "DEFECT" IS TRUE, AND WHAT KEEPS IT TRUE. One client-side fact
      // carries it: THIS SDK HAS NO RECONNECT. A `MeetingKekHolder` lives for one
      // `MeetingSession`, and a session ends with its signaling connection — so
      // no holder can outlive the KEK epoch it was given, and neither refusal arm
      // is reachable by honest traffic today.
      //
      // Deliberately NOT argued from MC's internals. MC's KEK is not persisted
      // and a graceful reconnect redelivers the same bytes at the same generation
      // (the `already_held` arm above), but a meeting actor CAN be torn down
      // while the MC process lives, so "an MC restart drops every client" is not
      // the whole story and must not be the basis. The client-side fact holds
      // whatever MC does, which is what makes it the right one to depend on.
      //
      // Two consequences for whoever adds client reconnect:
      //  (a) a reconnect into a DIFFERENT KEK epoch must be a NEW session with a
      //      NEW holder — never a key swap on a live one. There is deliberately
      //      no accept-newer path: both sides carry the same generation number,
      //      the only ordering the wire has, so "newer" is unprovable and any
      //      such rule collapses into accept-the-latest-delivery. Do not "fix" a
      //      refusal here by relaxing it.
      //  (b) this is the arm that fails SILENTLY. An older generation partitions
      //      the meeting visibly (`no_kek_for_generation` / `kek_generation_stale`,
      //      both alerted); a same-generation mismatch holds the generation and
      //      fails at the unwrap tag, surfacing only as `unwrap_failed`, which no
      //      alert or panel selects. The refusal counter is the sole witness.
      //  Tracked in `docs/TODO.md` §Media Path Obligations.
      this.#observer?.installRefused('conflicting_key');
      this.#observer?.warn(
        'the meeting controller delivered a different key under the KEK generation already held; ' +
          'the held key is kept',
      );
      return 'refused';
    }

    if (generation < current.generation) {
      // A delayed or reordered push. Never demotes, and never reinstates a
      // retained or expired generation as current.
      this.#observer?.installRefused('older_generation');
      return 'refused';
    }

    // Newer: demote the current to previous, drop (and zeroize) the old previous.
    this.#dropPrevious();
    this.#held[0] = {
      generation: current.generation,
      kek: current.kek,
      expiresAtMs: this.#clock() + retention.retentionMs,
    };
    this.#held.unshift({ generation, kek: Uint8Array.from(kek) });
    this.#expiryTimer = this.#setTimeout(() => {
      this.#expiryTimer = undefined;
      this.#dropPrevious();
    }, retention.retentionMs);
    // Counted from the HELD STATE, not from having reached this branch: the
    // counter's whole job is to go flat when a refactor stops retaining, so it
    // must not be incremented by the code path alone.
    if (this.#held[1]?.generation === current.generation) this.#observer?.generationRetained();

    // Installed FIRST, then announced: a listener that rotates transmit keys
    // must find the new KEK already current, or its next mint would wrap under
    // the one a departed member holds (R-13).
    for (const listener of this.#listeners) listener();
    return 'installed_retaining';
  }

  #reportRetention(
    debounceSeconds: number,
    retentionMs: number,
    outcome: KekRetentionOutcome,
  ): void {
    if (outcome === 'nominal') return;
    this.#observer?.retentionAnomaly(outcome);
    this.#observer?.warn(
      ANOMALY_WARNINGS[outcome](debounceSeconds, retentionMs, this.#rewrapLatencyMs),
    );
  }

  kekForGeneration(generation: number): Uint8Array | undefined {
    this.#expireIfDue();
    return this.#held.find((h) => h.generation === generation)?.kek;
  }

  isOlderThanRetained(generation: number): boolean {
    this.#expireIfDue();
    const current = this.#held[0];
    if (!current || generation >= current.generation) return false;
    return !this.#held.some((h) => h.generation === generation);
  }

  currentForWrapping(): KekForWrapping | undefined {
    const current = this.#held[0];
    return current ? { kek: current.kek, generation: current.generation } : undefined;
  }

  /**
   * Drop both generations, overwriting the buffers first, and cancel expiry.
   *
   * ADR-0028 §5's "explicit token cleanup on disconnect/logout". JavaScript
   * cannot guarantee no engine-internal copy survives; the ADR names the
   * practice, not the guarantee, and this is the cheap place to honour it.
   */
  clear(): void {
    this.#cancelExpiry();
    for (const held of this.#held) held.kek.fill(0);
    this.#held.length = 0;
    this.#listeners.clear();
  }

  /**
   * The LAZY BACKSTOP for the expiry timer: a timer can fire late (a throttled
   * background tab), and a previous generation past its window must not open a
   * frame in the meantime.
   */
  #expireIfDue(): void {
    const previous = this.#held[1];
    if (previous?.expiresAtMs !== undefined && this.#clock() >= previous.expiresAtMs) {
      this.#dropPrevious();
    }
  }

  /** Zeroize and drop the retained previous generation, if any. */
  #dropPrevious(): void {
    this.#cancelExpiry();
    const previous = this.#held[1];
    if (!previous) return;
    previous.kek.fill(0);
    this.#held.splice(1, 1);
  }

  #cancelExpiry(): void {
    if (this.#expiryTimer !== undefined) {
      this.#clearTimeout(this.#expiryTimer);
      this.#expiryTimer = undefined;
    }
  }
}
