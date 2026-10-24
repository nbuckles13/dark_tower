<script lang="ts">
  // R-29 view 4: meeting-join. The meeting code and the PUBLISH-SET CHOOSER
  // (story 3 R-2): what this participant publishes — microphone, camera, both or
  // neither — is fixed for the membership, so it is chosen HERE, before the
  // session exists. Each submit mounts a fresh `MeetingRoom` (one single-use
  // `MeetingSession` per attempt; re-submit to retry).
  //
  // The chooser starts from the SDK's own default (`DEFAULT_PUBLISH`, never a
  // restated literal), or — in a test-levers build only — from this context's
  // `publish` lever; the CHOOSER's result is what the join uses either way.
  // A choice that would neither publish nor receive is refused by the SDK at
  // construction and shown through `errorText` — never a silent empty meeting.
  //
  // The local debug panel (story 3 R-7 self-check) mounts in the IDLE branch
  // only, so submitting Join unmounts it and releases any preview capture it
  // opened before the session acquires the camera.
  import type { DemoConfig } from '../lib/config.js';
  import type { AuthSession } from '../lib/types.js';
  import { DEFAULT_PUBLISH, type PublishKind } from '@darktower/sdk-core';
  import { publishSeed, readTestLeversForSession } from '../lib/testLevers.js';
  import DebugPanel from './DebugPanel.svelte';
  import MeetingRoom from './MeetingRoom.svelte';

  let {
    config,
    auth,
    onSessionInvalid,
  }: {
    config: DemoConfig;
    auth: AuthSession;
    /**
     * Raised when GC rejects the retained token (401 — see `isSessionRejection`; a
     * 403 is an authorization decision on a VALID token and must NOT drop it). The
     * shell drops the dead session, which restores the sign-in affordance.
     */
    onSessionInvalid: () => void;
  } = $props();

  // Story 2 task 15: the per-context test levers, read ONCE here into a frozen
  // copy — `undefined` in every build without `DT_TEST_LEVERS=1`, where the read
  // is statically dead. A malformed lever is a loud, visible refusal to join,
  // never a silent fallback to the production wiring.
  const { levers, error: leverError = '' } = readTestLeversForSession();
  const seed: readonly PublishKind[] = publishSeed(levers) ?? DEFAULT_PUBLISH;

  let meetingCode = $state('');
  let publishMicrophone = $state(seed.includes('audio'));
  let publishCamera = $state(seed.includes('video'));
  /** The current attempt; a new id re-mounts `MeetingRoom` (a fresh single-use session). */
  let attempt = $state<
    | { readonly id: number; readonly code: string; readonly publish: readonly PublishKind[] }
    | undefined
  >(undefined);
  let nextAttemptId = 0;

  function submit(event: Event): void {
    event.preventDefault();
    if (leverError) return;
    const publish: PublishKind[] = [];
    if (publishMicrophone) publish.push('audio');
    if (publishCamera) publish.push('video');
    nextAttemptId += 1;
    attempt = { id: nextAttemptId, code: meetingCode, publish };
  }
</script>

<form onsubmit={submit}>
  <h2>Join meeting</h2>
  <label>Meeting code<input data-testid="meeting-code" bind:value={meetingCode} /></label>
  <fieldset data-testid="publish-chooser">
    <legend>What you share (fixed for this meeting)</legend>
    <label
      ><input type="checkbox" data-testid="publish-microphone" bind:checked={publishMicrophone} />
      Microphone</label
    >
    <label
      ><input type="checkbox" data-testid="publish-camera" bind:checked={publishCamera} />
      Camera</label
    >
  </fieldset>
  <button data-testid="join-button" type="submit">Join</button>
</form>

{#if attempt !== undefined}
  {#key attempt.id}
    <MeetingRoom
      {config}
      {auth}
      meetingCode={attempt.code}
      publish={attempt.publish}
      {levers}
      {onSessionInvalid}
    />
  {/key}
{:else}
  <p data-testid="meeting-state">idle</p>
  <ul data-testid="participant-list"></ul>
  {#if leverError}
    <p data-testid="last-error">{leverError}</p>
  {/if}
  <DebugPanel />
{/if}
