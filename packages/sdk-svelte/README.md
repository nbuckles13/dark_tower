# @darktower/sdk-svelte

Svelte 5 adapter for `@darktower/sdk-core`: rune stores that mirror a
`MeetingSession`'s events reactively, with automatic cleanup on component unmount
(R-28). No video components yet (deferred).

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
| `pnpm test:component` | Vitest 4 Browser Mode (Chromium) component tests |
| `pnpm lint` | svelte-check + eslint + prettier |
