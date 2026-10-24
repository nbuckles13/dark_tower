# @darktower/sdk-svelte

Svelte 5 adapter for `@darktower/sdk-core`: rune stores that mirror a
`MeetingSession`'s events reactively, with automatic cleanup on component unmount
(R-28), and the `VideoTile` render surface for a video receive slot (story 3
C-C1).

## API

- **`bindMeetingSession(session)`** — call during component init. Subscribes to
  the session's events, returns a reactive `MeetingStore`, and unsubscribes
  automatically on unmount (via `onDestroy`).
- **`MeetingStore`** — reactive `$state`, exposed through getters:
  - `meetingState` — the current `MeetingSessionState` (`$meetingState`)
  - `participants` — live roster, `RosterParticipant[]` (`$participants`)
  - `mediaConnections` — connected MH URLs, `string[]` (`$mediaConnections`)
  - `lastError` — the latest `SdkError | undefined` (`$lastError`)

  > The requirement's `$name` stores are realized as `$state`-backed **getters**:
  > Svelte 5 `$state` can't be exported as a bare re-assignable binding across a
  > module boundary (it breaks reactivity), so `store.participants` in a template
  > *is* the reactive `$participants` store.

- **`MeetingStore.media`** (a `MediaStore`) — the media-path projections, with
  FINE-GRAINED cells so a change re-renders only what it describes:
  - `audioMuted` / `videoMuted`, `firstMediaFrameMs`, `slots`,
    `unreachableSenderIds`, `lastMediaFault`, `cameraFailure` (R-19);
  - `slot(slotId)` — ONE slot's `assignment` and video `render` state;
  - `participant(participantId)` — ONE participant's per-kind server mute,
    who applied it, and a pending unmute request;
  - `pairOf(senderId)` — the audio and video slot showing ONE sender (paired by
    publisher, never by slot index; a live video slot by its verified sender);
  - `resumeNoticePending(kind)` / `acknowledgeResume(kind)` — the R-18 latch,
    set only by the SDK's `captureResumedAfterServerMute`, cleared only by an
    acknowledgement.

  Readers never construct cells (a reader of a not-yet-created key gets an
  empty view and re-runs when the cell appears). Decoded frames never touch
  the store.
- **`VideoTile`** — `<VideoTile slotId view slot onSurface?>{avatar}</VideoTile>`:
  binds a canvas as the slot's `VideoSink` through `VideoReceiveView.setVideoSink`
  and draws each frame synchronously (the SDK's borrow contract), sizing the
  backing store to the frame. The canvas shows only while the SDK's render state
  is `active`; otherwise the children (avatar / empty copy) render. `onSurface`
  hands an embedder the canvas and a paint count (the demo's test bus uses it).
- **`subscribeSession(store, session)`** — the pure event→state wiring behind
  `bindMeetingSession` (no component lifecycle); returns an aggregate unsubscribe.
- **`BoundMeetingSession`** — the structural type `bindMeetingSession` accepts
  (`{ state; on(...) }`); the concrete `MeetingSession` satisfies it.

## Usage

```svelte
<script lang="ts">
  import { bindMeetingSession } from '@darktower/sdk-svelte';
  import { MeetingSession } from '@darktower/sdk-core';

  const session = new MeetingSession({ acOriginTemplate, gcBaseUrl });
  const store = bindMeetingSession(session);
  // ... session.join({ ... }) elsewhere
</script>

<p>{store.meetingState}</p>
<ul>
  {#each store.participants as p (p.participantId)}<li>{p.name}</li>{/each}
</ul>
```

## Scripts

| Script | What |
|--------|------|
| `pnpm build` | Vite library build (ESM + CJS + `.d.ts`) |
| `pnpm test:component` | Vitest Browser Mode (Chromium) component tests |
| `pnpm lint` | svelte-check + eslint + prettier |
