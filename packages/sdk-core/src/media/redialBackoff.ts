// File: packages/sdk-core/src/media/redialBackoff.ts
//
// THE ONE redial-delay rule for a lost in-set media handler (ADR-0039 §4). Pure:
// the random source is injected, so tests pin it.
//
//   delay(n) = (n == 0 ? floorMs : 0) + U[0, min(capMs, baseMs * 2^n)]
//
// Full jitter, plus a floor ADDED on the FIRST retry: "the first retry is no
// earlier than one MH keepalive interval", because a redial before MH idles out
// the old session meets BINDING_CONFLICT. The floor is added rather than taken
// as `max(floor, jitter)` so the first retry keeps its jitter — with `max`,
// every survivor of one handler crash would redial at the same instant.
//
// `n` is the number of consecutive failed attempts since the ref last
// connected; it resets on a successful connect (the caller's state).

/** The inputs of {@link redialDelayMs}. */
export interface RedialBackoff {
  /** Exponential base (`MediaConfig.handlers.redialBaseMs`). */
  readonly baseMs: number;
  /** Cap on the jitter window (`MediaConfig.handlers.redialCapMs`). */
  readonly capMs: number;
  /** Added to the FIRST retry only (`MH_KEEPALIVE_FLOOR_MS`). */
  readonly floorMs: number;
  /** Uniform in [0, 1). Default `Math.random`; tests pin it. */
  readonly random: () => number;
}

/**
 * THE capped exponential backoff window `min(capMs, baseMs * 2^n)` — shared by
 * the MH redial (full jitter, below) and the join retry (equal jitter,
 * `session/joinRetry.ts`). The jitter policies differ on purpose; the window is
 * one rule.
 */
export function backoffWindowMs(
  attempt: number,
  backoff: { readonly baseMs: number; readonly capMs: number },
): number {
  // 2^n overflows to Infinity long before it matters; min() then takes the cap.
  return Math.min(backoff.capMs, backoff.baseMs * 2 ** Math.max(0, attempt));
}

/** The delay before redial attempt `attempt` (0-based). Never negative. */
export function redialDelayMs(attempt: number, backoff: RedialBackoff): number {
  const window = backoffWindowMs(attempt, backoff);
  const jitter = Math.floor(backoff.random() * window);
  return (attempt === 0 ? backoff.floorMs : 0) + jitter;
}
