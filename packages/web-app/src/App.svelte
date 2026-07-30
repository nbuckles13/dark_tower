<script lang="ts">
  // R-29: four-view SPA shell. Holds the active view + the in-memory authenticated
  // session (never persisted — R-23) and routes to one view at a time.
  //
  // The auth nav is a SECURITY CONTROL, not UX polish (task #58 B3): once a session
  // exists, the Sign-up / Sign-in buttons are REMOVED from the DOM (not disabled or
  // hidden) and the auth views are unreachable, so a stale-credential remount cannot
  // happen. The inverse matters just as much — when the session is dropped, the auth
  // affordance must come back, or a user whose token dies is stranded with no way to
  // re-authenticate (F-SEC-3).
  import type { DemoConfig } from './lib/config.js';
  import type { AuthSession, View } from './lib/types.js';
  import SignUp from './views/SignUp.svelte';
  import SignIn from './views/SignIn.svelte';
  import CreateMeeting from './views/CreateMeeting.svelte';
  import JoinMeeting from './views/JoinMeeting.svelte';

  let { config }: { config: DemoConfig } = $props();

  let view = $state<View>('signup');
  let auth = $state<AuthSession | undefined>(undefined);

  // Effective view is BIDIRECTIONAL. Each direction closes a dead-end:
  //   authed   + auth-view    -> 'create'  (the control: auth views unreachable)
  //   unauthed + session-view -> 'signin'  (the recovery: after the session is
  //                                        dropped, land where re-auth happens)
  // Without the second direction, clearing `auth` while `view === 'join'` falls
  // through every branch below and renders an EMPTY <main> — a blank page on the
  // exact recovery path, which is a poor way for a security control to fail.
  const effectiveView = $derived(
    auth
      ? view === 'signup' || view === 'signin'
        ? 'create'
        : view
      : view === 'create' || view === 'join'
        ? 'signin'
        : view,
  );

  function onAuthed(session: AuthSession): void {
    auth = session;
    view = 'create';
  }

  /**
   * Drop the session when a `userToken`-bearing call is rejected with **401**.
   *
   * NOT 403: that is an authorization denial on a valid token (org limit, permissions,
   * external-participant policy) and must leave the session intact — see `isSessionRejection`.
   *
   * The security action and the UX recovery are the same edit: a token the server has
   * refused is a dead credential and should not stay in memory, and clearing it
   * restores the auth nav through the `{#if !auth}` gate below.
   */
  function onSessionInvalid(): void {
    auth = undefined;
    view = 'signin';
  }
</script>

<header>
  <h1>Dark Tower demo</h1>
  <nav>
    {#if !auth}
      <button data-testid="nav-signup" onclick={() => (view = 'signup')}>Sign up</button>
      <button data-testid="nav-signin" onclick={() => (view = 'signin')}>Sign in</button>
    {:else}
      <button data-testid="nav-create" onclick={() => (view = 'create')}>Create</button>
      <button data-testid="nav-join" onclick={() => (view = 'join')}>Join</button>
    {/if}
  </nav>
</header>

<main>
  {#if effectiveView === 'signup'}
    <SignUp {config} {onAuthed} />
  {:else if effectiveView === 'signin'}
    <SignIn {config} {onAuthed} />
  {:else if effectiveView === 'create' && auth}
    <!-- `&& auth` is unreachable-but-necessary: it narrows the {auth} prop for TS. -->
    <CreateMeeting {config} {auth} onGoJoin={() => (view = 'join')} {onSessionInvalid} />
  {:else if effectiveView === 'join' && auth}
    <JoinMeeting {config} {auth} {onSessionInvalid} />
  {/if}
</main>
