// File: packages/sdk-core/src/signaling/muteRequest.ts
//
// THE one builder for the per-kind fields of `MuteRequest` and
// `ServerMuteRequest` (story 3 R-15).
//
// UNTIL MC-B (story 3 task 15): BOTH kinds are set EXPLICITLY on every request,
// the unchanged kind carrying its CURRENT state. The fields are `optional bool`
// and will mean "absent = unchanged", but the controller in service reads an
// absent kind as `false` (signaling.proto, "INTERIM UNTIL MC-B"), so sending
// only the changed kind would UNMUTE the other one. Sending only the changed
// kind is MC-B's switch (lead ruling at P1 review, 2026-10-20). Never both
// absent: this builder cannot produce that.

/** One participant's mute state, both kinds. */
export interface PerKindMuteState {
  readonly audioMuted: boolean;
  readonly videoMuted: boolean;
}

/** The wire fields for a mute request: both kinds, explicitly. */
export function explicitMuteFields(state: PerKindMuteState): {
  audioMuted: boolean;
  videoMuted: boolean;
} {
  return { audioMuted: state.audioMuted, videoMuted: state.videoMuted };
}
