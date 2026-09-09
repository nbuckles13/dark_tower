<script lang="ts">
  // Test harness for the RE-RENDER GRANULARITY property (ADR-0036 story task 20:
  // "minimizing re-renders on mute and unmute").
  //
  // The claim under test is that a mute toggle does not invalidate the roster
  // projection. A comment asserting that would be worthless, so this component
  // makes the invalidation OBSERVABLE: each `$derived` counts its own
  // recomputations into a counter the test reads. Both derivations are rendered,
  // because `$derived` is lazy — an unread derivation never recomputes and the
  // test would pass for the wrong reason.
  import { bindMeetingSession } from '../../stores/bindMeetingSession.js';
  import type { BoundMeetingSession } from '../../stores/bindMeetingSession.js';

  let {
    session,
    counts,
  }: {
    session: BoundMeetingSession;
    /** Mutable recompute tallies, owned by the test. */
    counts: { roster: number; mute: number };
  } = $props();

  // svelte-ignore state_referenced_locally
  const store = bindMeetingSession(session);

  const rosterText = $derived.by(() => {
    counts.roster += 1;
    return store.participants.map((p) => p.name).join(',');
  });

  const muteText = $derived.by(() => {
    counts.mute += 1;
    return store.media.audioMuted ? 'muted' : 'unmuted';
  });
</script>

<div data-testid="roster-text">{rosterText}</div>
<div data-testid="mute-text">{muteText}</div>
