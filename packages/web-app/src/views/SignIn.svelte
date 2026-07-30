<script lang="ts">
  // R-29 view 2: sign-in (email / password / orgSubdomain).
  import type { DemoConfig } from '../lib/config.js';
  import type { AuthSession } from '../lib/types.js';
  import { buildAuthClient } from '../lib/session.js';
  import { errorText } from '../lib/errorText.js';

  let { config, onAuthed }: { config: DemoConfig; onAuthed: (session: AuthSession) => void } =
    $props();

  let email = $state('');
  let password = $state('');
  let subdomain = $state('demo');
  let error = $state('');
  let busy = $state(false);

  async function submit(event: Event): Promise<void> {
    event.preventDefault();
    error = '';
    busy = true;
    try {
      const client = buildAuthClient(config);
      const resp = await client.login({ subdomain, email, password });
      // Defense in depth — see the note in SignUp.svelte. Narrows the window; the
      // control is that the session carries no credential.
      password = '';
      // No displayName: AC's token response carries no identity fields, so sign-in
      // users join with an empty roster label. Pre-existing, unchanged by task #58.
      onAuthed({ subdomain, userToken: resp.accessToken });
    } catch (err) {
      error = errorText(err);
    } finally {
      busy = false;
    }
  }
</script>

<form onsubmit={submit}>
  <h2>Sign in</h2>
  <label>Email<input data-testid="email" type="email" bind:value={email} /></label>
  <label>Password<input data-testid="password" type="password" bind:value={password} /></label>
  <label>Org subdomain<input data-testid="org-subdomain" bind:value={subdomain} /></label>
  <button data-testid="signin-button" type="submit" disabled={busy}>Sign in</button>
  {#if error}<p data-testid="last-error">{error}</p>{/if}
</form>
