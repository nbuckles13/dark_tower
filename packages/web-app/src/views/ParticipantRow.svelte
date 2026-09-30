<script lang="ts">
  // One roster row (story 2 R-10, R-11, R-33; ADR-0036 §5, §6, §9).
  //
  // Everything here is a wire fact rendered, never an inference from silence:
  // reachability from `unreachable_sender_ids`, server mute (and who applied it)
  // from `ParticipantMuteUpdate`, client mute from the slot carrying this
  // participant. The `data-*` attributes carry the TOKENS a test asserts on; the
  // text comes from `lib/participantState.ts`, the one home of this copy.
  //
  // The host affordance sends target + action only. The button's label follows
  // the WIRE state, not the click: after a click nothing here changes until MC's
  // `ParticipantMuteUpdate` arrives, so a refused request can never leave a
  // "muted" label beside an unmuted participant.
  import type { MeetingSession, RosterParticipant } from '@darktower/sdk-core';
  import type { MeetingStore } from '@darktower/sdk-svelte';
  import { errorText } from '../lib/errorText.js';
  import {
    PARTICIPANT_TEXT,
    participantIndicators,
    serverMutedByText,
  } from '../lib/participantState.js';

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
    participantIndicators(
      participant,
      {
        slots: store.media.slots,
        unreachableSenderIds: store.media.unreachableSenderIds,
        serverMutes: store.media.serverMutes,
      },
      { roster: store.participants, selfParticipantId },
    ),
  );
  const unmuteRequested = $derived(store.media.unmuteRequests.has(participant.participantId));

  async function toggleServerMute(): Promise<void> {
    controlError = '';
    try {
      await session.setServerMute(participant.participantId, !view.serverMuted);
    } catch (err) {
      controlError = errorText(err);
    }
  }
</script>

<li
  data-testid={`participant-${participant.participantId}`}
  data-reachability={view.reachability}
  data-server-muted={view.serverMuted}
  data-client-muted={view.clientMuted}
>
  <!-- The NAME alone, so the roster-name assertion stays exact beside the indicators. -->
  <span data-testid={`participant-name-${participant.participantId}`}>{participant.name}</span>
  {#if view.reachability === 'source_unreachable'}
    <span data-testid={`participant-unreachable-${participant.participantId}`}>
      — {PARTICIPANT_TEXT.unreachable}
    </span>
  {/if}
  {#if view.serverMuted && view.serverMutedByName !== undefined}
    <span data-testid={`participant-server-muted-${participant.participantId}`}>
      — {serverMutedByText(view.serverMutedByName)}
    </span>
  {/if}
  {#if view.clientMuted === 'true'}
    <span data-testid={`participant-client-muted-${participant.participantId}`}>
      — {PARTICIPANT_TEXT.mutedThemselves}
    </span>
  {/if}
  {#if hostControls}
    {#if unmuteRequested}
      <span data-testid={`unmute-requested-${participant.participantId}`}>
        — {PARTICIPANT_TEXT.asksToBeUnmuted}
      </span>
    {/if}
    <button
      data-testid={`server-mute-${participant.participantId}`}
      type="button"
      aria-pressed={view.serverMuted}
      onclick={toggleServerMute}
    >
      {view.serverMuted ? PARTICIPANT_TEXT.unmuteForEveryone : PARTICIPANT_TEXT.muteForEveryone}
    </button>
  {/if}
  {#if controlError}
    <span data-testid={`host-control-error-${participant.participantId}`}>{controlError}</span>
  {/if}
</li>
