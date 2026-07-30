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
  import type { AuthSession } from '../lib/types.js';
  import type { JoinCredentials } from '@darktower/sdk-core';
  import { bindMeetingSession } from '@darktower/sdk-svelte';
  import { buildMeetingSession } from '../lib/session.js';
  import { installE2EHooks } from '../lib/e2eBus.js';
  import { errorText, isSessionRejection } from '../lib/errorText.js';

  let {
    config,
    auth,
    onSessionInvalid,
  }: {
    config: DemoConfig;
    auth: AuthSession;
    /**
     * Raised when GC rejects the retained token (401 — see `isSessionRejection`; a
     * 403 is an authorization decision on a VALID token and must NOT drop it). The shell drops the dead
     * session, which restores the sign-in affordance — without this the user is
     * stranded holding a credential the server has already refused.
     */
    onSessionInvalid: () => void;
  } = $props();

  // svelte-ignore state_referenced_locally
  const session = buildMeetingSession(config);
  installE2EHooks(session);
  const store = bindMeetingSession(session);
  onDestroy(() => session.disconnect());

  let meetingCode = $state('');
  let joinError = $state('');
  let busy = $state(false);

  function credentials(): JoinCredentials {
    // Join presents the token the app already holds. No re-authentication, so no
    // password is retained or re-transmitted — and with no `mode` to carry, the
    // register-vs-login mismatch that caused AC's 409 is structurally impossible
    // rather than avoided by a band-aid.
    //
    // Conditional spread, not `displayName: auth.displayName`:
    // `exactOptionalPropertyTypes` rejects assigning `string | undefined` to `?:`.
    return {
      mode: 'token',
      userToken: auth.userToken,
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
      if (isSessionRejection(err)) onSessionInvalid();
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
