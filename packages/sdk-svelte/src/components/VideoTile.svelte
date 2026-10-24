<script lang="ts" module>
  /**
   * The render surface of one tile, handed to an embedder that must INSPECT
   * what was painted (the demo's test-build-only evidence bus samples lumas
   * from it). Plain values: the canvas and a paint count. Never reactive.
   */
  export interface TileSurface {
    readonly slotId: number;
    readonly canvas: HTMLCanvasElement;
    /** Frames painted onto `canvas` so far (cumulative). */
    framesRendered(): number;
  }
</script>

<script lang="ts">
  // File: packages/sdk-svelte/src/components/VideoTile.svelte
  //
  // ONE video receive slot's render surface (story 3 C-C1; ADR-0036 §6, R-4,
  // R-5). It binds a canvas as the slot's `VideoSink` through the SDK's narrow
  // `VideoReceiveView.setVideoSink`, and the SDK calls `drawFrame` at playout.
  //
  // DECODED FRAMES NEVER TOUCH A STORE. `drawFrame` paints synchronously (the
  // SDK's BORROW contract: the frame is the SDK's, valid for the call only, and
  // the SDK closes it) and bumps a plain counter. Nothing reactive is written
  // per frame, so a frame re-renders nothing — the granularity proof in
  // `__tests__/MediaStore.test.ts` counts it.
  //
  // WHAT THE TILE SHOWS FOLLOWS THE SDK's RENDER STATE, never frame arrival:
  // the canvas while the slot renders `active` video, the embedder's avatar /
  // empty content (`children`) otherwise. A live slot with no frame yet keeps
  // the previous rendering because the SDK keeps the previous render state until
  // the first decode (R-5) — so there is no spinner for a steady state.
  //
  // NO LETTERBOX: the backing store is sized to the frame's display size and
  // CSS scales the element, so the drawn picture IS the whole backing store.
  import { untrack, type Snippet } from 'svelte';
  import type { VideoReceiveView, VideoSink } from '@darktower/sdk-core';
  import type { SlotView } from '../stores/mediaCells.svelte.js';

  let {
    slotId,
    view,
    slot,
    onSurface,
    children,
  }: {
    slotId: number;
    /** The session's video receive view; `undefined` until media has started. */
    view: VideoReceiveView | undefined;
    /** This slot's cells (`MediaStore.slot(slotId)`). */
    slot: SlotView;
    /** Optional: receive the render surface once mounted (and `undefined` on unmount). */
    onSurface?: ((surface: TileSurface | undefined) => void) | undefined;
    /** What to show when the slot is not rendering video (avatar / empty copy). */
    children?: Snippet;
  } = $props();

  let canvas: HTMLCanvasElement | undefined = $state(undefined);

  const showsVideo = $derived(slot.render?.kind === 'active');

  $effect(() => {
    const target = canvas;
    const receive = view;
    const id = slotId;
    if (target === undefined || receive === undefined) return;
    let painted = 0;
    let ctx: CanvasRenderingContext2D | null = null;
    const sink: VideoSink = {
      drawFrame(frame: VideoFrame): void {
        const width = frame.displayWidth;
        const height = frame.displayHeight;
        if (target.width !== width) target.width = width;
        if (target.height !== height) target.height = height;
        ctx ??= target.getContext('2d');
        // Loud, never a silent no-op: the SDK counts a throwing sink as a sink
        // fault and raises its fixed-text `mediaFault`.
        if (ctx === null) throw new Error('2d context unavailable');
        ctx.drawImage(frame, 0, 0, width, height);
        painted += 1;
      },
    };
    receive.setVideoSink(id, sink);
    // Untracked: the callback's identity is not a reason to re-bind the sink.
    const report = untrack(() => onSurface);
    report?.({ slotId: id, canvas: target, framesRendered: () => painted });
    return () => {
      receive.setVideoSink(id, undefined);
      report?.(undefined);
    };
  });
</script>

<div class="dt-video-tile" data-video-tile={slotId}>
  <canvas bind:this={canvas} class:dt-hidden={!showsVideo} data-testid={`video-canvas-${slotId}`}
  ></canvas>
  {#if !showsVideo}
    {@render children?.()}
  {/if}
</div>

<style>
  .dt-video-tile {
    position: relative;
  }
  canvas {
    display: block;
    width: 100%;
    height: auto;
    background: #000;
  }
  .dt-hidden {
    display: none;
  }
</style>
