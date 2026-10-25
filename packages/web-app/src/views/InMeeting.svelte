<script lang="ts">
  // The in-meeting experience (ADR-0036 §5/§6/§9; story 2 R-2, R-10, R-11;
  // story 3 R-3, R-4, R-5, R-15, R-17, R-18, R-19): the joined participant sees
  // its N audio slots and an M-tile VIDEO GRID — each slot with the participant
  // MC placed there (a live video tile: the verified sender it renders) and the
  // slot's wire state, a participant's audio and video PAIRED BY SENDER — hears
  // and sees the others (never itself), sees whether the host has muted either
  // kind for everyone, and can mute, unmute and choose a microphone. A server
  // unmute that resumes capture raises the non-suppressible R-18 notice.
  //
  // -------------------------------------------------------------------------
  // ONE RULE GOVERNS THIS WHOLE COMPONENT
  // -------------------------------------------------------------------------
  //
  // Every "nothing is arriving" condition is rendered from an explicit signal
  // the SDK owns. Nothing here infers anything from frames not arriving:
  //
  //   * the mute indicator reads `store.media.audioMuted`, which is written ONLY
  //     by the SDK's `muteChanged` event — the same `MuteState` that gates
  //     capture. Not from the click (that would show what the user ASKED for),
  //     and not from frame counts (that inverts dangerously: a transport stall
  //     would display "muted" beside a live microphone);
  //   * far-end state reads `store.media.slots[].slotState`, straight off the
  //     wire (§6) — what a slot SHOWS. Another participant's own client mute is
  //     NOT read from slots: it is a roster fact in their participant cell
  //     (`lib/participantState.ts`; ADR-0036 §5 amendment 2026-10-24);
  //   * "audio is coming back" reads `store.media.firstMediaFrameMs`;
  //   * a broken pipeline reads `store.media.lastMediaFault`, so a fault shows as
  //     a fault instead of as silence;
  //   * this participant's SERVER mute reads `store.media.serverMutes` — written
  //     only from MC's `ParticipantMuteUpdate`. Asking the host to unmute changes
  //     nothing here (R-10); the indicator moves when MC says so.
  //
  // Client mute and server mute are DIFFERENT indicators (`mute-state` vs
  // `server-mute-state`) and compose independently: the client-mute toggle keeps
  // working while server-muted, and neither can clear the other.
  //
  // -------------------------------------------------------------------------
  // MEDIA IS STARTED EXPLICITLY
  // -------------------------------------------------------------------------
  //
  // `startMedia()` is a deliberate action, not a side effect of joining: it
  // opens the microphone, and it is the only lever resembling a kill switch for
  // the media path. So there is a button, and a join with no audio is a
  // supported state rather than an accident.
  import { onDestroy } from 'svelte';
  import type {
    AudioPipeline,
    MeetingSession,
    PublishKind,
    VideoReceiveView,
  } from '@darktower/sdk-core';
  import { listMicrophones, localCaptureTracksOf, localStatsOf } from '@darktower/sdk-core';
  import type { MeetingStore, TileSurface } from '@darktower/sdk-svelte';
  import { errorText } from '../lib/errorText.js';
  import {
    AWAITING_ASSIGNMENT,
    slotIsActive,
    slotStateText,
    type SlotStateAttr,
  } from '../lib/slotState.js';
  import {
    CAMERA_FAILURE_TEXT,
    PARTICIPANT_TEXT,
    SELF_CAMERA_TEXT,
    nameForParticipantId,
    nameForSenderId,
    selfCameraState,
    serverMutedByText,
  } from '../lib/participantState.js';
  import DebugPanel from './DebugPanel.svelte';
  import ResumeNotice from './ResumeNotice.svelte';
  import VideoSlotTile from './VideoSlotTile.svelte';

  let {
    session,
    store,
    selfParticipantId,
    onSurface,
  }: {
    session: MeetingSession;
    store: MeetingStore;
    /** This participant's id from the settled join (MC's roster excludes self). */
    selfParticipantId: string | undefined;
    /**
     * TEST BUILD ONLY: receives each video tile's render surface (the e2e bus
     * samples lumas from it). `undefined` in production (`e2eAnalysis.ts`).
     */
    onSurface?: ((surface: TileSurface | undefined, slotId: number) => void) | undefined;
  } = $props();

  /**
   * The declared slot ids per kind and the fixed publish set, from the
   * session's own diagnostics (static for the session; the SDK is the one
   * producer of slot ids — never restated here).
   */
  // svelte-ignore state_referenced_locally
  const declared = session.receiveSlots;
  const audioSlotIds = declared.audio.slotIds;
  const videoSlotIds = declared.video.slotIds;
  const publishes = (kind: PublishKind): boolean => declared.publish.includes(kind);

  /** This participant's own server mute, per kind, as MC last reported it. */
  const ownServerMute = $derived(
    selfParticipantId === undefined ? undefined : store.media.participant(selfParticipantId),
  );
  const ownServerMutedAny = $derived(
    ownServerMute !== undefined &&
      (ownServerMute.audioServerMuted || ownServerMute.videoServerMuted),
  );
  const ownServerMutedBy = $derived(
    nameForParticipantId(ownServerMute?.serverMutedBy, {
      roster: store.participants,
      selfParticipantId,
    }),
  );
  /**
   * Whether this participant has asked the host to unmute it — a local record of
   * the ASK only, scoped to ONE server-mute episode: cleared when MC reports
   * every kind lifted, so a later re-mute starts with nothing asked.
   */
  let unmuteAsked = $state(false);
  $effect(() => {
    if (!ownServerMutedAny) unmuteAsked = false;
  });

  /** R-19 / R-17: this participant's own camera, from SDK-reported state only. */
  const cameraState = $derived(
    selfCameraState({
      publishesVideo: publishes('video'),
      cameraFailure: store.media.cameraFailure,
      videoServerMuted: ownServerMute?.videoServerMuted ?? false,
      videoMuted: store.media.videoMuted,
    }),
  );

  async function askHostToUnmute(): Promise<void> {
    mediaError = '';
    try {
      await session.requestUnmute();
      unmuteAsked = true;
    } catch (err) {
      mediaError = errorText(err);
    }
  }

  /** The running pipeline. `undefined` until the participant starts media. */
  let pipeline = $state<AudioPipeline | undefined>(undefined);
  /** The session's video receive view, once media has started (M > 0). */
  let videoView = $state<VideoReceiveView | undefined>(undefined);
  let starting = $state(false);
  /** Bounded, SDK-authored text for a failure this view caused. Never a raw platform string. */
  let mediaError = $state('');
  /**
   * The microphone list, for the picker.
   *
   * `MediaDeviceInfo.label` is empty until permission has been granted — the
   * platform's behaviour, not a bug to work around — so the picker renders the
   * empty case rather than assuming labels exist, and the list is re-read after
   * `startMedia()` succeeds, which is when labels appear.
   *
   * PRIVACY: labels are user-identifying hardware strings and `deviceId` is a
   * stable per-origin identifier. Both are used HERE and nowhere else — they are
   * never logged, never put on the E2E bus, never placed in a `data-*`
   * attribute, never sent to the telemetry proxy, and never persisted.
   */
  let microphones = $state<readonly MediaDeviceInfo[]>([]);
  let selectedDeviceId = $state('');

  onDestroy(() => {
    // The session owns pipeline teardown (`disconnect()` disposes the media
    // registry, which stops capture and darkens the camera/microphone
    // indicators). This view only drops its references.
    pipeline = undefined;
    videoView = undefined;
  });

  async function refreshMicrophones(): Promise<void> {
    try {
      microphones = await listMicrophones();
    } catch (err) {
      // Enumeration failing is not fatal — the default device still works — but
      // it is not swallowed either, or a picker that is permanently empty looks
      // like "this machine has no microphones".
      mediaError = errorText(err);
    }
  }

  // Populate the picker as soon as the view appears, so a device can be chosen
  // BEFORE the microphone is opened. Labels will mostly be blank at this point;
  // that is the platform contract, and `optionLabel` renders it honestly.
  void refreshMicrophones();

  function optionLabel(device: MediaDeviceInfo, index: number): string {
    // Never invent a name that implies knowledge we do not have. An unlabelled
    // device gets a positional placeholder that reads as a placeholder.
    return device.label !== '' ? device.label : `Microphone ${index + 1}`;
  }

  async function startMedia(): Promise<void> {
    if (pipeline || starting) return;
    starting = true;
    mediaError = '';
    try {
      pipeline = await session.startMedia(
        selectedDeviceId !== '' ? { deviceId: selectedDeviceId } : {},
      );
      videoView = session.videoReceive;
      // Labels become readable only after permission is granted, so the picker
      // is re-read here rather than left showing placeholders for the session.
      await refreshMicrophones();
    } catch (err) {
      // R-8 / R-28: an unusable VP8 refuses media start AS A WHOLE (the SDK
      // throws and raises a fatal `mediaFault`, rendered below) — never an
      // audio-only degradation.
      mediaError = errorText(err);
    } finally {
      starting = false;
    }
  }

  function toggleMute(): void {
    const running = pipeline;
    if (!running) {
      // Loud rather than a silent `?.` no-op: a mute control that does nothing
      // is precisely the class of lie this view exists to avoid. (Unreachable
      // through the UI — the control only renders once `pipeline` exists — which
      // is why it states the invariant rather than trying to recover.)
      mediaError = 'Media is not running, so there is nothing to mute.';
      return;
    }
    // Drive the SDK. The indicator does NOT update here; it updates when the
    // SDK echoes `muteChanged` back through the store. One direction in, one
    // direction out, so the indicator cannot disagree with what gates capture.
    session.setAudioMuted(!store.media.audioMuted);
  }

  function toggleVideoMute(): void {
    if (!pipeline) {
      mediaError = 'Media is not running, so there is nothing to mute.';
      return;
    }
    // Client video mute STOPS the camera (R-17); the indicator follows the SDK.
    session.setVideoMuted(!store.media.videoMuted);
  }

  /** The R-18 notice's inline action: client-mute `kind` at once. */
  function clientMuteNow(kind: PublishKind): void {
    if (kind === 'audio') session.setAudioMuted(true);
    else session.setVideoMuted(true);
  }

  async function changeMicrophone(event: Event): Promise<void> {
    const value = (event.currentTarget as HTMLSelectElement).value;
    selectedDeviceId = value;
    const running = pipeline;
    // Before media starts the choice is simply remembered and applied by
    // `startMedia`. Once running, it is a REAL device swap — a picker whose
    // selection silently did nothing would be a masked failure wearing a UI
    // disguise.
    if (!running) return;
    try {
      await running.setCaptureDevice(value !== '' ? value : undefined);
    } catch (err) {
      mediaError = errorText(err);
    }
  }

  const nameCtx = $derived({ roster: store.participants, selfParticipantId });

  /** Each declared audio slot's row: ONE slot's cells, plus its pairing (R-3). */
  function audioRow(slotId: number): {
    state: SlotStateAttr;
    senderId: number | undefined;
    pairedVideo: number | undefined;
  } {
    const assignment = store.media.slot(slotId).assignment;
    const senderId = assignment?.senderId;
    return {
      state: assignment?.slotState ?? AWAITING_ASSIGNMENT,
      senderId,
      pairedVideo: senderId === undefined ? undefined : store.media.pairOf(senderId).videoSlotId,
    };
  }
</script>

<ResumeNotice media={store.media} clientMute={clientMuteNow} />

<section data-testid="in-meeting">
  <h2>In meeting</h2>

  {#if publishes('audio')}
    <div>
      <label for="mic-select">Microphone</label>
      <select
        id="mic-select"
        data-testid="mic-select"
        value={selectedDeviceId}
        onchange={changeMicrophone}
      >
        <option value="">System default</option>
        {#each microphones as device, index (device.deviceId)}
          <option value={device.deviceId}>{optionLabel(device, index)}</option>
        {/each}
      </select>
    </div>
  {/if}

  {#if !pipeline}
    <button data-testid="start-media" type="button" onclick={startMedia} disabled={starting}>
      {starting ? 'Starting media…' : 'Start media'}
    </button>
  {:else}
    {#if publishes('audio')}
      <!--
        `aria-pressed` and the visible text are fed by the SAME store value, so a
        screen reader and a sighted user cannot be told different things about
        whether the microphone is live.
      -->
      <button
        data-testid="mute-toggle"
        type="button"
        aria-pressed={store.media.audioMuted}
        onclick={toggleMute}
      >
        {store.media.audioMuted ? 'Unmute' : 'Mute'}
      </button>
      <p data-testid="mute-state" data-muted={store.media.audioMuted}>
        {store.media.audioMuted ? 'muted' : 'unmuted'}
      </p>
    {/if}
    {#if publishes('video')}
      <button
        data-testid="video-mute-toggle"
        type="button"
        aria-pressed={store.media.videoMuted}
        onclick={toggleVideoMute}
      >
        {store.media.videoMuted ? 'Turn camera on' : 'Turn camera off'}
      </button>
      <p data-testid="video-mute-state" data-muted={store.media.videoMuted}>
        {store.media.videoMuted ? 'camera off' : 'camera on'}
      </p>
    {/if}
  {/if}

  <!-- R-19 / R-17: this participant's own camera, with the failure REASON when it failed. -->
  <p data-testid="self-camera-state" data-camera-state={cameraState}>
    {SELF_CAMERA_TEXT[
      cameraState
    ]}{#if cameraState === 'failed' && store.media.cameraFailure !== undefined}:
      {CAMERA_FAILURE_TEXT[store.media.cameraFailure]}{/if}
  </p>

  <!--
    SERVER mute, PER KIND: separate indicators from the client ones, fed ONLY by
    MC's `ParticipantMuteUpdate`. Never described as a ban (R-10).
    (`server-mute-state` is the AUDIO indicator, the story-2 testid.)
  -->
  <p data-testid="server-mute-state" data-server-muted={ownServerMute?.audioServerMuted ?? false}>
    {#if ownServerMute?.audioServerMuted}
      {serverMutedByText(ownServerMutedBy, 'audio')}
    {:else}
      {PARTICIPANT_TEXT.ownNotServerMuted.audio}
    {/if}
  </p>
  <p
    data-testid="server-mute-state-video"
    data-server-muted={ownServerMute?.videoServerMuted ?? false}
  >
    {#if ownServerMute?.videoServerMuted}
      {serverMutedByText(ownServerMutedBy, 'video')}
    {:else}
      {PARTICIPANT_TEXT.ownNotServerMuted.video}
    {/if}
  </p>
  {#if ownServerMutedAny}
    <button data-testid="request-unmute" type="button" onclick={askHostToUnmute}>
      {unmuteAsked ? PARTICIPANT_TEXT.unmuteRequestSent : PARTICIPANT_TEXT.requestUnmute}
    </button>
  {/if}

  <p data-testid="media-status">
    {#if store.media.hasReceivedMedia}
      receiving media from other participants
    {:else if pipeline}
      no media from other participants yet
    {:else}
      media not started
    {/if}
  </p>

  <!--
    The AUDIO slots: one row per declared slot. An unfilled slot is an EMPTY row
    carrying its text — an under-filled grid, never a spinner (R-2). An
    unreachable participant takes no slot (§9); it is marked on its roster row.
    Each row names its participant's paired VIDEO slot (by sender, R-3).
  -->
  <ul data-testid="slot-list" class="slot-grid">
    {#each audioSlotIds as id (id)}
      {@const row = audioRow(id)}
      <li
        data-testid={`slot-${id}`}
        data-slot-state={row.state}
        data-slot-active={slotIsActive(row.state)}
        data-sender-id={row.senderId}
        data-paired-video-slot={row.pairedVideo}
      >
        {slotStateText(row.state, { name: nameForSenderId(row.senderId, nameCtx), kind: 'audio' })}
      </li>
    {/each}
  </ul>

  <!-- The M-tile VIDEO grid (story 3). One tile per declared video slot. -->
  {#if videoSlotIds.length > 0}
    <ul data-testid="video-grid" class="video-grid">
      {#each videoSlotIds as id (id)}
        <VideoSlotTile
          slotId={id}
          {store}
          view={videoView}
          {selfParticipantId}
          onSurface={onSurface === undefined ? undefined : (s) => onSurface(s, id)}
        />
      {/each}
    </ul>
  {/if}

  {#if store.media.lastMediaFault}
    <p data-testid="media-fault" data-fault-stage={store.media.lastMediaFault.stage}>
      {store.media.lastMediaFault.message}
    </p>
  {/if}
  {#if mediaError}
    <p data-testid="media-error">{mediaError}</p>
  {/if}
  <DebugPanel stats={() => localStatsOf(session)} tracks={() => localCaptureTracksOf(session)} />
</section>

<style>
  .slot-grid,
  .video-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(12rem, 1fr));
    gap: 0.5rem;
    list-style: none;
    padding: 0;
  }
  .slot-grid > li {
    border: 1px solid #999;
    padding: 0.5rem;
    min-height: 3rem;
  }
</style>
