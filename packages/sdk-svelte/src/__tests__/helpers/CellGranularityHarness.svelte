<script lang="ts">
  // Test harness for the story-3 RE-RENDER GRANULARITY property (C-C1): a slot's
  // state change recomputes only that tile, a participant's mute change only
  // their row, and a decoded frame nothing. Each `$derived` counts its own
  // recomputations; all are rendered, because an unread `$derived` never runs.
  import { bindMeetingSession } from '../../stores/bindMeetingSession.js';
  import type { BoundMeetingSession } from '../../stores/bindMeetingSession.js';
  import VideoTile, { type TileSurface } from '../../components/VideoTile.svelte';
  import type { VideoReceiveView } from '@darktower/sdk-core';

  let {
    session,
    counts,
    view,
    onSurface,
  }: {
    session: BoundMeetingSession;
    counts: { tile3: number; tile4: number; rowX: number; rowY: number };
    view?: VideoReceiveView;
    onSurface?: (surface: TileSurface | undefined) => void;
  } = $props();

  // svelte-ignore state_referenced_locally
  const store = bindMeetingSession(session);

  function tileText(slotId: number): string {
    const cell = store.media.slot(slotId);
    return `${cell.assignment?.slotState ?? 'none'}/${cell.render?.kind ?? 'none'}`;
  }
  const tile3 = $derived.by(() => {
    counts.tile3 += 1;
    return tileText(3);
  });
  const tile4 = $derived.by(() => {
    counts.tile4 += 1;
    return tileText(4);
  });
  function rowText(id: string): string {
    const p = store.media.participant(id);
    return `${p.audioServerMuted ? 'audio-server-muted' : 'audio-ok'} ${p.videoServerMuted ? 'video-server-muted' : 'video-ok'}`;
  }
  const rowX = $derived.by(() => {
    counts.rowX += 1;
    return rowText('x');
  });
  const rowY = $derived.by(() => {
    counts.rowY += 1;
    return rowText('y');
  });
</script>

<div data-testid="tile-3">{tile3}</div>
<div data-testid="tile-4">{tile4}</div>
<div data-testid="row-x">{rowX}</div>
<div data-testid="row-y">{rowY}</div>
{#if view}
  <VideoTile slotId={3} {view} slot={store.media.slot(3)} {onSurface} />
{/if}
