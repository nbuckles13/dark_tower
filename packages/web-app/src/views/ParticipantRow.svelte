<script lang="ts">
  // One roster row (story 2 R-10, R-11, R-33; story 3 R-15; ADR-0036 §5, §6, §9).
  //
  // Everything here is a wire fact rendered, never an inference from silence:
  // reachability from `unreachable_sender_ids`, server mute PER KIND (and who
  // applied it) from `ParticipantMuteUpdate`, client mute per kind from the
  // participant's roster mute facts (their roster entry, moved by every later
  // `ParticipantMuteUpdate`) — never from slots, so a participant none of our
  // slots shows still has a known client mute. The `data-*` attributes carry the TOKENS
  // a test asserts on; the text comes from `lib/participantState.ts`.
  //
  // It reads only THIS participant's store cells (`mediaFactsFor`), so another
  // participant's mute change re-renders nothing here.
  //
  // The host affordance — mute and unmute PER KIND (story 3 R-15) — sends target
  // + action only, and renders only under the host predicate (a UI hint: MC is
  // the authority on every request). Each button's label follows the WIRE
  // state, not the click: nothing here changes until MC's update arrives, so a
  // refused request can never leave a "muted" label beside an unmuted
  // participant.
  import type { MeetingSession, PublishKind, RosterParticipant } from '@darktower/sdk-core';
  import type { MeetingStore } from '@darktower/sdk-svelte';
  import { errorText } from '../lib/errorText.js';
  import {
    MEDIA_KINDS,
    PARTICIPANT_TEXT,
    boundedName,
    mediaFactsFor,
    participantIndicators,
    serverMutedByText,
  } from '../lib/participantState.js';
  import MuteIndicators from './MuteIndicators.svelte';

  let {
    participant,
    store,
    session,
    selfParticipantId,
    hostControls,
  }: {
    participant: RosterParticipant;
    store: MeetingStore;
    session: MeetingSession;
    selfParticipantId: string | undefined;
    hostControls: boolean;
  } = $props();

  let controlError = $state('');

  const view = $derived(
    participantIndicators(participant, mediaFactsFor(store.media, participant), {
      roster: store.participants,
      selfParticipantId,
    }),
  );
  const unmuteRequested = $derived(
    store.media.participant(participant.participantId).unmuteRequested,
  );
  const serverMutedAny = $derived(view.serverMuted.audio || view.serverMuted.video);
  /** Story-2 roster token (audio): kept so its specs read the same attribute. */
  const clientMutedAudio = $derived(view.clientMuted.audio);

  async function toggleServerMute(kind: PublishKind): Promise<void> {
    controlError = '';
    try {
      // ONE kind per request: the other kind's field is left absent, so MC
      // leaves it exactly as it is (story 3 R-15).
      await session.setServerMute(participant.participantId, kind, !view.serverMuted[kind]);
    } catch (err) {
      controlError = errorText(err);
    }
  }
</script>

<li
  data-testid={`participant-${participant.participantId}`}
  data-reachability={view.reachability}
  data-server-muted={view.serverMuted.audio}
  data-server-muted-video={view.serverMuted.video}
  data-client-muted={clientMutedAudio}
  data-client-muted-video={view.clientMuted.video}
>
  <!-- The NAME alone, so the roster-name assertion stays exact beside the indicators. -->
  <span data-testid={`participant-name-${participant.participantId}`}
    >{boundedName(participant.name)}</span
  >
  {#if view.reachability === 'source_unreachable'}
    <span data-testid={`participant-unreachable-${participant.participantId}`}>
      — {PARTICIPANT_TEXT.unreachable}
    </span>
  {/if}
  {#if serverMutedAny && view.serverMutedByName !== undefined}
    <span data-testid={`participant-server-muted-${participant.participantId}`}>
      {#each MEDIA_KINDS as kind (kind)}
        {#if view.serverMuted[kind]}— {serverMutedByText(view.serverMutedByName, kind)}{/if}
      {/each}
    </span>
  {/if}
  <MuteIndicators participantId={participant.participantId} indicators={view} />
  {#if hostControls}
    {#if unmuteRequested}
      <span data-testid={`unmute-requested-${participant.participantId}`}>
        — {PARTICIPANT_TEXT.asksToBeUnmuted}
      </span>
    {/if}
    {#each MEDIA_KINDS as kind (kind)}
      <button
        data-testid={`server-mute-${kind}-${participant.participantId}`}
        type="button"
        aria-pressed={view.serverMuted[kind]}
        onclick={() => toggleServerMute(kind)}
      >
        {view.serverMuted[kind]
          ? PARTICIPANT_TEXT.unmuteForEveryone[kind]
          : PARTICIPANT_TEXT.muteForEveryone[kind]}
      </button>
    {/each}
  {/if}
  {#if controlError}
    <span data-testid={`host-control-error-${participant.participantId}`}>{controlError}</span>
  {/if}
</li>
