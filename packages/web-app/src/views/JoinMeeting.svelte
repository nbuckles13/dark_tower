<script lang="ts">
  // R-29 view 4: meeting-join. Drives MeetingSession.join and renders the live
  // roster from the sdk-svelte reactive store (existing participants +
  // ParticipantJoined/ParticipantLeft).
  //
  // The session is built + bound at component INIT (bindMeetingSession registers
  // `onDestroy`, which is only valid during init); `join()` runs on submit. One
  // join per mount (MeetingSession is single-use) — re-mount to retry.
  import { onDestroy } from 'svelte';
  import type { DemoConfig } from '../lib/config.js';
  import type { AuthResult } from '../lib/types.js';
  import type { JoinCredentials } from '@darktower/sdk-core';
  import { bindMeetingSession } from '@darktower/sdk-svelte';
  import { buildMeetingSession } from '../lib/session.js';
  import { installE2EHooks } from '../lib/e2eBus.js';
  import { errorText } from '../lib/errorText.js';

  let { config, auth }: { config: DemoConfig; auth: AuthResult } = $props();

  // svelte-ignore state_referenced_locally
  const session = buildMeetingSession(config);
  installE2EHooks(session);
  const store = bindMeetingSession(session);
  onDestroy(() => session.disconnect());

  let meetingCode = $state('');
  let joinError = $state('');
  let busy = $state(false);

  function credentials(): JoinCredentials {
    if (auth.mode === 'register') {
      return {
        mode: 'register',
        email: auth.email,
        password: auth.password,
        displayName: auth.displayName ?? '',
      };
    }
    return {
      mode: 'login',
      email: auth.email,
      password: auth.password,
      ...(auth.displayName ? { displayName: auth.displayName } : {}),
    };
  }

  async function submit(event: Event): Promise<void> {
    event.preventDefault();
    joinError = '';
    busy = true;
    try {
      await session.join({ orgSubdomain: auth.subdomain, meetingCode, credentials: credentials() });
    } catch (err) {
      joinError = errorText(err);
    } finally {
      busy = false;
    }
  }
</script>

<form onsubmit={submit}>
  <h2>Join meeting</h2>
  <label>Meeting code<input data-testid="meeting-code" bind:value={meetingCode} /></label>
  <button data-testid="join-button" type="submit" disabled={busy}>Join</button>
</form>

<p data-testid="meeting-state">{store.meetingState}</p>
<ul data-testid="participant-list">
  {#each store.participants as participant (participant.participantId)}
    <li data-testid={`participant-${participant.participantId}`}>{participant.name}</li>
  {/each}
</ul>

{#if store.lastError}
  <p data-testid="last-error">{store.lastError.code}: {store.lastError.message}</p>
{:else if joinError}
  <p data-testid="last-error">{joinError}</p>
{/if}
