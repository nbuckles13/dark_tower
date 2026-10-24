// File: packages/sdk-core/src/media/lifecycle/keyframePolicy.ts
//
// THE keyframe gate for one published video stream (story 3 R-10, R-11, R-12,
// amendment 4). Every keyframe this publisher produces — for an MH request, or
// because a target has no group in progress — passes through here, so every
// one of them is bounded by ONE effective minimum interval and at most ONE
// deferral per stream. Pure apart from the injected clock and timer.
//
// ---------------------------------------------------------------------------
// REQUESTS ARE UNTRUSTED
// ---------------------------------------------------------------------------
//
// A `KeyframeRequest { stream_number, gop_counter }` on handler H's connection
// is honoured only when the stream is directed with NON-EMPTY targets, H is in
// its CURRENT target set (H is the connection the request was READ on, compared
// byte-exact — never anything in the payload), and video is not client-muted
// (server mute arrives as empty targets). A request NEVER starts capture and
// never makes a muted or undirected stream send. It needs
// `gop_counter >= current`; a counter above current is honoured and counted as
// `honoured_counter_ahead` (MH sends its newest counter, so equality is normal).
//
// ---------------------------------------------------------------------------
// THE INTERVAL AND THE ONE DEFERRAL
// ---------------------------------------------------------------------------
//
// effective = max(directed `min_keyframe_interval_ms`, compiled
// `KEYFRAME_MIN_INTERVAL_FLOOR_MS`). A directed value below the floor is
// clamped, counted per directive receipt, and WARNed once per session.
//
// The interval runs from the last keyframe — the last one DECIDED here, or the
// last key chunk the encoder actually EMITTED (an unsolicited platform key chunk
// restarts the clock: security S2), whichever is later.
//
// A keyframe owed inside the interval is HELD as the stream's single deferral —
// never dropped while still owed. Any later cause MERGES into it; there is never
// a second timer. The deferral is discarded on withdrawal or mute; a target-set
// change drops held requests whose handler left. At FIRE TIME every member is
// re-checked — a held request needs its handler still targeted and its counter
// still >= current; a held self-key needs some current target still awaiting a
// group — and the keyframe fires if any member survives, counted ONCE: under the
// cause that created the deferral, or the earliest surviving one if that lapsed.

import {
  KEYFRAME_MIN_INTERVAL_CEILING_MS,
  KEYFRAME_MIN_INTERVAL_FLOOR_MS,
} from '../../config/clientConfig.js';
import {
  MEDIA_KEYFRAME_REQUEST_OUTCOMES,
  MEDIA_KEYFRAME_SOURCES,
  type MediaKeyframePerKindOutcome,
  type MediaKeyframeSource,
  type MediaKindMetrics,
} from '../setup/mediaMetrics.js';

/** Why a self-key is owed: a target with no group in progress. */
export type SelfKeyCause =
  typeof MEDIA_KEYFRAME_SOURCES.DirectiveStart | typeof MEDIA_KEYFRAME_SOURCES.Abandonment;

/** One reason the held deferral exists. */
type DeferredCause =
  | { readonly kind: 'request'; readonly url: string; readonly gop: number }
  | { readonly kind: 'self'; readonly source: SelfKeyCause };

interface Deferral {
  /** Members in arrival order; `[0]` created it. */
  causes: DeferredCause[];
  timer: ReturnType<typeof setTimeout> | undefined;
}

/** Construction options for {@link KeyframeGate}. */
export interface KeyframeGateOptions {
  readonly metrics: MediaKindMetrics;
  readonly clock: () => number;
  readonly setTimeout: (fn: () => void, ms: number) => ReturnType<typeof setTimeout>;
  readonly clearTimeout: (handle: ReturnType<typeof setTimeout>) => void;
  /** The stream's current GoP counter (`TransmitKeyManager.currentGopCounter`). */
  readonly currentGop: () => number | undefined;
  /** Active targets the publisher owes a group (`VideoEgress.targetsAwaitingGroup`). */
  readonly targetsAwaitingGroup: () => number;
  /** Ask the encoder for a key chunk on the next captured frame. */
  readonly produceKeyframe: () => void;
  /** Lifecycle WARN sink (fixed text + config values only). */
  readonly warn: (message: string) => void;
}

/** See the module header. */
export class KeyframeGate {
  readonly #o: KeyframeGateOptions;
  #targets: ReadonlySet<string> = new Set();
  #muted = false;
  #effectiveIntervalMs = KEYFRAME_MIN_INTERVAL_FLOOR_MS;
  #lastKeyAt: number | undefined;
  #deferral: Deferral | undefined;
  #clampWarned = false;
  #closed = false;
  /**
   * Keyframes PRODUCED, by cause (cumulative). The same call site as
   * `keyframeProduced`, so the sampled view and the metric cannot disagree.
   * Counts only — no timing (story 3 C-C1, the test bus's `sendCounts`).
   */
  readonly #produced = Object.fromEntries(
    Object.values(MEDIA_KEYFRAME_SOURCES).map((source) => [source, 0]),
  ) as Record<MediaKeyframeSource, number>;

  constructor(options: KeyframeGateOptions) {
    this.#o = options;
  }

  /** max(directed, floor), as currently in force. */
  get effectiveIntervalMs(): number {
    return this.#effectiveIntervalMs;
  }

  /** Keyframes produced so far, by cause (a fresh copy; counts only). Sample it, never per frame. */
  producedBySource(): Readonly<Record<MediaKeyframeSource, number>> {
    return { ...this.#produced };
  }

  /** Whether a deferral is held (for assertion). */
  get hasDeferral(): boolean {
    return this.#deferral !== undefined;
  }

  /**
   * Apply a validated directive's video targets and directed minimum interval.
   * A target-set change re-checks the held deferral; an empty set discards it.
   */
  setDirective(targets: readonly string[], directedMinIntervalMs: number): void {
    if (directedMinIntervalMs < KEYFRAME_MIN_INTERVAL_FLOOR_MS) {
      this.#o.metrics.keyframeMinIntervalClamped();
      if (!this.#clampWarned) {
        this.#clampWarned = true;
        this.#o.warn(
          `directed min_keyframe_interval_ms ${directedMinIntervalMs} is below the compiled ` +
            `floor ${KEYFRAME_MIN_INTERVAL_FLOOR_MS}; clamped to the floor`,
        );
      }
    }
    // `validateVideoEncoding` refuses a value above the compiled ceiling; the
    // min() is defense in depth so no delay can overflow a timer into a spin.
    this.#effectiveIntervalMs = Math.min(
      Math.max(directedMinIntervalMs, KEYFRAME_MIN_INTERVAL_FLOOR_MS),
      KEYFRAME_MIN_INTERVAL_CEILING_MS,
    );
    this.#targets = new Set(targets.filter((url) => url !== ''));
    if (this.#targets.size === 0) {
      this.#discardDeferral();
      return;
    }
    const held = this.#deferral;
    if (held) {
      held.causes = held.causes.filter((c) => c.kind !== 'request' || this.#targets.has(c.url));
      if (held.causes.length === 0) this.#discardDeferral();
    }
  }

  /** Client video mute. Muting discards the deferral. */
  setMuted(muted: boolean): void {
    this.#muted = muted;
    if (muted) this.#discardDeferral();
  }

  /** The encoder emitted a key chunk (solicited or not): the interval restarts here. */
  keyChunkEmitted(): void {
    this.#lastKeyAt = this.#o.clock();
  }

  /** An MH `KeyframeRequest` for THIS stream, read on handler `url`'s connection. */
  request(url: string, gopCounter: number): MediaKeyframePerKindOutcome {
    const outcome = this.#classifyRequest(url, gopCounter);
    this.#o.metrics.keyframeRequestReceived(outcome);
    return outcome;
  }

  /** A target has no group in progress the publisher owes it. */
  selfKey(cause: SelfKeyCause): void {
    if (this.#closed || !this.#sending()) return;
    if (this.#o.targetsAwaitingGroup() === 0) return;
    if (this.#deferral) {
      this.#merge({ kind: 'self', source: cause });
      return;
    }
    if (this.#tooSoon()) {
      this.#defer({ kind: 'self', source: cause });
      return;
    }
    this.#produce(cause);
  }

  /** Stop all timers. Idempotent. */
  close(): void {
    this.#closed = true;
    this.#discardDeferral();
  }

  #classifyRequest(url: string, gop: number): MediaKeyframePerKindOutcome {
    if (this.#closed || !this.#sending() || !this.#targets.has(url)) {
      return MEDIA_KEYFRAME_REQUEST_OUTCOMES.NotSending;
    }
    const current = this.#o.currentGop();
    if (current !== undefined && gop < current) return MEDIA_KEYFRAME_REQUEST_OUTCOMES.StaleCounter;
    const ahead = current === undefined || gop > current;
    if (this.#deferral) {
      this.#merge({ kind: 'request', url, gop });
      return MEDIA_KEYFRAME_REQUEST_OUTCOMES.MinInterval;
    }
    if (this.#tooSoon()) {
      this.#defer({ kind: 'request', url, gop });
      return MEDIA_KEYFRAME_REQUEST_OUTCOMES.MinInterval;
    }
    this.#produce(MEDIA_KEYFRAME_SOURCES.Request);
    return ahead
      ? MEDIA_KEYFRAME_REQUEST_OUTCOMES.HonouredCounterAhead
      : MEDIA_KEYFRAME_REQUEST_OUTCOMES.Honoured;
  }

  #sending(): boolean {
    return this.#targets.size > 0 && !this.#muted;
  }

  #remainingMs(): number {
    if (this.#lastKeyAt === undefined) return 0;
    return this.#lastKeyAt + this.#effectiveIntervalMs - this.#o.clock();
  }

  #tooSoon(): boolean {
    return this.#remainingMs() > 0;
  }

  #produce(source: MediaKeyframeSource): void {
    this.#lastKeyAt = this.#o.clock();
    this.#produced[source] += 1;
    this.#o.metrics.keyframeProduced(source);
    this.#o.produceKeyframe();
  }

  /**
   * Merge a cause into the held deferral, BOUNDED whatever the input rate
   * (requests are untrusted): at most one request member per handler url
   * (holding the highest counter — the one most likely to survive the
   * `>= current` re-check) and one self member per source. Creator and
   * counting semantics are unchanged.
   */
  #merge(cause: DeferredCause): void {
    const held = this.#deferral;
    if (!held) return;
    const same = held.causes.findIndex((c) =>
      c.kind === 'request' && cause.kind === 'request'
        ? c.url === cause.url
        : c.kind === 'self' && cause.kind === 'self' && c.source === cause.source,
    );
    if (same < 0) {
      held.causes.push(cause);
      return;
    }
    const existing = held.causes[same];
    if (existing?.kind === 'request' && cause.kind === 'request' && cause.gop > existing.gop) {
      held.causes[same] = cause;
    }
  }

  /** The held deferral's member count (for the boundedness assertion). */
  get deferredCauseCount(): number {
    return this.#deferral?.causes.length ?? 0;
  }

  #defer(cause: DeferredCause): void {
    const deferral: Deferral = { causes: [cause], timer: undefined };
    this.#deferral = deferral;
    this.#arm(deferral);
  }

  #arm(deferral: Deferral): void {
    deferral.timer = this.#o.setTimeout(
      () => this.#fire(deferral),
      Math.max(0, this.#remainingMs()),
    );
  }

  #fire(deferral: Deferral): void {
    if (this.#deferral !== deferral || this.#closed) return;
    deferral.timer = undefined;
    // An emitted key chunk may have restarted the interval meanwhile: still owed,
    // so re-arm for the remainder rather than drop.
    if (this.#tooSoon()) {
      this.#arm(deferral);
      return;
    }
    this.#deferral = undefined;
    if (!this.#sending()) return;
    const current = this.#o.currentGop();
    const awaiting = this.#o.targetsAwaitingGroup();
    const survives = (c: DeferredCause): boolean =>
      c.kind === 'request'
        ? this.#targets.has(c.url) && (current === undefined || c.gop >= current)
        : awaiting > 0;
    const creator = deferral.causes[0];
    const survivor =
      creator !== undefined && survives(creator) ? creator : deferral.causes.find(survives);
    if (survivor === undefined) return;
    this.#produce(survivor.kind === 'request' ? MEDIA_KEYFRAME_SOURCES.Request : survivor.source);
  }

  #discardDeferral(): void {
    const held = this.#deferral;
    this.#deferral = undefined;
    if (held?.timer !== undefined) this.#o.clearTimeout(held.timer);
  }
}
