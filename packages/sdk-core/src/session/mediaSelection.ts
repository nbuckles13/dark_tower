// File: packages/sdk-core/src/session/mediaSelection.ts
//
// Two decisions `MeetingSession.startMedia` makes before it builds a pipeline,
// kept out of the session class so each is one small, separately testable
// function (story 2 R-1, R-7, R-23):
//
//   * WHICH RECEIVE SLOTS TO DECLARE — N against the cap MC advertised;
//   * WHAT FEEDS THE SEND PATH — the one capture-source substitution point.

import type { MediaMetrics } from '../media/setup/mediaMetrics.js';
import { createMicrophoneCapture, createTestToneCapture } from '../media/setup/capture.js';
import { testToneFrequencyHz } from '../media/setup/testTone.js';
import type { CaptureSourceFactory } from '../media/setup/seams.js';
import type { ReceiveSlotsSource } from '../config/clientConfig.js';
import { SignalingError, SignalingErrorCode } from '../errors/SignalingError.js';
import type { CaptureSourceInfo, ReceiveSlotCap } from './events.js';

/** The default capture source. The ONE `microphone` literal on the emitting path. */
const MICROPHONE_SOURCE: CaptureSourceInfo = { mode: 'microphone' };

/**
 * Classify `JoinResponse.max_receive_slots` as received (`undefined` = absent).
 * See {@link ReceiveSlotCap}; the proto comment is the canonical statement.
 */
export function classifySlotCap(advertised: number | undefined): ReceiveSlotCap {
  if (advertised === undefined) return { state: 'unknown' };
  if (advertised === 0) return { state: 'invalid', value: 0 };
  return { state: 'known', value: advertised };
}

/**
 * The slot ids to declare: `0..N-1`. THROWS (typed, counted) when N exceeds a
 * KNOWN cap — MC would reject the declaration WHOLE, so it is refused here,
 * before sending, and N is NEVER shrunk to fit (R-1: a participant who silently
 * hears fewer people than configured is the failure this prevents).
 *
 * A user-experience pre-check, not the authority: MC re-checks every
 * declaration. The cap bounds the TOTAL slot count of one declaration across
 * media kinds, so the comparison is on the full declared list.
 *
 * `unknown` (an older MC) and `invalid` (a present 0) do not refuse: the
 * declaration is sent and MC's rejection, which surfaces as a session `error`,
 * is the backstop. `invalid` is WARNed first. Log lines carry numbers and the
 * source token only — never the join response or the options.
 */
export function receiveSlotIdsToDeclare(args: {
  readonly count: number;
  readonly source: ReceiveSlotsSource;
  readonly cap: ReceiveSlotCap;
  readonly metrics: MediaMetrics;
}): number[] {
  const { count, source, cap, metrics } = args;
  const declared = Array.from({ length: count }, (_, i) => i);
  if (cap.state === 'invalid') {
    console.warn(
      '[dt-media-slots]',
      `MC advertised a receive-slot cap of ${cap.value}, which violates the JoinResponse ` +
        `contract; treating the cap as unknown and declaring N=${count}`,
    );
  }
  console.info(
    '[dt-media-slots]',
    `declaring N=${count} receive slots (${source}); server cap ` +
      (cap.state === 'unknown' ? 'unknown' : String(cap.value)) +
      (cap.state === 'invalid' ? ' (invalid)' : ''),
  );
  if (cap.state === 'known' && count > cap.value) {
    metrics.receiveSlotsRejected();
    throw new SignalingError(
      SignalingErrorCode.ReceiveSlotsOverCap,
      `this client is configured to receive ${count} audio slots, but the meeting controller ` +
        `allows at most ${cap.value}; the controller would reject the whole declaration, so ` +
        `none was sent`,
    );
  }
  return declared;
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
