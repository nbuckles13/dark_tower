<script lang="ts">
  // Test harness: binds a (mock) session via `bindMeetingSession` during
  // component init and renders the reactive store. `onStore` hands the store
  // back to the test so the unmount-leak assertion can observe it AFTER the
  // component (and its `onDestroy` cleanup) is gone.
  import { bindMeetingSession } from '../../stores/bindMeetingSession.js';
  import type { BoundMeetingSession } from '../../stores/bindMeetingSession.js';
  import type { MeetingStore } from '../../stores/MeetingStore.svelte.js';

  let {
    session,
    onStore,
  }: {
    session: BoundMeetingSession;
    onStore?: (store: MeetingStore) => void;
  } = $props();

  // svelte-ignore state_referenced_locally
  const store = bindMeetingSession(session);
  // svelte-ignore state_referenced_locally
  onStore?.(store);
</script>

<div data-testid="state">{store.meetingState}</div>
<ul data-testid="participant-list">
  {#each store.participants as participant (participant.participantId)}
    <li data-testid={`participant-${participant.participantId}`}>{participant.name}</li>
  {/each}
</ul>
<div data-testid="media-connections">{store.mediaConnections.length}</div>
<div data-testid="last-error">{store.lastError ? store.lastError.code : ''}</div>
