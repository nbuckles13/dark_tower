// File: packages/sdk-svelte/src/stores/snapshots.ts
//
// IMMUTABLE snapshot builders for the media store's keyed cells (story 2 R-10/R-11).
//
// Plain TypeScript, deliberately outside the `.svelte.ts` store: these build a
// NEW `ReadonlyMap` / `ReadonlySet` per change, which the store then ASSIGNS to
// its own `$state` cell. That is the store's reactivity model — replace, never
// mutate — so Svelte's reactive `SvelteMap` / `SvelteSet` (per-key reactivity for
// IN-PLACE mutation) would be the wrong tool: nothing here is ever mutated after
// it is published. Each result is a fresh instance, so a reader holding the
// previous snapshot never sees it change.

/** `map` with `key` set to `value` (or removed when `value` is `undefined`). */
export function withEntry<K, V>(
  map: ReadonlyMap<K, V>,
  key: K,
  value: V | undefined,
): ReadonlyMap<K, V> {
  const next = new Map(map);
  if (value === undefined) next.delete(key);
  else next.set(key, value);
  return next;
}

/** `set` with `member` added (`present`) or removed. */
export function withMember<T>(set: ReadonlySet<T>, member: T, present: boolean): ReadonlySet<T> {
  const next = new Set(set);
  if (present) next.add(member);
  else next.delete(member);
  return next;
}

/** An empty map snapshot. */
export function emptyMap<K, V>(): ReadonlyMap<K, V> {
  return new Map<K, V>();
}

/** An empty set snapshot. */
export function emptySet<T>(): ReadonlySet<T> {
  return new Set<T>();
}
