// File: packages/sdk-core/src/session/slotPlan.ts
//
// What a session PUBLISHES and DECLARES (story 3 R-1, R-2, R-23), resolved
// once at construction and frozen: the publish set, N audio and M video receive
// slots with their sources, the one receive declaration, and the per-kind caps
// the join reports. Its own object — never the options object — so the one
// JoinRequest builder (and, later, a reconnect) reads exactly these values.

import {
  DEFAULT_CLIENT_CONFIG,
  validateMediaConfig,
  type MediaConfig,
  type ReceiveSlotsSource,
} from '../config/clientConfig.js';
import type { PublishKind, ReceiveSlotDeclaration } from '../signaling/events.js';
import type { ReceiveSlotsDiagnostics, MeetingSessionOptions } from './events.js';
import { classifySlotCap, receiveDeclaration, validatePublishSet } from './mediaSelection.js';

/** See the module header. */
export class SlotPlan {
  /** The effective media configuration, N and M applied. */
  readonly mediaConfig: MediaConfig;
  /** Validated, copied and FROZEN (story 3 R-2). */
  readonly publish: readonly PublishKind[];
  /** Audio ids `0..N-1`, then video `N..N+M-1`; the one producer is `receiveDeclaration`. */
  readonly declaration: readonly ReceiveSlotDeclaration[];
  readonly #audioSource: ReceiveSlotsSource;
  readonly #videoSource: ReceiveSlotsSource;
  /** `JoinResponse.max_{audio,video}_receive_slots`, verbatim (`undefined` = absent). */
  #audioCap: number | undefined;
  #videoCap: number | undefined;

  /**
   * Resolve from session options. THROWS typed, before anything is used: an
   * out-of-ceiling N or M (`ClientConfigError`), a publish kind this build
   * cannot serve, or nothing to publish or receive (`SignalingError`).
   */
  constructor(options: MeetingSessionOptions) {
    const baseMedia = options.mediaConfig ?? DEFAULT_CLIENT_CONFIG.media;
    this.mediaConfig = {
      ...baseMedia,
      receive: {
        ...baseMedia.receive,
        ...(options.audioReceiveSlots ? { audioSlots: options.audioReceiveSlots.count } : {}),
        ...(options.videoReceiveSlots ? { videoSlots: options.videoReceiveSlots.count } : {}),
      },
    };
    validateMediaConfig(this.mediaConfig);
    const fallbackSource: ReceiveSlotsSource = options.mediaConfig ? 'configured' : 'default';
    this.#audioSource = options.audioReceiveSlots?.source ?? fallbackSource;
    this.#videoSource = options.videoReceiveSlots?.source ?? fallbackSource;
    const { audioSlots, videoSlots } = this.mediaConfig.receive;
    this.publish = validatePublishSet(options.publish ?? ['audio'], audioSlots, videoSlots);
    this.declaration = receiveDeclaration(audioSlots, videoSlots);
  }

  /**
   * Record the caps THIS session's join reported, and log one bounded line:
   * numbers and fixed tokens only, never the join response or options.
   */
  joined(audioCap: number | undefined, videoCap: number | undefined): void {
    this.#audioCap = audioCap;
    this.#videoCap = videoCap;
    const cap = (value: number | undefined): string =>
      value === undefined ? 'unknown' : String(value);
    const { audioSlots, videoSlots } = this.mediaConfig.receive;
    console.info(
      '[dt-media-slots]',
      `publishing [${this.publish.join(',')}]; declared N=${audioSlots} audio ` +
        `(${this.#audioSource}), M=${videoSlots} video (${this.#videoSource}); ` +
        `server caps audio ${cap(audioCap)}, video ${cap(videoCap)}` +
        (videoCap === 0 ? ' (video disabled)' : ''),
    );
  }

  /** The runtime-readable publish set, N, M, sources and per-kind caps (story 3 R-23). */
  diagnostics(): ReceiveSlotsDiagnostics {
    const { audioSlots, videoSlots } = this.mediaConfig.receive;
    return {
      publish: this.publish,
      audio: {
        declared: audioSlots,
        source: this.#audioSource,
        serverCap: classifySlotCap(this.#audioCap),
      },
      video: {
        declared: videoSlots,
        source: this.#videoSource,
        serverCap: classifySlotCap(this.#videoCap),
      },
    };
  }
}
