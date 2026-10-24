<script lang="ts">
  // Story 3 R-18: the NON-SUPPRESSIBLE notice that a host's server unmute turned
  // this participant's camera or microphone back on.
  //
  // WHAT MAKES IT NON-SUPPRESSIBLE (security C-C1, binding):
  //   * It is rendered from the store's per-kind LATCH, which only the SDK's
  //     `captureResumedAfterServerMute` sets and only an explicit acknowledgement
  //     clears (`MediaStore.acknowledgeResume`). The latch lives in the store,
  //     not here, so a re-render, a grid rebuild or a re-mount of this component
  //     cannot drop a pending notice.
  //   * There is NO timeout or auto-hide, NO close on Escape or click-outside,
  //     and NO prop, setting, env var or query parameter that hides it. This
  //     component takes no option that could.
  //   * Two kinds are two notices; neither replaces the other.
  //   * The inline action client-mutes THAT kind AT ONCE (no confirm step), then
  //     acknowledges. "OK" only acknowledges.
  //   * `role="alertdialog"` + `aria-live="assertive"`, fixed above everything
  //     else (it can never render under the video grid).
  import type { PublishKind } from '@darktower/sdk-core';
  import type { MediaStore } from '@darktower/sdk-svelte';
  import { MEDIA_KINDS } from '../lib/participantState.js';

  let {
    media,
    clientMute,
  }: {
    media: MediaStore;
    /** Client-mute `kind` NOW (the SDK's `setAudioMuted` / `setVideoMuted(true)`). */
    clientMute: (kind: PublishKind) => void;
  } = $props();

  const TEXT = {
    video: {
      body: 'The host turned your camera back on.',
      mute: 'Turn my camera off now',
    },
    audio: {
      body: 'The host turned your microphone back on.',
      mute: 'Mute my microphone now',
    },
  } as const;

  function muteNow(kind: PublishKind): void {
    clientMute(kind);
    media.acknowledgeResume(kind);
  }
</script>

<div class="notices">
  {#each MEDIA_KINDS as kind (kind)}
    {#if media.resumeNoticePending(kind)}
      <div
        class="notice"
        role="alertdialog"
        aria-live="assertive"
        aria-labelledby={`resume-notice-text-${kind}`}
        data-testid={`resume-notice-${kind}`}
      >
        <p id={`resume-notice-text-${kind}`}>{TEXT[kind].body}</p>
        <button
          type="button"
          data-testid={`resume-notice-mute-${kind}`}
          onclick={() => muteNow(kind)}>{TEXT[kind].mute}</button
        >
        <button
          type="button"
          data-testid={`resume-notice-ack-${kind}`}
          onclick={() => media.acknowledgeResume(kind)}>OK</button
        >
      </div>
    {/if}
  {/each}
</div>

<style>
  .notices {
    position: fixed;
    top: 1rem;
    left: 50%;
    transform: translateX(-50%);
    z-index: 2147483647;
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }
  .notice {
    background: #fff4c2;
    border: 2px solid #a66a00;
    padding: 0.75rem 1rem;
    box-shadow: 0 2px 8px rgb(0 0 0 / 30%);
  }
</style>
