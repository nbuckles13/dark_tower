<script lang="ts">
  // R-29: four-view SPA shell. Holds the active view + the in-memory auth result
  // (credentials/token never persisted — R-23) and routes to one view at a time.
  import type { DemoConfig } from './lib/config.js';
  import type { AuthResult, View } from './lib/types.js';
  import SignUp from './views/SignUp.svelte';
  import SignIn from './views/SignIn.svelte';
  import CreateMeeting from './views/CreateMeeting.svelte';
  import JoinMeeting from './views/JoinMeeting.svelte';

  let { config }: { config: DemoConfig } = $props();

  let view = $state<View>('signup');
  let auth = $state<AuthResult | undefined>(undefined);

  function onAuthed(result: AuthResult): void {
    auth = result;
    view = 'create';
  }
</script>

<header>
  <h1>Dark Tower demo</h1>
  <nav>
    <button data-testid="nav-signup" onclick={() => (view = 'signup')}>Sign up</button>
    <button data-testid="nav-signin" onclick={() => (view = 'signin')}>Sign in</button>
    {#if auth}
      <button data-testid="nav-create" onclick={() => (view = 'create')}>Create</button>
      <button data-testid="nav-join" onclick={() => (view = 'join')}>Join</button>
    {/if}
  </nav>
</header>

<main>
  {#if view === 'signup'}
    <SignUp {config} {onAuthed} />
  {:else if view === 'signin'}
    <SignIn {config} {onAuthed} />
  {:else if view === 'create' && auth}
    <CreateMeeting {config} {auth} onGoJoin={() => (view = 'join')} />
  {:else if view === 'join' && auth}
    <JoinMeeting {config} {auth} />
  {/if}
</main>
