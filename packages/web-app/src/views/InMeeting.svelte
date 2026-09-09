<script lang="ts">
  // The in-meeting experience (ADR-0036 story 1, §5/§6): the joined participant
  // sees its slot state, hears its own audio back through the media handler, and
  // can mute, unmute and choose a microphone.
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
  //     wire (§6);
  //   * "audio is coming back" reads `store.media.firstMediaFrameMs`;
  //   * a broken pipeline reads `store.media.lastMediaFault`, so a fault shows as
  //     a fault instead of as silence.
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
  import type { AudioPipeline, MeetingSession, StreamAssignmentEvent } from '@darktower/sdk-core';
  import { listMicrophones } from '@darktower/sdk-core';
  import type { MeetingStore } from '@darktower/sdk-svelte';
  import { errorText } from '../lib/errorText.js';
  import {
    AWAITING_ASSIGNMENT,
    slotIsActive,
    slotStateText,
    type SlotStateAttr,
  } from '../lib/slotState.js';

  let {
    session,
    store,
  }: {
    session: MeetingSession;
    store: MeetingStore;
  } = $props();

  /** The running pipeline. `undefined` until the participant starts audio. */
  let pipeline = $state<AudioPipeline | undefined>(undefined);
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
    // registry, which stops capture and darkens the microphone indicator). This
    // view only drops its reference.
    pipeline = undefined;
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

  async function startAudio(): Promise<void> {
    if (pipeline || starting) return;
    starting = true;
    mediaError = '';
    try {
      pipeline = await session.startMedia(
        selectedDeviceId !== '' ? { deviceId: selectedDeviceId } : {},
      );
      // Labels become readable only after permission is granted, so the picker
      // is re-read here rather than left showing placeholders for the session.
      await refreshMicrophones();
    } catch (err) {
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
      mediaError = 'Audio is not running, so there is nothing to mute.';
      return;
    }
    // Drive the SDK. The indicator does NOT update here; it updates when the
    // SDK echoes `muteChanged` back through the store. One direction in, one
    // direction out, so the indicator cannot disagree with what gates capture.
    running.setAudioMuted(!store.media.audioMuted);
  }

  async function changeMicrophone(event: Event): Promise<void> {
    const value = (event.currentTarget as HTMLSelectElement).value;
    selectedDeviceId = value;
    const running = pipeline;
    // Before audio starts the choice is simply remembered and applied by
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

  /**
   * Slot rows to render.
   *
   * Before MC sends any assignment there is still a slot — the client declared
   * one — so a single placeholder row is shown carrying `awaiting-assignment`.
   * Rendering nothing would make "the controller has not answered yet" look
   * identical to "there are no slots", which is the collapse §6 forbids.
   */
  const slotRows = $derived.by((): ReadonlyArray<{ id: number; state: SlotStateAttr }> => {
    const assignments: readonly StreamAssignmentEvent[] = store.media.slots;
    if (assignments.length === 0) return [{ id: 0, state: AWAITING_ASSIGNMENT }];
    return assignments.map((a) => ({ id: a.slotId, state: a.slotState }));
  });
</script>

<section data-testid="in-meeting">
  <h2>In meeting</h2>

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

  {#if !pipeline}
    <button data-testid="start-audio" type="button" onclick={startAudio} disabled={starting}>
      {starting ? 'Starting audio…' : 'Start audio'}
    </button>
  {:else}
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

  <p data-testid="media-status">
    {#if store.media.hasReceivedMedia}
      audio returning from the handler
    {:else if pipeline}
      waiting for audio to return
    {:else}
      audio not started
    {/if}
  </p>

  <ul data-testid="slot-list">
    {#each slotRows as row (row.id)}
      <li
        data-testid={`slot-${row.id}`}
        data-slot-state={row.state}
        data-slot-active={slotIsActive(row.state)}
      >
        {slotStateText(row.state)}
      </li>
    {/each}
  </ul>

  {#if store.media.lastMediaFault}
    <p data-testid="media-fault">{store.media.lastMediaFault.message}</p>
  {/if}
  {#if mediaError}
    <p data-testid="media-error">{mediaError}</p>
  {/if}
</section>
