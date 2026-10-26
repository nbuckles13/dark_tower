// File: packages/sdk-core/src/signaling/muteRequest.ts
//
// THE one builder for the kind-keyed `changes` of `MuteRequest` and
// `ServerMuteRequest` (story 3 R-15; CONVENTIONS §6, ADR-0036 §5 amendment
// 2026-10-25).
//
// ONE KIND PER REQUEST. A kind ABSENT from `changes` is "unchanged" at MC
// (signaling.proto, mute section): a request names exactly the kind it
// changes, and never restates another kind's current value (that would
// re-assert, and race, a state this client does not own for server mute). An
// empty list is refused and counted by MC (`no_kind_named`); this builder
// cannot produce one. Closed wire kinds only (`wireMediaKind`).

import type { MediaKindToken } from '../media/mediaKind.js';
import type { MessageInitShape } from '@bufbuild/protobuf';
import type { MuteChangeSchema } from '../proto/dark_tower/signaling/v1/signaling_pb.js';
import { wireMediaKind } from './slotStateMap.js';

/** The `changes` list for a request setting `kind` to `muted`: exactly one entry. */
export function singleKindMuteChanges(
  kind: MediaKindToken,
  muted: boolean,
): [MessageInitShape<typeof MuteChangeSchema>] {
  return [{ mediaKind: wireMediaKind(kind), muted }];
}
