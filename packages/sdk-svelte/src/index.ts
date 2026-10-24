// File: packages/sdk-svelte/src/index.ts
//
// Public barrel for `@darktower/sdk-svelte`. The package `exports` map is closed
// (no `./*` wildcard) — this barrel is the only entry point. Svelte 5 adapter:
// rune stores wrapping the sdk-core `MeetingSession` events (R-28), and the
// `VideoTile` render surface (story 3 C-C1).
//
// All event/roster/state/error types are re-used from `@darktower/sdk-core` —
// none are re-declared here.

export { MeetingStore } from './stores/MeetingStore.svelte.js';
export { MediaStore } from './stores/MediaStore.svelte.js';
export type { PairView, ParticipantMediaView, SlotView } from './stores/mediaCells.svelte.js';

// Story 3 C-C1: the slot's render surface. Binds a canvas as the slot's
// `VideoSink`; decoded frames are drawn there and never touch a store.
export { default as VideoTile } from './components/VideoTile.svelte';
export type { TileSurface } from './components/VideoTile.svelte';

export { bindMeetingSession, subscribeSession } from './stores/bindMeetingSession.js';
export type { BoundMeetingSession } from './stores/bindMeetingSession.js';
