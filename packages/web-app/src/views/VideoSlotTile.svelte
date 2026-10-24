<script lang="ts">
  // One VIDEO receive slot in the grid (story 3 R-3, R-4, R-5): the SDK
  // adapter's `VideoTile` (the canvas the slot's `VideoSink` draws into) plus
  // who it shows and why.
  //
  //   * WHAT IT SHOWS follows the SDK's render state for the slot, which follows
  //     MC's slot state: live video, an avatar for MC's named participant with
  //     MC's reason, or empty (fewer sources / unavailable / fail-closed).
  //   * WHO a live tile shows is the VERIFIED key-id sender of what it renders
  //     (MC never names the live video source, R-5); an avatar's is MC's named
  //     participant. Resolved via the roster, bounded, text only.
  //   * PAIRED BY SENDER (R-3): the participant's audio slot, if any, is found
  //     through the pairing view by sender id — never by slot index — and its
  //     state is shown inline, so one participant's audio and video read as ONE
  //     participant.
  //   * `data-slot-state` is the RAW wire token; `data-rendered` is
  //     video | avatar | empty. Tests assert those, never the copy.
  import type { RosterParticipant, VideoReceiveView } from '@darktower/sdk-core';
  import { VideoTile, type MeetingStore, type TileSurface } from '@darktower/sdk-svelte';
  import {
    AWAITING_ASSIGNMENT,
    renderedKind,
    slotStateText,
    type SlotStateAttr,
  } from '../lib/slotState.js';
  import {
    mediaFactsFor,
    nameForSenderId,
    participantForSenderId,
    participantIndicators,
  } from '../lib/participantState.js';
  import MuteIndicators from './MuteIndicators.svelte';

  let {
    slotId,
    store,
    view,
    selfParticipantId,
    onSurface,
  }: {
    slotId: number;
    store: MeetingStore;
    view: VideoReceiveView | undefined;
    selfParticipantId: string | undefined;
    onSurface?: ((surface: TileSurface | undefined) => void) | undefined;
  } = $props();

  const cell = $derived(store.media.slot(slotId));
  const state = $derived<SlotStateAttr>(cell.assignment?.slotState ?? AWAITING_ASSIGNMENT);
  const rendered = $derived(renderedKind(cell.render?.kind));
  /** The sender this tile shows: the verified live sender, else MC's named participant. */
  const senderId = $derived(
    cell.render?.kind === 'active'
      ? cell.render.senderId
      : cell.render?.kind === 'avatar'
        ? cell.render.participantSenderId
        : cell.assignment?.senderId,
  );
  const ctx = $derived({ roster: store.participants, selfParticipantId });
  const name = $derived(nameForSenderId(senderId, ctx));
  const participant = $derived<RosterParticipant | undefined>(
    participantForSenderId(senderId, ctx),
  );
  const pairedAudioSlot = $derived(
    senderId === undefined ? undefined : store.media.pairOf(senderId).audioSlotId,
  );
  const audioState = $derived(
    pairedAudioSlot === undefined ? undefined : store.media.slot(pairedAudioSlot).assignment,
  );
  const indicators = $derived(
    participant === undefined
      ? undefined
      : participantIndicators(participant, mediaFactsFor(store.media, participant), ctx),
  );
</script>

<li
  class="tile"
  data-testid={`video-slot-${slotId}`}
  data-slot-state={state}
  data-rendered={rendered}
  data-participant-id={participant?.participantId}
  data-paired-audio-slot={pairedAudioSlot}
>
  <VideoTile {slotId} {view} slot={cell} {onSurface}>
    <p class="avatar" data-testid={`video-slot-text-${slotId}`}>
      {slotStateText(state, { name, kind: 'video' })}
    </p>
  </VideoTile>
  {#if rendered === 'video'}
    <p class="caption" data-testid={`video-slot-caption-${slotId}`}>
      {slotStateText('active', { name, kind: 'video' })}
    </p>
  {/if}
  {#if participant !== undefined}
    <p class="audio" data-testid={`video-slot-audio-${slotId}`}>
      {#if audioState !== undefined}
        audio: {slotStateText(audioState.slotState, { name, kind: 'audio' })}
      {:else}
        audio: not in one of your audio slots
      {/if}
    </p>
  {/if}
  {#if participant !== undefined && indicators !== undefined}
    <MuteIndicators participantId={participant.participantId} {indicators} />
  {/if}
</li>

<style>
  .tile {
    border: 1px solid #999;
    padding: 0.25rem;
    list-style: none;
  }
  .avatar {
    min-height: 6rem;
    display: flex;
    align-items: center;
    justify-content: center;
    background: #eee;
    margin: 0;
    padding: 0.5rem;
    text-align: center;
  }
  .caption,
  .audio {
    margin: 0.25rem 0 0;
    font-size: 0.9rem;
  }
</style>
