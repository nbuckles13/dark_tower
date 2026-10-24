<script lang="ts">
  // LOCAL DEBUG OPTIONS (story 3 R-7, R-35), hidden behind one toggle that is
  // CLOSED by default:
  //
  //   * the R-7 camera/microphone LOOPBACK self-check (`lib/selfCheck.ts`) —
  //     started only by an explicit "Start self-check" press; in a meeting it
  //     shows the session's OWN tracks and never stops them, before join it
  //     opens (and always releases) its own preview capture;
  //   * the R-35 statistics overlay (`lib/debugStats.ts`) — per receive slot
  //     and per sent stream, sampled from the SDK's local snapshot on ONE timer
  //     that runs only while the panel is open, rates computed here from the
  //     previous sample.
  //
  // LOCAL ONLY AND NEVER EXPORTED: this component and its helpers import no
  // MetricsSink, no telemetry module and no test bus, write nothing to the
  // console or to globals, and send nothing (enforced by
  // `tests/debugNeverExported.test.ts`). The per-slot bit rates are per-stream
  // size series (ADR-0036 §11). Only the panel's open/closed bit is persisted
  // (`lib/debugToggle.ts`); the self-check never auto-starts.
  //
  // PROPS ARE NARROW CLOSURES, never the session: `stats` and `tracks` are
  // absent before join (the self-check then uses its own preview capture, and
  // there is no overlay). Every other prop is a test seam.
  import { onDestroy } from 'svelte';
  import type { LocalCaptureTracks, LocalMediaStats } from '@darktower/sdk-core';
  import {
    DEBUG_STATS_SAMPLE_INTERVAL_MS,
    formatRate,
    statsView,
    type StatsSample,
    type StatsView,
  } from '../lib/debugStats.js';
  import {
    readDebugPanelOpen,
    writeDebugPanelOpen,
    type ToggleStorage,
  } from '../lib/debugToggle.js';
  import {
    SelfCheck,
    streamOfTrack,
    type SelfCheckDeps,
    type SelfCheckView,
  } from '../lib/selfCheck.js';

  let {
    stats = undefined,
    tracks = undefined,
    clock = () => performance.now(),
    setInterval: setI = (fn: () => void, ms: number) => setInterval(fn, ms),
    clearInterval: clearI = (h: ReturnType<typeof setInterval>) => clearInterval(h),
    storage = undefined,
    selfCheckDeps = {},
  }: {
    /** The session's local statistics snapshot (`localStatsOf(session)`). In a meeting only. */
    stats?: (() => LocalMediaStats | undefined) | undefined;
    /** The session's own capture tracks (`localCaptureTracksOf(session)`). In a meeting only. */
    tracks?: (() => LocalCaptureTracks) | undefined;
    /** Test seams. */
    clock?: () => number;
    setInterval?: (fn: () => void, ms: number) => ReturnType<typeof setInterval>;
    clearInterval?: (handle: ReturnType<typeof setInterval>) => void;
    storage?: ToggleStorage | undefined;
    selfCheckDeps?: Omit<SelfCheckDeps, 'tracks'>;
  } = $props();

  // svelte-ignore state_referenced_locally
  let open = $state(readDebugPanelOpen(storage));

  let check = $state<SelfCheckView>({
    status: 'idle',
    video: undefined,
    metering: false,
    meterFailed: false,
    level: undefined,
    failure: undefined,
  });
  // svelte-ignore state_referenced_locally
  const selfCheck = new SelfCheck(
    { ...selfCheckDeps, ...(tracks !== undefined ? { tracks } : {}) },
    (view) => {
      check = view;
    },
  );

  let view = $state<StatsView | undefined>(undefined);
  /** The SDK getter THREW (a defect) — shown as its own state, never as "media not started". */
  let statsFailed = $state(false);
  /** The ONE previous sample the rates are computed against. Never a history. */
  let previous: StatsSample | undefined;

  let videoEl = $state<HTMLVideoElement | null | undefined>(undefined);

  function toggle(): void {
    open = !open;
    writeDebugPanelOpen(open, storage);
    if (!open) selfCheck.stop();
  }

  function startCheck(): void {
    // `start()` reports every failure in its own view (status `failed`); it
    // does not reject. Nothing is swallowed here.
    void selfCheck.start();
  }

  function stopCheck(): void {
    selfCheck.stop();
  }

  function sample(): void {
    let snapshot: LocalMediaStats | undefined;
    try {
      snapshot = stats?.();
      statsFailed = false;
    } catch {
      snapshot = undefined;
      statsFailed = true;
    }
    if (snapshot === undefined) {
      previous = undefined;
      view = undefined;
      return;
    }
    const next: StatsSample = { atMs: clock(), stats: snapshot };
    view = statsView(previous, next);
    previous = next;
  }

  // The overlay timer exists ONLY while the panel is open and there is a stats
  // source; the effect's cleanup clears it on hide and on destroy.
  $effect(() => {
    if (!open || stats === undefined) return;
    previous = undefined;
    sample();
    const handle = setI(sample, DEBUG_STATS_SAMPLE_INTERVAL_MS);
    return () => {
      clearI(handle);
      previous = undefined;
      view = undefined;
    };
  });

  // Bind the self-check's camera track to the preview element (re-bound when the
  // session's track changes; cleared when there is none).
  $effect(() => {
    const el = videoEl;
    const track = check.video;
    // `bind:this` resets the element to null when the preview unmounts.
    if (!el) return;
    if (track === undefined) {
      el.srcObject = null;
      return;
    }
    const current = el.srcObject as MediaStream | null;
    if (current?.getVideoTracks()[0] === track) return;
    el.srcObject = (selfCheckDeps.streamOf ?? streamOfTrack)(track);
  });

  onDestroy(() => {
    // SYNCHRONOUS: an own preview capture is released before anything that
    // follows this view (e.g. the session's camera acquisition after Join).
    selfCheck.close();
  });
</script>

<section class="debug" data-testid="debug-panel-root">
  <button type="button" data-testid="debug-panel-toggle" aria-expanded={open} onclick={toggle}>
    {open ? 'Hide debug options' : 'Show debug options'}
  </button>

  {#if open}
    <div data-testid="debug-panel">
      <p>Local only: nothing in this panel is sent, recorded or reported.</p>

      <h3>Camera and microphone self-check</h3>
      <p>
        {tracks !== undefined
          ? 'Shows the camera and microphone this meeting is using.'
          : 'Opens your camera and microphone locally; they are released when you stop.'}
      </p>
      {#if check.status === 'running' || check.status === 'starting'}
        <button type="button" data-testid="self-check-stop" onclick={stopCheck}>
          Stop self-check
        </button>
      {:else}
        <button type="button" data-testid="self-check-start" onclick={startCheck}>
          Start self-check
        </button>
      {/if}
      <p data-testid="self-check-status" data-status={check.status}>{check.status}</p>
      {#if check.status === 'failed'}
        <p data-testid="self-check-failure" data-reason={check.failure}>
          Self-check failed: {check.failure}
        </p>
      {/if}
      {#if check.status === 'running'}
        <video
          data-testid="self-check-video"
          bind:this={videoEl}
          autoplay
          muted
          playsinline
          width="240"
        ></video>
        {#if check.video === undefined}
          <p data-testid="self-check-no-camera">no local camera track</p>
        {/if}
        {#if check.metering}
          <label
            >Microphone level <meter
              data-testid="self-check-level"
              min="0"
              max="1"
              value={check.level ?? 0}
            ></meter></label
          >
        {:else if check.meterFailed}
          <p data-testid="self-check-meter-failed">microphone meter unavailable</p>
        {:else}
          <p data-testid="self-check-no-microphone">no local microphone track</p>
        {/if}
      {/if}

      {#if stats !== undefined}
        <h3>Statistics</h3>
        {#if statsFailed}
          <p data-testid="debug-stats-error">statistics unavailable</p>
        {:else if view === undefined}
          <p data-testid="debug-stats-empty">media not started</p>
        {:else}
          <table data-testid="debug-stats-receive">
            <thead>
              <tr>
                <th>Slot</th><th>Packet rate</th><th>Bit rate</th><th>Queue</th><th>Decoded</th>
                <th>Keyframes seen</th><th>Drops</th><th>Unfulfilled</th>
              </tr>
            </thead>
            <tbody>
              {#each view.receive as row (row.key)}
                <tr data-testid={`debug-stats-slot-${row.key}`}>
                  <td>{row.kind} {row.slotId}</td>
                  <td data-testid="pkt-rate">{formatRate(row.packetsPerSec, 'pkt/s')}</td>
                  <td data-testid="bit-rate">{formatRate(row.bitsPerSec, 'kbit/s')}</td>
                  <td data-testid="queue-depth">{row.queueDepth}</td>
                  <td data-testid="decoded">{row.framesDecoded}</td>
                  <td data-testid="keyframes-seen">{row.keyframesSeen ?? '—'}</td>
                  <td data-testid="drops">
                    {#each row.drops as d (d.reason)}<span>{d.reason}: {d.count} </span>{/each}
                    {#if row.localDrops !== undefined && row.localDrops > 0}<span
                        >local: {row.localDrops}</span
                      >{/if}
                  </td>
                  <td data-testid="unfulfilled">
                    {row.unfulfilledMs === undefined ? '—' : `${row.unfulfilledMs} ms`}
                  </td>
                </tr>
              {/each}
            </tbody>
          </table>
          <table data-testid="debug-stats-send">
            <thead>
              <tr>
                <th>Sent</th><th>Packet rate</th><th>Bit rate</th><th>Keyframes by cause</th>
                <th>Groups abandoned</th>
              </tr>
            </thead>
            <tbody>
              {#each view.send as row (row.key)}
                <tr data-testid={`debug-stats-send-${row.key}`}>
                  <td>{row.kind}</td>
                  <td data-testid="pkt-rate">{formatRate(row.packetsPerSec, 'pkt/s')}</td>
                  <td data-testid="bit-rate">{formatRate(row.bitsPerSec, 'kbit/s')}</td>
                  <td data-testid="keyframes">
                    {#if row.keyframes === undefined}
                      —
                    {:else}
                      {#each row.keyframes as k (k.cause)}
                        <span>{`${k.cause}: ${k.count} `}</span>
                      {/each}
                    {/if}
                  </td>
                  <td data-testid="groups-abandoned">{row.groupsAbandoned ?? '—'}</td>
                </tr>
              {/each}
            </tbody>
          </table>
        {/if}
      {/if}
    </div>
  {/if}
</section>

<style>
  .debug {
    margin-top: 1rem;
    border-top: 1px dashed #999;
    padding-top: 0.5rem;
  }
  table {
    border-collapse: collapse;
    font-size: 0.85rem;
  }
  td,
  th {
    border: 1px solid #ccc;
    padding: 0.15rem 0.4rem;
  }
</style>
