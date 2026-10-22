// File: packages/sdk-core/src/session/mediaSelection.ts
//
// The decisions `MeetingSession` makes about what it declares and what feeds
// its send path, kept out of the session class so each is one small,
// separately testable function (story 2 R-7; story 3 R-1, R-2, R-23):
//
//   * WHAT IT PUBLISHES — the publish set, validated once (`validatePublishSet`);
//   * WHICH RECEIVE SLOTS TO DECLARE — N audio then M video (`receiveDeclaration`,
//     THE ONE producer of slot ids);
//   * WHAT IT LEARNED OF MC's CAPS — per kind (`classifySlotCap`);
//   * WHAT FEEDS THE SEND PATH — the one capture-source substitution point.

import { createMicrophoneCapture, createTestToneCapture } from '../media/setup/capture.js';
import { testToneFrequencyHz } from '../media/setup/testTone.js';
import type { CaptureSourceFactory } from '../media/setup/seams.js';
import { SignalingError, SignalingErrorCode } from '../errors/SignalingError.js';
import type { PublishKind, ReceiveSlotDeclaration } from '../signaling/events.js';
import type { CaptureSourceInfo, ReceiveSlotCap } from './events.js';
import type { MediaKindToken } from '../media/mediaKind.js';

/** The default capture source. The ONE `microphone` literal on the emitting path. */
const MICROPHONE_SOURCE: CaptureSourceInfo = { mode: 'microphone' };

/**
 * The kinds this SDK build can PUBLISH. Video joins in story 3 C-A2 (task 13),
 * with its capture and encoder; until then a video request is refused typed,
 * never silently dropped from the set.
 */
const PUBLISHABLE_KINDS: ReadonlySet<PublishKind> = new Set<PublishKind>(['audio']);

/**
 * Validate and FREEZE a publish set (story 3 R-2). THROWS typed:
 *
 *   * `PublishKindNotSupported` — a kind this build cannot publish yet (video),
 *     a duplicate, or a value outside the closed vocabulary (possible only if a
 *     caller defeats the type system). MC rejects all three as well.
 *   * `NothingToPublishOrReceive` — nothing published and `audioSlots +
 *     videoSlots` is 0. A user-experience pre-check only: MC is the authority
 *     and refuses such a join itself.
 *
 * Returns a frozen COPY: the caller's array can never change what this
 * membership publishes.
 */
export function validatePublishSet(
  publish: readonly PublishKind[],
  audioSlots: number,
  videoSlots: number,
): readonly PublishKind[] {
  const seen = new Set<PublishKind>();
  for (const kind of publish) {
    if (!PUBLISHABLE_KINDS.has(kind) || seen.has(kind)) {
      throw new SignalingError(
        SignalingErrorCode.PublishKindNotSupported,
        kind === 'video'
          ? 'publishing video is not yet supported by this client'
          : seen.has(kind)
            ? `the publish set names ${kind} more than once`
            : 'the publish set names a media kind this client cannot publish',
      );
    }
    seen.add(kind);
  }
  if (seen.size === 0 && audioSlots + videoSlots === 0) {
    throw new SignalingError(
      SignalingErrorCode.NothingToPublishOrReceive,
      'this client is configured to publish nothing and to receive 0 audio and 0 video slots, ' +
        'so it would join a meeting it can neither hear nor be heard in',
    );
  }
  return Object.freeze([...publish]);
}

/**
 * The receive declaration: audio slot ids `0..N-1`, then video ids
 * `N..N+M-1`, IN ID ORDER, so MC's per-kind truncation keeps the low ids.
 * Either count may be 0. THE ONE producer of slot ids — the join-carried
 * declaration, the post-join redeclaration and the pipeline's declared audio ids
 * all come from here.
 */
export function receiveDeclaration(
  audioSlots: number,
  videoSlots: number,
): readonly ReceiveSlotDeclaration[] {
  const audio = Array.from({ length: audioSlots }, (_, i) => ({
    slotId: i,
    mediaKind: 'audio' as const,
  }));
  const video = Array.from({ length: videoSlots }, (_, i) => ({
    slotId: audioSlots + i,
    mediaKind: 'video' as const,
  }));
  return Object.freeze([...audio, ...video]);
}

/** The slot ids of one kind in a declaration, in id order. */
export function declaredSlotIds(
  declaration: readonly ReceiveSlotDeclaration[],
  kind: MediaKindToken,
): readonly number[] {
  return declaration.filter((s) => s.mediaKind === kind).map((s) => s.slotId);
}

/**
 * Classify a per-kind cap from `JoinResponse.max_{audio,video}_receive_slots`
 * as received (`undefined` = absent = unknown). 0 is LEGAL (a video cap of 0 is
 * the video-off lever, R-26) and is `known`, never folded into unknown.
 * Informational: MC truncates, so a client never shrinks its declaration.
 */
export function classifySlotCap(advertised: number | undefined): ReceiveSlotCap {
  if (advertised === undefined) return { state: 'unknown' };
  return { state: 'known', value: advertised };
}

/**
 * THE ONE capture-source substitution point (story 2 R-7).
 *
 * An embedder-injected factory is always respected, and reported as the
 * microphone (only the gated branch below may spell `test_tone`). Otherwise the
 * default is the microphone — unless this is a TEST-TONE BUILD:
 * `__DT_TEST_TONE__` is a Vite build-time define, `false` in every production
 * build and in the SDK's own library build, and opt-in (`DT_TEST_TONE=1`) in
 * dev. In a production bundle the branch is statically dead, so the tone
 * synthesis and the `test_tone` token are tree-shaken (proved by the
 * bundle-content tests). It is never a runtime option.
 *
 * The tone's frequency comes from the meeting-scoped `senderId` ONLY — the
 * caller has already refused to proceed without one — never from a durable
 * identity, and with no fallback frequency.
 */
export function selectCaptureSource(
  injected: CaptureSourceFactory | undefined,
  senderId: number,
): { readonly factory: CaptureSourceFactory; readonly info: CaptureSourceInfo } {
  if (injected) return { factory: injected, info: MICROPHONE_SOURCE };
  if (__DT_TEST_TONE__) {
    const toneHz = testToneFrequencyHz(senderId);
    return {
      factory: (options) =>
        createTestToneCapture({
          sampleRateHz: options.sampleRateHz,
          channels: options.channels,
          frequencyHz: toneHz,
        }),
      info: { mode: 'test_tone', toneHz },
    };
  }
  return { factory: createMicrophoneCapture, info: MICROPHONE_SOURCE };
}
