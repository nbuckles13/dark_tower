// Ambient declarations for `@darktower/web-app`.
//
// Svelte 5 rune globals for any `.svelte.ts` / `<script>` runes.
/// <reference types="svelte" />

// Build-time Vite `define` literals (see vite.config.ts).
declare const __SDK_VERSION__: string;
declare const __DEV_TRUST_FINGERPRINT__: boolean;
/** Separate flag from __DEV_TRUST_FINGERPRINT__: gates the E2E test bus. */
declare const __E2E_HOOKS__: boolean;
/** Dev MC/MH cert SHA-256 fingerprints (base64); empty in prod / when absent. */
declare const __DEV_CERT_SHA256_HASHES__: readonly string[];

/**
 * The stable E2E contract surface (R-29) — attached to `window` only when the
 * `__E2E_HOOKS__` define is true (tree-shaken out of prod). Consumed by the
 * test-owned Playwright specs (tasks #18/#19). Carries only bounded, non-PII
 * fields (never tokens / bindingToken / correlationId / email / password).
 */
interface DarktowerTestBus {
  /** Replay buffer of every bounded event fired so far (late subscribers included). */
  readonly events: ReadonlyArray<Readonly<Record<string, unknown>>>;
  /** Subscribe to a bounded event type; returns an unsubscribe closure. */
  on(type: string, listener: (event: Readonly<Record<string, unknown>>) => void): () => void;
}

interface Window {
  __darktower_test__?: DarktowerTestBus;
}
