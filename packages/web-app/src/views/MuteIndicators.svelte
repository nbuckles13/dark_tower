<script lang="ts">
  // The FOUR mute indicators for one participant (story 3 R-15): client audio,
  // client video, server audio, server video — each its own element, its own
  // token (`data-muted`) and its own visibly distinct text + glyph, so "muted
  // their microphone" can never be mistaken for "the host turned the camera
  // off". Rendered from wire facts only (`lib/participantState.ts`).
  import {
    INDICATOR_TEXT,
    MEDIA_KINDS,
    type ParticipantIndicators,
  } from '../lib/participantState.js';

  let { participantId, indicators }: { participantId: string; indicators: ParticipantIndicators } =
    $props();

  /** Distinct glyphs per (cause, kind): client = person, server = shield. */
  const GLYPH = {
    client: { audio: '🎙', video: '📷' },
    server: { audio: '🛡🎙', video: '🛡📷' },
  } as const;
</script>

<span class="indicators" data-testid={`mute-indicators-${participantId}`}>
  {#each MEDIA_KINDS as kind (kind)}
    <span
      class="indicator client"
      data-testid={`mute-${kind}-client-${participantId}`}
      data-muted={indicators.clientMuted[kind]}
      ><span aria-hidden="true">{GLYPH.client[kind]}</span>
      {INDICATOR_TEXT.client[kind][indicators.clientMuted[kind]]}</span
    >
  {/each}
  {#each MEDIA_KINDS as kind (kind)}
    <span
      class="indicator server"
      class:active={indicators.serverMuted[kind]}
      data-testid={`mute-${kind}-server-${participantId}`}
      data-muted={indicators.serverMuted[kind]}
      ><span aria-hidden="true">{GLYPH.server[kind]}</span>
      {INDICATOR_TEXT.server[kind][indicators.serverMuted[kind] ? 'true' : 'false']}</span
    >
  {/each}
</span>

<style>
  .indicators {
    display: inline-flex;
    flex-wrap: wrap;
    gap: 0.25rem;
  }
  .indicator {
    font-size: 0.8rem;
    padding: 0 0.25rem;
    border-radius: 0.25rem;
  }
  .client {
    border: 1px dashed #666;
  }
  .server {
    border: 1px solid #666;
  }
  .server.active {
    background: #fdd;
    border-color: #a00;
  }
</style>
