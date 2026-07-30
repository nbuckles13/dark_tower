<script lang="ts">
  // R-29 view 1: sign-up (email / password / displayName / orgSubdomain).
  import type { DemoConfig } from '../lib/config.js';
  import type { AuthSession } from '../lib/types.js';
  import { buildAuthClient } from '../lib/session.js';
  import { errorText } from '../lib/errorText.js';

  let { config, onAuthed }: { config: DemoConfig; onAuthed: (session: AuthSession) => void } =
    $props();

  let email = $state('');
  let password = $state('');
  let displayName = $state('');
  let subdomain = $state('demo');
  let error = $state('');
  let busy = $state(false);

  async function submit(event: Event): Promise<void> {
    event.preventDefault();
    error = '';
    busy = true;
    try {
      const client = buildAuthClient(config);
      const resp = await client.register({ subdomain, email, password, displayName });
      // Defense in depth: drop this component's reference to the password the moment
      // the token exchange succeeds. JS strings are immutable, so this narrows the
      // window rather than erasing the value — the original survives until GC and may
      // persist in the DOM input value. The session below carries no credential at all,
      // which is the actual control; this is belt-and-braces on top of it.
      password = '';
      onAuthed({ subdomain, displayName, userToken: resp.accessToken });
    } catch (err) {
      error = errorText(err);
    } finally {
      busy = false;
    }
  }
</script>

<form onsubmit={submit}>
  <h2>Sign up</h2>
  <label>Email<input data-testid="email" type="email" bind:value={email} /></label>
  <label>Password<input data-testid="password" type="password" bind:value={password} /></label>
  <label>Display name<input data-testid="display-name" bind:value={displayName} /></label>
  <label>Org subdomain<input data-testid="org-subdomain" bind:value={subdomain} /></label>
  <button data-testid="create-account-button" type="submit" disabled={busy}>Create account</button>
  {#if error}<p data-testid="last-error">{error}</p>{/if}
</form>
