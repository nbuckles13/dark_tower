// File: packages/sdk-svelte/src/index.ts
//
// Public barrel for `@darktower/sdk-svelte`. The package `exports` map is closed
// (no `./*` wildcard) — this barrel is the only entry point. Svelte 5 adapter:
// rune stores wrapping the sdk-core `MeetingSession` events (R-28). No video
// components yet (deferred).
//
// All event/roster/state/error types are re-used from `@darktower/sdk-core` —
// none are re-declared here.

export { MeetingStore } from './stores/MeetingStore.svelte.js';

export { bindMeetingSession, subscribeSession } from './stores/bindMeetingSession.js';
export type { BoundMeetingSession } from './stores/bindMeetingSession.js';
