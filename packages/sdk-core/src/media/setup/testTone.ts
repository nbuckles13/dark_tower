// File: packages/sdk-core/src/media/setup/testTone.ts
//
// Test-tone FREQUENCY DERIVATION (story 2 R-7). Pure; no Web Audio here — the
// synthesis is `createTestToneCapture` in `./capture.ts`, the capture seam.
//
// ---------------------------------------------------------------------------
// THE ONLY INPUT IS THE MEETING-SCOPED SENDER ID
// ---------------------------------------------------------------------------
//
// A frequency derived from a DURABLE identity (user id, participant id, email,
// display name, identity key) would be an audible watermark linking a person
// across meetings. `sender_id` is allocated per meeting by MC, so a tone derived
// from it links nothing. The signature takes one number and nothing else, and
// there is no fallback: an absent or zero sender id (0 is the proto's
// reserved-invalid value, `JoinResponse.sender_id`) THROWS rather than playing a
// default frequency that could collide with a real participant's.
//
// ---------------------------------------------------------------------------
// SHARES NO CODE WITH THE RECEIVE-SIDE DETECTOR
// ---------------------------------------------------------------------------
//
// The detector that checks received audio for these tones is the test suite's
// (story 2 task 14, `packages/web-app/e2e/`), implemented independently so a
// shared bug cannot cancel out. It must not import this file; the harness reads
// each participant's expected tone off the E2E bus instead of restating the
// formula.
//
// ---------------------------------------------------------------------------
// THE BAND
// ---------------------------------------------------------------------------
//
// `f = TEST_TONE_BAND_START_HZ + ((senderId - 1) mod TEST_TONE_BUCKETS) *
// TEST_TONE_STEP_HZ`, so the band is [600, 1175] Hz:
//   * under one octave (2 x 600 > 1175), so no tone is a harmonic of another;
//   * inside Opus `voip`'s passband at the configured bitrate;
//   * 25 Hz apart — more than four analysis bins at fftSize 8192 / 48 kHz.
// Sender ids TEST_TONE_BUCKETS apart share a tone. That is inevitable (the id
// space is 16-bit) and is DETECTABLE, not silent: every participant's own tone is
// published on the E2E bus, so a harness treats a cohort collision as a
// precondition failure rather than as misrouting.

/** Lowest tone in the band, Hz. */
export const TEST_TONE_BAND_START_HZ = 600;
/** Spacing between adjacent tones, Hz. */
export const TEST_TONE_STEP_HZ = 25;
/** Distinct tones before the mapping wraps. */
export const TEST_TONE_BUCKETS = 24;
/** Highest tone in the band, Hz — derived, never restated. */
export const TEST_TONE_BAND_END_HZ =
  TEST_TONE_BAND_START_HZ + (TEST_TONE_BUCKETS - 1) * TEST_TONE_STEP_HZ;

/** Largest legal sender id: the key id allots it 16 bits (`JoinResponse.sender_id`). */
const MAX_SENDER_ID = 0xffff;

/**
 * The tone frequency for a participant, from its meeting-scoped sender id ONLY.
 *
 * @throws {RangeError} for anything but an integer in `1..=65535` — including
 * 0, the reserved-invalid sender id. Never a fallback frequency.
 */
export function testToneFrequencyHz(senderId: number): number {
  if (!Number.isInteger(senderId) || senderId < 1 || senderId > MAX_SENDER_ID) {
    throw new RangeError(
      'a test tone is derived from an assigned sender id (1..65535); refusing to play a default',
    );
  }
  return TEST_TONE_BAND_START_HZ + ((senderId - 1) % TEST_TONE_BUCKETS) * TEST_TONE_STEP_HZ;
}
