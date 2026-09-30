// File: packages/web-app/e2e/windowSampling.ts
//
// THE stop condition for an observation window sampled by out-of-page reads
// (`receiveEvidence.ts:observeWindow`). Pure — no Playwright, no `./env`, the
// clock and the sleep are injected — so `tests/windowSampling.test.ts` drives it
// hermetically with a fake clock.
//
// Why it exists (task-17 runner-gate failure, 2026-09-30): the window used to
// stop on wall-clock alone. Each sample is a `page.evaluate` per probe, so under
// load one read can take a large fraction of the window, and a fixed window then
// yielded fewer samples than the anti-vacuity floor — a HARNESS failure on a
// correct mute. The rule is now: stop only when the window has elapsed AND the
// sample floor is met; keep sampling otherwise, under a hard upper bound that
// fails LOUDLY. The floor is never lowered and the vacuity guard in the caller
// stays — this only stops the loop from quitting early.
//
// The bound is HARD for a slow or hung read too: each read is raced against the
// budget left, so a read that never returns fails here, as the same HARNESS
// error, instead of sitting until the Playwright test timeout.
//
// Also here: the same-page mover rule (`unpairedObservers`), pure so it has a
// positive control of its own.

/** What the loop does after a sample. */
export type WindowStep = 'continue' | 'done' | 'overrun';

/** The inputs of one stop decision. All times in ms on one monotonic-enough clock. */
export interface WindowStepInput {
  /** Samples taken so far (including the one just taken). */
  readonly samples: number;
  /** Time elapsed since the window started. */
  readonly elapsedMs: number;
  /** The minimum observation span. */
  readonly windowMs: number;
  /** The sample floor (anti-vacuity). */
  readonly minSamples: number;
  /** The hard upper bound on the whole window. */
  readonly maxMs: number;
}

/**
 * The stop condition. `done` needs BOTH the span and the floor; short of that,
 * `overrun` once the hard bound is reached, else `continue`.
 */
export function windowStep(input: WindowStepInput): WindowStep {
  const { samples, elapsedMs, windowMs, minSamples, maxMs } = input;
  if (elapsedMs >= windowMs && samples >= minSamples) return 'done';
  if (elapsedMs >= maxMs) return 'overrun';
  return 'continue';
}

/** Why a sampled window is refused before it starts: the observers lacking a mover. */
export interface ObservedProbe {
  /** Label of the page the probe reads. */
  readonly observer: string;
  readonly expect: 'flat' | 'advance' | 'zero';
}

/**
 * The same-page mover rule. Returns `null` when NO probe must advance (the whole
 * window would be vacuous), else the observers that carry a flat/zero probe but
 * no `advance` probe of their own (empty = the window is well formed). Each read
 * returns a page's latest bus snapshot, so a flat reading on a page with no
 * mover could be a stalled page sampler repeating a stale value.
 */
export function unpairedObservers(probes: readonly ObservedProbe[]): string[] | null {
  const movers = new Set(probes.filter((p) => p.expect === 'advance').map((p) => p.observer));
  if (movers.size === 0) return null;
  return [...new Set(probes.filter((p) => !movers.has(p.observer)).map((p) => p.observer))];
}

/** The loop's parameters and its injected effects. */
export interface SampleWindowOptions<T> {
  /** Take one sample (every probe, together). */
  readonly sample: () => Promise<T>;
  readonly windowMs: number;
  readonly minSamples: number;
  readonly maxMs: number;
  /** Pause between samples. */
  readonly intervalMs: number;
  /** Names the window in the overrun error. */
  readonly why: string;
  readonly now?: () => number;
  readonly sleep?: (ms: number) => Promise<void>;
  /** A cancellable timer the in-flight read is raced against. */
  readonly timer?: (ms: number) => { readonly fired: Promise<void>; cancel(): void };
}

/** What the raced timer resolves to. */
const TIMED_OUT: unique symbol = Symbol('timed-out');

/** The real cancellable timer: nothing is left pending once a read wins the race. */
function realTimer(ms: number): { readonly fired: Promise<void>; cancel(): void } {
  let handle: ReturnType<typeof setTimeout> | undefined;
  const fired = new Promise<void>((resolve) => {
    handle = setTimeout(resolve, ms);
  });
  return { fired, cancel: () => clearTimeout(handle) };
}

/**
 * Sample until {@link windowStep} says `done`; THROW a HARNESS error on
 * `overrun`, or when one read outlives the budget left before the bound (the
 * read is raced against it). Returns every sample, in order.
 */
export async function sampleWindow<T>(opts: SampleWindowOptions<T>): Promise<T[]> {
  const { windowMs, minSamples, maxMs } = opts;
  if (!(maxMs > windowMs)) {
    throw new Error(
      `sampleWindow(${opts.why}): hard bound ${maxMs}ms must exceed the window ${windowMs}ms`,
    );
  }
  const now = opts.now ?? Date.now;
  const sleep = opts.sleep ?? ((ms: number) => new Promise<void>((r) => setTimeout(r, ms)));
  const start = now();
  const out: T[] = [];
  const overrun = (): Error =>
    new Error(
      `HARNESS: observation window (${opts.why}) reached its hard bound of ${maxMs}ms ` +
        `with ${out.length} sample(s) (need >= ${minSamples}); the probe reads are too slow ` +
        `to observe the window at all. This is a harness/load failure, not a media verdict.`,
    );
  const makeTimer = opts.timer ?? realTimer;
  for (;;) {
    const timer = makeTimer(Math.max(0, maxMs - (now() - start)));
    let value: T | typeof TIMED_OUT;
    try {
      value = await Promise.race([
        opts.sample(),
        timer.fired.then((): typeof TIMED_OUT => TIMED_OUT),
      ]);
    } finally {
      timer.cancel();
    }
    if (value === TIMED_OUT) throw overrun();
    out.push(value);
    const elapsedMs = now() - start;
    const step = windowStep({ samples: out.length, elapsedMs, windowMs, minSamples, maxMs });
    if (step === 'done') return out;
    if (step === 'overrun') throw overrun();
    await sleep(opts.intervalMs);
  }
}
