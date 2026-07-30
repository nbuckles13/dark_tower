<script lang="ts">
  // R-29 view 3 / R-12: create-meeting (title + optional scheduledStart). Calls
  // MeetingApiClient.createMeeting and displays the returned meeting code for the
  // user to copy into the join view (in this or a second browser context).
  import type { DemoConfig } from '../lib/config.js';
  import type { AuthSession } from '../lib/types.js';
  import { buildMeetingClient } from '../lib/session.js';
  import { errorText, isSessionRejection } from '../lib/errorText.js';

  let {
    config,
    auth,
    onGoJoin,
    onSessionInvalid,
  }: {
    config: DemoConfig;
    auth: AuthSession;
    onGoJoin: () => void;
    /** Raised when the retained token is rejected (401 only) — see `isSessionRejection`. */
    onSessionInvalid: () => void;
  } = $props();

  let title = $state('');
  let scheduledStart = $state('');
  let code = $state('');
  let error = $state('');
  let busy = $state(false);

  async function submit(event: Event): Promise<void> {
    event.preventDefault();
    error = '';
    code = '';
    busy = true;
    try {
      const client = buildMeetingClient(config);
      const input = {
        displayName: title,
        ...(scheduledStart ? { scheduledStartTime: new Date(scheduledStart).toISOString() } : {}),
      };
      const resp = await client.createMeeting(input, { userToken: auth.userToken });
      code = resp.meetingCode;
    } catch (err) {
      error = errorText(err);
      // A 401 here proves the retained token is dead, exactly as it does at join. A 403
      // does NOT — it is an authorization decision (e.g. org meeting limit) on a valid token.
      // The recovery is credential-scoped, not view-scoped: whichever call discovers
      // the dead credential drops it (task #58 F-SEC-3).
      if (isSessionRejection(err)) onSessionInvalid();
    } finally {
      busy = false;
    }
  }
</script>

<form onsubmit={submit}>
  <h2>Create meeting</h2>
  <label>Title<input data-testid="meeting-title" bind:value={title} /></label>
  <label
    >Scheduled start<input
      data-testid="scheduled-start"
      type="datetime-local"
      bind:value={scheduledStart}
    /></label
  >
  <button data-testid="create-button" type="submit" disabled={busy}>Create meeting</button>
  {#if code}
    <p>Meeting code: <strong data-testid="created-meeting-code">{code}</strong></p>
    <button type="button" onclick={onGoJoin}>Go to join</button>
  {/if}
  {#if error}<p data-testid="last-error">{error}</p>{/if}
</form>
