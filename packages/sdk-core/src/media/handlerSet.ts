// File: packages/sdk-core/src/media/handlerSet.ts
//
// THE CLIENT'S HANDLER IDENTITY IS `handler_ref` (ADR-0039 §4). Every
// per-handler thing on this client — the transport, the selectors over it, the
// egress lanes, the read loops, the hop monitors, the recovery queues, the
// public events and the test bus — is keyed by the branded `HandlerRef` below,
// never by the handler URL. A URL is an ADDRESS that an incarnation holds and a
// later incarnation may reuse (ADR-0039 §7); the ref is the incarnation. Keying
// anything on the URL would let a new incarnation at an old address inherit the
// old one's state, which is exactly the "new ref at a used URL must be a real
// close + open" rule this module exists to make structural.
//
// Two shapes:
//   * `HandlerSetView` — PUBLIC, token-free: `{version, handlers:[{ref, url}]}`.
//     The only shape that reaches events, stores, the e2e bus or the DOM.
//   * `HandlerSetIntake` — INTERNAL: the view plus each member's dial token. It
//     travels exactly one hop, from `signaling/handlerSetIntake.ts` into
//     `MediaTransport.applyHandlerSet`, and is never exported from the package.
//
// And one pure function, `diffHandlerSet`, keyed on the ref ONLY.

/**
 * An MC-assigned media-handler incarnation id (`MediaServerInfo.handler_ref`,
 * uint32, never 0 for a member). Branded so a URL or an arbitrary number cannot
 * be passed where a handler identity is required.
 *
 * OBSERVABILITY (ADR-0039 §8): never a metric label, span attribute or
 * per-frame log field.
 */
export type HandlerRef = number & { readonly __handlerRef: unique symbol };

/** Brand a wire `handler_ref`. The caller has already rejected 0. */
export function handlerRef(value: number): HandlerRef {
  return value as HandlerRef;
}

/** One member of a handler set, as the public surface sees it. */
export interface MediaHandlerView {
  readonly ref: HandlerRef;
  readonly url: string;
}

/**
 * The participant's current media-handler set, token-free (ADR-0039 §4).
 *
 * `version` is the MEMBERSHIP EPOCH (`handler_set_version`, uint64) as its
 * exact DECIMAL STRING: a correlation key, never an ordering or discard key, and
 * a string so this public payload stays `JSON.stringify`-able (a bigint
 * throws). The transport echoes the wire `bigint` itself
 * ({@link HandlerSetIntake.version}); nothing parses this string back.
 */
export interface HandlerSetView {
  readonly version: string;
  readonly handlers: readonly MediaHandlerView[];
}

/**
 * INTERNAL. A validated set plus each member's `media_dial` token, in the
 * set's wire order. Constructed only by `signaling/handlerSetIntake.ts`;
 * consumed only by `MediaTransport.applyHandlerSet`, which moves the tokens into
 * its one private holder. Never emitted, never stored anywhere else.
 */
export interface HandlerSetIntake {
  readonly view: HandlerSetView;
  /** The wire `handler_set_version` (uint64), echoed as is on loss reports. */
  readonly version: bigint;
  /** ref -> the opaque dial token for it. One entry per member of `view`. */
  readonly dialTokens: ReadonlyMap<HandlerRef, string>;
}

/** What changed between two sets, by ref. */
export interface HandlerSetDiff {
  /** In `next`, not in `prev`. In `next`'s order. */
  readonly added: readonly HandlerRef[];
  /** In `prev`, not in `next`. */
  readonly removed: readonly HandlerRef[];
  /** In both. In `next`'s order. */
  readonly retained: readonly HandlerRef[];
}

/**
 * Diff two memberships on `handler_ref` ONLY. A URL never participates: the
 * same URL under a new ref is one removal plus one add (a real close and a
 * fresh dial), and identical refs with new tokens are a token-only refresh
 * (nothing added, nothing removed).
 */
export function diffHandlerSet(
  prev: Iterable<HandlerRef>,
  next: readonly HandlerRef[],
): HandlerSetDiff {
  const before = new Set(prev);
  const after = new Set(next);
  return {
    added: next.filter((ref) => !before.has(ref)),
    removed: [...before].filter((ref) => !after.has(ref)),
    retained: next.filter((ref) => before.has(ref)),
  };
}
