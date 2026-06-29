// File: packages/sdk-core/src/events/TypedEventEmitter.ts
//
// Shared generic typed event-emitter (extract-on-third-use, @dry-reviewer /
// @code-reviewer E): the identical `#listeners` Map + `on`/`off`/`emit<K>`
// plumbing previously lived inline in `SignalingClient` and would otherwise be
// copy-pasted into `MediaTransport` (#2) and `MeetingSession` (#3). One
// implementation, three typed event maps. No metric / trace / log emission
// originates here.
//
// `emit` is `protected` — only the owning subclass may fire events; consumers get
// only `on`/`off`. Copy-on-emit: a listener that unsubscribes (or subscribes)
// mid-dispatch cannot mutate the in-flight iteration.

/** A listener for event `K` in `EventMap`. */
export type EventListener<EventMap, K extends keyof EventMap> = (payload: EventMap[K]) => void;

/**
 * Minimal typed event emitter. Subclass with a concrete `EventMap` (an interface
 * mapping event names to payload types); call the `protected emit` to dispatch.
 *
 * @example
 * interface FooEvents { ready: void; error: Error }
 * class Foo extends TypedEventEmitter<FooEvents> {
 *   start() { this.emit('ready', undefined); }
 * }
 */
export class TypedEventEmitter<EventMap> {
  readonly #listeners = new Map<keyof EventMap, Set<(payload: never) => void>>();

  /**
   * Register a listener for `type`. Returns an unsubscribe function (calling it is
   * equivalent to {@link off}).
   */
  on<K extends keyof EventMap>(type: K, listener: EventListener<EventMap, K>): () => void {
    let set = this.#listeners.get(type);
    if (set === undefined) {
      set = new Set();
      this.#listeners.set(type, set);
    }
    set.add(listener as (payload: never) => void);
    return () => {
      this.off(type, listener);
    };
  }

  /** Remove a previously-registered listener. No-op if not registered. */
  off<K extends keyof EventMap>(type: K, listener: EventListener<EventMap, K>): void {
    this.#listeners.get(type)?.delete(listener as (payload: never) => void);
  }

  /**
   * Dispatch `payload` to every listener of `type`. Iterates a COPY so a listener
   * that (un)subscribes mid-emit doesn't mutate the in-flight set.
   */
  protected emit<K extends keyof EventMap>(type: K, payload: EventMap[K]): void {
    const set = this.#listeners.get(type);
    if (set === undefined) return;
    for (const listener of [...set]) {
      (listener as EventListener<EventMap, K>)(payload);
    }
  }
}
