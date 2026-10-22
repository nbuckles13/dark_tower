// File: packages/sdk-core/src/media/setup/meetingSenderId.ts
//
// THE ONE range check for a meeting-scoped `sender_id` used as a test-source
// watermark (story 2 R-7 test tone, story 3 R-27 test pattern).
//
// `sender_id` is allocated per meeting by MC and fills the key id's 16-bit
// sender field (`JoinResponse.sender_id`), so a watermark derived from it links
// nothing across meetings. 0 is the proto's reserved-invalid value. Both
// generators refuse anything outside `1..=65535` rather than fall back to a
// default that could collide with a real participant's watermark.

/** Largest legal sender id: the key id allots it 16 bits (`JoinResponse.sender_id`). */
export const MAX_MEETING_SENDER_ID = 0xffff;

/**
 * Assert `senderId` is an assigned meeting sender id, an integer in
 * `1..=`{@link MAX_MEETING_SENDER_ID}.
 *
 * @param what names the derived watermark in the (bounded) error message.
 * @throws {RangeError} for 0, out-of-range, non-integer or non-finite input.
 */
export function assertMeetingSenderId(senderId: number, what: string): void {
  if (!Number.isInteger(senderId) || senderId < 1 || senderId > MAX_MEETING_SENDER_ID) {
    throw new RangeError(
      `a ${what} is derived from an assigned sender id (1..65535); refusing to use a default`,
    );
  }
}
