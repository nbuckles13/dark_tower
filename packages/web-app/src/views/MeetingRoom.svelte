<script lang="ts">
  // ONE join attempt (story 3 C-C1): builds the single-use `MeetingSession` for
  // the join view's chosen publish set, binds it, joins, and renders the roster
  // and the in-meeting view.
  //
  // WHY A COMPONENT PER ATTEMPT. The publish set is FIXED at `MeetingSession`
  // construction (story 3 R-2), and `bindMeetingSession` must run during
  // component initialisation. So the chooser lives in `JoinMeeting.svelte`, and
  // each submit mounts a fresh `MeetingRoom` (keyed per attempt) that builds,
  // binds and joins at its own init. Unmounting it disconnects the session.
  //
  // CREDENTIALS. This component receives exactly what the join view held before
  // the split — the in-memory `AuthSession` (for the token join) — plus the
  // meeting code and the publish set. Nothing credential-bearing is copied into
  // component `$state`, the store or the bus: `credentials()` builds the join's
  // argument at call time from the prop, as before.
  import { onDestroy, onMount } from 'svelte';
  import type { DemoConfig } from '../lib/config.js';
  import type { AuthSession } from '../lib/types.js';
  import type { JoinCredentials, MeetingSession, PublishKind } from '@darktower/sdk-core';
  import { bindMeetingSession, type MeetingStore } from '@darktower/sdk-svelte';
  import { MeetingSessionState } from '@darktower/sdk-core';
  import { buildMeetingSession } from '../lib/session.js';
  import { installE2EHooks } from '../lib/e2eBus.js';
  import { createE2EInstrumentation } from '../lib/e2eAnalysis.js';
  import { errorText, isSessionRejection } from '../lib/errorText.js';
  import {
    hostControlsForced,
    leverViolationAfterJoin,
    type TestLevers,
  } from '../lib/testLevers.js';
  import InMeeting from './InMeeting.svelte';
  import ParticipantRow from './ParticipantRow.svelte';

  let {
    config,
    auth,
    meetingCode,
    publish,
    levers,
    onSessionInvalid,
  }: {
    config: DemoConfig;
    auth: AuthSession;
    meetingCode: string;
    /** The join's fixed publish set (the chooser's result). */
    publish: readonly PublishKind[];
    /** This context's frozen test levers (`undefined` outside a test-levers build). */
    levers: TestLevers | undefined;
    onSessionInvalid: () => void;
  } = $props();

  // Story 2 R-30: the receive-side instrumentation for the E2E bus — `undefined`
  // in production, where it is eliminated with the bus. Built BEFORE the session
  // so it can be injected into it.
  const e2e = createE2EInstrumentation();

  /** Build this attempt's session, or the bounded refusal text when the SDK refuses. */
  function build(): { readonly session?: MeetingSession; readonly refusal: string } {
    try {
      return { session: buildMeetingSession(config, { e2e, levers, publish }), refusal: '' };
    } catch (err) {
      // R-2: never a silent empty meeting — the SDK's refusal, through the one
      // bounded formatter.
      return { refusal: errorText(err) };
    }
  }
  const built = build();
  const session: MeetingSession | undefined = built.session;
  /** A refusal at construction (e.g. nothing to publish or receive), as bounded text. */
  const refusal = built.refusal;

  const store: MeetingStore | undefined = session ? bindMeetingSession(session) : undefined;
  // Returns a disposer because the bus runs a sampling timer in test builds (a
  // no-op function in production, where the whole bus is eliminated).
  const telemetryConfigured = (): boolean => config.telemetryEndpoint !== undefined;
  const disposeE2EHooks = session
    ? installE2EHooks(session, e2e, { telemetryConfigured: telemetryConfigured() })
    : () => {};
  onDestroy(() => {
    disposeE2EHooks();
    session?.disconnect();
  });

  let joinError = $state('');
  /** This participant's own id, from the settled join (MC's roster excludes self). */
  let selfParticipantId = $state<string | undefined>(undefined);
  /**
   * Whether to render host controls: the meeting-token role HINT (MC authorises
   * every request), or — in a test-levers build only — forced on so the suite can
   * drive a NON-host into MC's refusal.
   */
  let showHostControls = $state(false);

  function credentials(): JoinCredentials {
    // Join presents the token the app already holds. No re-authentication, so no
    // password is retained or re-transmitted. Built at call time, never stored.
    return {
      mode: 'token',
      userToken: auth.userToken,
      ...(auth.displayName ? { displayName: auth.displayName } : {}),
    };
  }

  onMount(() => {
    const s = session;
    if (s === undefined) return;
    void (async () => {
      try {
        const joined = await s.join({
          orgSubdomain: auth.subdomain,
          meetingCode,
          credentials: credentials(),
        });
        // The S10a non-vacuity guard (test-levers builds only).
        const violation = leverViolationAfterJoin(levers, joined.mediaServers);
        if (violation !== undefined) {
          s.disconnect();
          joinError = violation;
          return;
        }
        selfParticipantId = joined.participantId;
        showHostControls = s.isHost || hostControlsForced(levers);
      } catch (err) {
        joinError = errorText(err);
        if (isSessionRejection(err)) onSessionInvalid();
      }
    })();
  });

  const onSurface = e2e?.onSurface;
</script>

{#if session !== undefined && store !== undefined}
  <p data-testid="meeting-state">{store.meetingState}</p>
  <ul data-testid="participant-list">
    {#each store.participants as participant (participant.participantId)}
      <ParticipantRow
        {participant}
        {store}
        {session}
        {selfParticipantId}
        hostControls={showHostControls && store.meetingState === MeetingSessionState.Joined}
      />
    {/each}
  </ul>

  <!--
    The in-meeting experience appears AFTER a settled join, alongside the roster
    rather than replacing it: the DOM contract (`meeting-state`,
    `participant-list`, `last-error`) is what the browser suite and the dev
    runbook's triage both read.
  -->
  {#if store.meetingState === MeetingSessionState.Joined}
    <InMeeting {session} {store} {selfParticipantId} {onSurface} />
  {/if}

  {#if store.lastError}
    <p data-testid="last-error">{store.lastError.code}: {store.lastError.message}</p>
  {:else if joinError}
    <p data-testid="last-error">{joinError}</p>
  {/if}
{:else}
  <p data-testid="meeting-state">idle</p>
  <p data-testid="last-error" data-refused="true">{refusal}</p>
{/if}
