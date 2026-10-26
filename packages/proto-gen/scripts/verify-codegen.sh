#!/usr/bin/env bash
#
# verify-codegen.sh — smoke-test the TS proto codegen pipeline.
#
# Invariant we check: `buf generate` (run from `proto/`) produces non-empty
# `_pb.ts` files in `packages/sdk-core/src/proto/`, each containing the
# expected exported symbol from its source `.proto`. Catches:
#   - silent codegen plugin failure (exit 0 but no output)
#   - plugin/option drift that produces files with different names or strips
#     message classes (e.g. wrong target=, wrong import_extension, plugin swap) —
#     the clean in codegen.sh, before every generate, ensures stale outputs
#     from a prior config can't mask a broken current config (per @test Gate 3
#     finding 2026-05-06)
#   - wrong output directory layout
#
# READ-ONLY: it asserts over the output of the `codegen` target (its
# `dependsOn`) and never deletes or regenerates. Before 2026-10-25 it did both
# itself, which would race the parallel unit tests that import the same files
# (see codegen.sh). It was also not reached by any lane until 2026-10-25, which
# is how it sat red on the retired `publishKinds` name while Layers 1-6 passed.
#
# Invoked by the Nx target `proto-gen:test:unit` (cwd: packages/proto-gen), so it
# runs in Layer 4 (scripts/lang/ts/test.sh) and in CI (`pnpm test:unit`).
set -euo pipefail

# shellcheck source=./codegen-paths.sh
source "$(dirname "${BASH_SOURCE[0]}")/codegen-paths.sh"
OUT_DIR="${CODEGEN_OUT_DIR}"

assert_generated() {
  local file="$1"
  local symbol="$2"
  local path="${OUT_DIR}/${file}"

  if [[ ! -f "${path}" ]]; then
    echo "verify-codegen: FAIL — expected file not produced: ${path}" >&2
    exit 1
  fi
  if [[ ! -s "${path}" ]]; then
    echo "verify-codegen: FAIL — generated file is empty: ${path}" >&2
    exit 1
  fi
  if ! grep -q -- "${symbol}" "${path}"; then
    echo "verify-codegen: FAIL — generated file missing expected symbol '${symbol}': ${path}" >&2
    exit 1
  fi
  echo "verify-codegen: OK — ${file} (contains ${symbol})"
}

# Absence half of the oracle. Presence greps alone would stay green through an
# entire deletion set: the ADR-0036 reshape removes twelve symbols, and a
# half-done deletion or a rename-back must FAIL this script rather than pass it
# quietly (@test, Gate 1).
#
# Named `assert_not_generated`, deliberately NOT `assert_absent`:
# `scripts/lang/_test_helpers.sh:60` already defines that name with an
# incompatible contract (tally + `report_results` vs this script's fail-fast
# `exit 1`). Two helpers sharing a name would shadow silently if this script
# ever sourced that file, so it pairs with the local `assert_generated` instead.
#
# Matches the EXPORT DECLARATION, not a bare token, so a surviving mention in a
# doc comment cannot produce a false pass.
assert_not_generated() {
  local file="$1"
  local symbol="$2"
  local path="${OUT_DIR}/${file}"

  # Fail loudly on a missing file rather than passing open. Without this, a
  # missing/renamed target makes `grep -qE` return a file-error rc, the `if`
  # below reads it as "not found", and every absence assert prints a false
  # "OK — absent". Today only ordering saves it (a presence assert on the same
  # path runs first); the oracle must not depend on that (@test, Gate 3).
  if [[ ! -f "${path}" ]]; then
    echo "verify-codegen: FAIL — cannot check absence, file not produced: ${path}" >&2
    exit 1
  fi
  if grep -qE "^export (type|enum|const) ${symbol}\\b" "${path}"; then
    echo "verify-codegen: FAIL — deleted symbol '${symbol}' is still generated: ${path}" >&2
    exit 1
  fi
  echo "verify-codegen: OK — ${symbol} absent from ${file}"
}

# Member-scoped enum oracle (story 3). A reserved enum VALUE cannot be checked
# by `assert_not_generated`: the enum itself still exists, and a file-wide grep
# for the member name would also hit doc comments that cite it. These helpers
# extract ONE `export enum <Enum> {` ... `}` block and match a member
# DECLARATION (`  NAME = n,`) inside it only, so comment text never satisfies
# them. The extraction is checked by a POSITIVE CONTROL member that must be
# found in the same block, so a broken extraction (renamed enum, changed
# layout, empty block) fails loudly instead of reporting a false "absent".
enum_block() {
  local path="$1"
  local enum="$2"
  awk -v start="^export enum ${enum} [{]" '
    $0 ~ start { inside = 1 }
    inside { print }
    inside && /^}/ { exit }
  ' "${path}"
}

assert_enum_member() {
  local file="$1"
  local enum="$2"
  local member="$3"
  local path="${OUT_DIR}/${file}"
  if [[ ! -f "${path}" ]]; then
    echo "verify-codegen: FAIL — expected file not produced: ${path}" >&2
    exit 1
  fi
  if ! enum_block "${path}" "${enum}" | grep -qE "^[[:space:]]+${member} = [0-9]+,"; then
    echo "verify-codegen: FAIL — enum ${enum} has no member '${member}': ${path}" >&2
    exit 1
  fi
  echo "verify-codegen: OK — ${enum}.${member} in ${file}"
}

assert_enum_member_absent() {
  local file="$1"
  local enum="$2"
  local member="$3"
  local control="$4"
  local path="${OUT_DIR}/${file}"
  # Positive control first: the same extraction must find a member that IS
  # declared, or the absence below proves nothing.
  assert_enum_member "${file}" "${enum}" "${control}"
  if enum_block "${path}" "${enum}" | grep -qE "^[[:space:]]+${member} = [0-9]+,"; then
    echo "verify-codegen: FAIL — reserved '${enum}.${member}' still generated: ${path}" >&2
    exit 1
  fi
  echo "verify-codegen: OK — ${enum}.${member} absent from ${file}"
}

# Member-scoped MESSAGE-FIELD oracle (story 3 MC-B). A deleted FIELD cannot be
# checked by `assert_not_generated` (which matches only export declarations, so
# it passes vacuously on a field name) nor by a file-wide grep (`transportMode`
# legitimately survives on `SourceStream`/`SourceStreamEcho`, and doc comments
# cite deleted names). These helpers extract ONE message's generated type --
# `export type <Msg> = Message<...> & {` ... `};` -- AND its JSON twin
# `export type <Msg>Json = {` ... `};`, and match a field DECLARATION
# (`  name: ...` / `  name?: ...`) inside them only. Every absence check names a
# POSITIVE CONTROL field that must be found by the SAME extraction and matcher,
# so a broken extraction fails loudly instead of reporting a false "absent".
message_blocks() {
  local path="$1"
  local msg="$2"
  awk -v start="^export type ${msg} = Message<" -v json="^export type ${msg}Json = [{]" '
    $0 ~ start || $0 ~ json { inside = 1 }
    inside { print }
    inside && /^};/ { inside = 0 }
  ' "${path}"
}

message_field_count() {
  local path="$1"
  local msg="$2"
  local field="$3"
  message_blocks "${path}" "${msg}" | grep -cE "^[[:space:]]+${field}[?]?: " || true
}

assert_message_field() {
  local file="$1"
  local msg="$2"
  local field="$3"
  local path="${OUT_DIR}/${file}"
  if [[ ! -f "${path}" ]]; then
    echo "verify-codegen: FAIL — expected file not produced: ${path}" >&2
    exit 1
  fi
  # Both the message type and its JSON twin must declare it.
  if [[ "$(message_field_count "${path}" "${msg}" "${field}")" -lt 2 ]]; then
    echo "verify-codegen: FAIL — message ${msg} (or ${msg}Json) has no field '${field}': ${path}" >&2
    exit 1
  fi
  echo "verify-codegen: OK — ${msg}.${field} in ${file}"
}

assert_message_field_absent() {
  local file="$1"
  local msg="$2"
  local field="$3"
  local control="$4"
  local path="${OUT_DIR}/${file}"
  # Positive control first: the same extraction must find a declared field.
  assert_message_field "${file}" "${msg}" "${control}"
  if [[ "$(message_field_count "${path}" "${msg}" "${field}")" -ne 0 ]]; then
    echo "verify-codegen: FAIL — deleted field '${msg}.${field}' still generated: ${path}" >&2
    exit 1
  fi
  echo "verify-codegen: OK — ${msg}.${field} absent from ${file}"
}

# Declaration-anchored PRESENCE (2026-10-25): the twin of
# `assert_not_generated`. `assert_generated` is a bare substring grep, so a
# name that only survives in a doc comment (or as a prefix of a longer one)
# would satisfy it; new message/enum symbols are asserted on their export
# declaration instead.
assert_exported() {
  local file="$1"
  local symbol="$2"
  local path="${OUT_DIR}/${file}"
  if [[ ! -f "${path}" ]]; then
    echo "verify-codegen: FAIL — expected file not produced: ${path}" >&2
    exit 1
  fi
  if ! grep -qE "^export (type|enum|const) ${symbol}\\b" "${path}"; then
    echo "verify-codegen: FAIL — no export declaration for '${symbol}': ${path}" >&2
    exit 1
  fi
  echo "verify-codegen: OK — ${symbol} exported from ${file}"
}

# Oneof-CASE oracle (2026-10-25). A deleted oneof variant leaves no export of
# its own, and its message name may be reused elsewhere, so the check matches
# the generated discriminant line (`case: "<name>";`) only. A POSITIVE CONTROL
# case must be found by the same matcher, so a changed generator layout fails
# loudly instead of reporting a false "absent".
oneof_case_count() {
  local path="$1"
  local case_name="$2"
  grep -cE "^[[:space:]]+case: \"${case_name}\";" "${path}" || true
}

assert_oneof_case_absent() {
  local file="$1"
  local case_name="$2"
  local control="$3"
  local path="${OUT_DIR}/${file}"
  if [[ ! -f "${path}" ]]; then
    echo "verify-codegen: FAIL — cannot check absence, file not produced: ${path}" >&2
    exit 1
  fi
  if [[ "$(oneof_case_count "${path}" "${control}")" -lt 1 ]]; then
    echo "verify-codegen: FAIL — positive control oneof case '${control}' not found: ${path}" >&2
    exit 1
  fi
  if [[ "$(oneof_case_count "${path}" "${case_name}")" -ne 0 ]]; then
    echo "verify-codegen: FAIL — deleted oneof case '${case_name}' still generated: ${path}" >&2
    exit 1
  fi
  echo "verify-codegen: OK — oneof case ${case_name} absent from ${file}"
}

assert_generated "dark_tower/signaling/v1/signaling_pb.ts" "JoinRequest"
assert_generated "dark_tower/internal/v1/internal_pb.ts" "FastHeartbeatResponse"
assert_generated "dark_tower/internal/v1/internal_pb.ts" "ComprehensiveHeartbeatResponse"

# ADR-0036 internal contract (2026-09-01 reshape) — the MC→MH control-plane
# shapes must exist. `RegisterRequest` was previously asserted here; it is one of
# the eight symbols the reshape deletes and is asserted ABSENT below.
for symbol in \
  RegisterMeetingRequest \
  RegisterMeetingResponse \
  EgressStream \
  SubscriberSlot \
  CandidateSource \
  SelectionRules; do
  assert_generated "dark_tower/internal/v1/internal_pb.ts" "${symbol}"
done

# Story 2 (R-20) — the distinct teardown RPC with its own request/response pair
# (buf STANDARD: distinct types per RPC). Method names are checked in
# protoc-gen-es's generated casing, read from a regenerated output rather than
# written from memory. `endMeeting: {` is the service descriptor's method entry,
# so it cannot be satisfied by the message names. (Story 2's meeting-level
# muted set, `MutedSource` / `serverMutedSources`, was deleted by MC-B and is
# asserted ABSENT below.)
for symbol in \
  EndMeetingRequest \
  EndMeetingResponse \
  "endMeeting: {"; do
  assert_generated "dark_tower/internal/v1/internal_pb.ts" "${symbol}"
done

# ADR-0036 internal contract — the retired shapes must be GONE.
#
# All eight, not a sample. A presence-only oracle stays green through a
# half-done deletion or a rename-back, which is the same reasoning that added
# the signalling absence block below. Note `RegisterResponse` is asserted absent
# from `internal_pb.ts` ONLY: `packages/sdk-core/src/http/types.ts` defines an
# unrelated hand-written AC HTTP type of the same name, which is why this
# assertion is file-scoped and why the deletion audit had to check both.
for symbol in \
  RegisterRequest \
  RegisterResponse \
  RoutingOptions \
  CascadeDestination \
  RouteMediaRequest \
  RouteMediaResponse \
  StreamTelemetryRequest \
  StreamTelemetryResponse; do
  assert_not_generated "dark_tower/internal/v1/internal_pb.ts" "${symbol}"
done

# ADR-0036 signalling contract — the media-path shapes must exist.
for symbol in \
  ReceiveCapability \
  ReceiveSlot \
  SendDirective \
  SendTarget \
  StreamAssignment \
  MeetingKekUpdate \
  ServerMuteRequest \
  EncodingParameters; do
  assert_generated "dark_tower/signaling/v1/signaling_pb.ts" "${symbol}"
done

for symbol in MediaKind Codec TransportMode SlotState; do
  assert_generated "dark_tower/signaling/v1/signaling_pb.ts" "${symbol}"
done

# Story 2 (R-14, R-33) — additive signalling fields: W on the join response and
# the KEK push, and the per-subscriber unreachable set on `StreamAssignments`.
for symbol in kekRotationDebounceSeconds unreachableSenderIds; do
  assert_generated "dark_tower/signaling/v1/signaling_pb.ts" "${symbol}"
done

# Story 2 task 13's single receive-slot cap (`JoinResponse.max_receive_slots`)
# was deleted by MC-B and is asserted ABSENT below (member-scoped).

# ADR-0036 signalling contract — the superseded shapes must be GONE.
# `StreamAssignment` is deliberately absent from this list: the name is reused
# by the new shape, so an absence assert on it would contradict the presence
# assert above.
for symbol in \
  HostMuteRequest \
  EncryptionKeys \
  SubscribeToLayout \
  UpdateLayout \
  UnsubscribeLayout \
  LayoutConfig \
  LayoutType \
  StreamType \
  MediaType \
  StreamMetadata \
  VideoMetadata \
  SimulcastLayer; do
  assert_not_generated "dark_tower/signaling/v1/signaling_pb.ts" "${symbol}"
done

# Story 3 (see-each-other) — the additive signalling wire: join declarations
# and per-kind caps, the minimum keyframe interval, and the MH bidi-stream
# keyframe/recovery messages. Field names in protoc-gen-es casing, read from a
# regenerated output.
for symbol in \
  minKeyframeIntervalMs \
  MhServerMessage \
  KeyframeRequest \
  RecoveryRequest \
  keyframeRequest \
  recoveryRequest; do
  assert_generated "dark_tower/signaling/v1/signaling_pb.ts" "${symbol}"
done
assert_enum_member "dark_tower/signaling/v1/signaling_pb.ts" Codec VP8
for member in AVATAR_NO_VIDEO UNAVAILABLE AVATAR_CLIENT_MUTED AVATAR_SERVER_MUTED; do
  assert_enum_member "dark_tower/signaling/v1/signaling_pb.ts" SlotState "${member}"
done
# Reserved by number and name in this story (ZERO_REQUESTED by P1,
# SOURCE_MUTED by MC-B): member-scoped absences, each with a positive control.
assert_enum_member_absent "dark_tower/signaling/v1/signaling_pb.ts" SlotState ZERO_REQUESTED ACTIVE
assert_enum_member_absent "dark_tower/signaling/v1/signaling_pb.ts" SlotState SOURCE_MUTED AVATAR_SERVER_MUTED

# Story 3 — the internal per-stream source declarations and echo.
for symbol in \
  SourceStream \
  SourceStreamEcho \
  SourceDisposition \
  sourceStreams \
  appliedSourceStreams; do
  assert_generated "dark_tower/internal/v1/internal_pb.ts" "${symbol}"
done
for member in FORWARD PAUSED DROP_AT_INGRESS; do
  assert_enum_member "dark_tower/internal/v1/internal_pb.ts" SourceDisposition "${member}"
done

# Story 3 MC-B — the superseded fields and message are GONE (reserved by tag
# and name in the protos). Field absences are member-scoped with a positive
# control in the same message; `MutedSource` is a whole message, so the
# export-declaration check is the right one for it.
assert_not_generated "dark_tower/internal/v1/internal_pb.ts" MutedSource
assert_message_field_absent "dark_tower/internal/v1/internal_pb.ts" EgressStream transportMode supersedeOnIndependentFrame
assert_message_field_absent "dark_tower/internal/v1/internal_pb.ts" RegisterMeetingRequest serverMutedSources sourceStreams
assert_message_field_absent "dark_tower/internal/v1/internal_pb.ts" RegisterMeetingResponse transportMode appliedSourceStreams
assert_message_field_absent "dark_tower/signaling/v1/signaling_pb.ts" JoinResponse maxReceiveSlots receiveSlotCaps
# The surviving `transportMode` declarations are still matched by the same
# helper, so the absences above cannot be a matcher that never matches.
assert_message_field "dark_tower/internal/v1/internal_pb.ts" SourceStream transportMode
assert_message_field "dark_tower/internal/v1/internal_pb.ts" SourceStreamEcho transportMode

# 2026-10-25 (ADR-0036 §5 amendment; CONVENTIONS §6) — every per-kind fact is
# a repeated entry keyed by `MediaKind`. Presence is declaration-anchored
# (export / field / enum-member declarations); every retired field pair is
# asserted ABSENT member-scoped, each with a positive control in the same
# message, so neither a half-done migration nor a rename-back passes.
SIG="dark_tower/signaling/v1/signaling_pb.ts"
for symbol in ClientMediaState MediaKindState PublishedKind MuteChange ReceiveSlotCap; do
  assert_exported "${SIG}" "${symbol}"
done
for member in UNSPECIFIED NOT_PUBLISHED LIVE CLIENT_MUTED; do
  assert_enum_member "${SIG}" ClientMediaState "${member}"
done
assert_message_field "${SIG}" JoinRequest publishedKinds
assert_message_field "${SIG}" JoinResponse receiveSlotCaps
assert_message_field "${SIG}" Participant mediaStates
assert_message_field "${SIG}" ParticipantMuteUpdate mediaStates
assert_message_field "${SIG}" MuteRequest changes
assert_message_field "${SIG}" ServerMuteRequest changes
assert_message_field "${SIG}" UnmuteRequest mediaKinds
for field in mediaKind clientState serverMuted serverMutedBy; do
  assert_message_field "${SIG}" MediaKindState "${field}"
done
for field in mediaKind maxSlots; do
  assert_message_field "${SIG}" ReceiveSlotCap "${field}"
done
for field in mediaKind clientMuted; do
  assert_message_field "${SIG}" PublishedKind "${field}"
done
for field in mediaKind muted; do
  assert_message_field "${SIG}" MuteChange "${field}"
done

assert_message_field_absent "${SIG}" JoinRequest publishKinds publishedKinds
for field in maxAudioReceiveSlots maxVideoReceiveSlots; do
  assert_message_field_absent "${SIG}" JoinResponse "${field}" receiveSlotCaps
done
for field in audioClientMuted videoClientMuted; do
  assert_message_field_absent "${SIG}" Participant "${field}" mediaStates
done
for field in \
  audioSelfMuted videoSelfMuted \
  audioServerMuted videoServerMuted serverMutedBy \
  audioClientMuted videoClientMuted; do
  assert_message_field_absent "${SIG}" ParticipantMuteUpdate "${field}" mediaStates
done
for field in audioMuted videoMuted; do
  assert_message_field_absent "${SIG}" MuteRequest "${field}" changes
  assert_message_field_absent "${SIG}" ServerMuteRequest "${field}" changes
done
for field in requestAudio requestVideo; do
  assert_message_field_absent "${SIG}" UnmuteRequest "${field}" mediaKinds
done
# `UnmuteResponse` is deleted as a message and as a variant of BOTH envelopes.
assert_not_generated "${SIG}" UnmuteResponse
assert_oneof_case_absent "${SIG}" unmuteResponse unmuteRequest

echo "verify-codegen: all checks passed"
