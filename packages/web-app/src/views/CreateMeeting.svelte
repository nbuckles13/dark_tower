<script lang="ts">
  // R-29 view 3 / R-12: create-meeting (title + optional scheduledStart). Calls
  // MeetingApiClient.createMeeting and displays the returned meeting code for the
  // user to copy into the join view (in this or a second browser context).
  import type { DemoConfig } from '../lib/config.js';
  import type { AuthResult } from '../lib/types.js';
  import { buildMeetingClient } from '../lib/session.js';
  import { errorText } from '../lib/errorText.js';

  let {
    config,
    auth,
    onGoJoin,
  }: {
    config: DemoConfig;
    auth: AuthResult;
    onGoJoin: () => void;
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
