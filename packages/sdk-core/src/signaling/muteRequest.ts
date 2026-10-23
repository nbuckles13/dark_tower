// File: packages/sdk-core/src/signaling/muteRequest.ts
//
// THE one builder for the per-kind fields of `MuteRequest` and
// `ServerMuteRequest` (story 3 R-15).
//
// ONE KIND PER REQUEST. The fields are `optional bool`, and MC reads an ABSENT
// kind as "unchanged" (signaling.proto, mute section): a request names exactly
// the kind it changes, and the other kind's field is left ABSENT -- never set
// to its current value (that would re-assert, and race, a state this client
// does not own for server mute) and never `false` (that would be an unmute).
// A request naming no kind is refused and counted by MC; this builder cannot
// produce one.

import type { MediaKindToken } from '../media/mediaKind.js';

/** The wire fields for a mute request about ONE kind; the other kind is absent. */
export type SingleKindMuteFields =
  | { readonly audioMuted: boolean; readonly videoMuted?: undefined }
  | { readonly audioMuted?: undefined; readonly videoMuted: boolean };

/** The wire fields for a request setting `kind` to `muted`, the other kind absent. */
export function singleKindMuteFields(kind: MediaKindToken, muted: boolean): SingleKindMuteFields {
  return kind === 'audio' ? { audioMuted: muted } : { videoMuted: muted };
}
