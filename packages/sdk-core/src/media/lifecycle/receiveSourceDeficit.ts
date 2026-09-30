// File: packages/sdk-core/src/media/lifecycle/receiveSourceDeficit.ts
//
// The per-interval decision behind `dt_client_media_receive_source_deficit_total`
// (story 2 R-28), as a PURE function over two samples so the grace rule and the
// slot-state filter are testable without timers. The timer and the emission live
// in `intervalSignals.ts`.
//
// ---------------------------------------------------------------------------
// OBSERVED DECODE ACTIVITY IS THE RECEIVER'S GROUND TRUTH
// ---------------------------------------------------------------------------
//
// MC states which slots are ACTIVE — a sender assigned and not source-muted.
// This client observes what it actually decoded. The signal is the DIVERGENCE:
// an active assignment from which nothing decoded in a whole interval. Mirroring
// slot state instead would read healthy exactly when MC says active and nothing
// decodes. The premise that "nothing decoded" is a fault rather than silence is
// DTX being off (`muteState.ts`): a live, unmuted sender always produces frames.

/** One active assignment at one sample. */
export interface ActiveAssignmentSample {
  readonly slotId: number;
  readonly senderId: number;
  /**
   * The sender's decode-lane instance and output count at this sample, or
   * `undefined` if no lane exists (e.g. the lane bound was exhausted) — which
   * decodes nothing, so it counts once it has been active a full interval.
   */
  readonly activity: { readonly epoch: number; readonly decoded: number } | undefined;
}

/** Every active assignment at one sample. */
export type ActiveAssignmentsSample = readonly ActiveAssignmentSample[];

/**
 * How many assignments were active for the WHOLE interval between `previous`
 * and `current` and decoded nothing in it.
 *
 * GRACE RULE: an assignment counts only if the same (slot, sender) pair was
 * active at `previous` too. One that became active mid-interval — a join, or a
 * re-map — has not had a full interval to produce frames, and counting it would
 * tick the counter on every membership change.
 *
 * "Decoded nothing" compares the lane's output count between the samples. A
 * changed lane epoch means the lane was rebuilt (the sender left and returned)
 * within the interval, which is not a full active interval either, so it does
 * not count.
 */
export function receiveSourceDeficit(
  previous: ActiveAssignmentsSample,
  current: ActiveAssignmentsSample,
): number {
  let deficit = 0;
  for (const now of current) {
    const before = previous.find((p) => p.slotId === now.slotId && p.senderId === now.senderId);
    if (!before) continue;
    const a = before.activity;
    const b = now.activity;
    if (a === undefined && b === undefined) {
      deficit += 1;
      continue;
    }
    if (a === undefined || b === undefined || a.epoch !== b.epoch) continue;
    if (b.decoded === a.decoded) deficit += 1;
  }
  return deficit;
}

/**
 * MC's ACTIVE assignments on DECLARED slots, with each sender's lane activity:
 * the sample {@link receiveSourceDeficit} compares. "Active" is MC's statement
 * (a sender assigned and not source-muted); a slot this client never declared
 * is not ours to count, whatever MC says.
 */
export function activeAssignments(
  assignments: readonly {
    readonly slotId: number;
    readonly senderId: number | undefined;
    readonly active: boolean;
  }[],
  declaredSlotIds: readonly number[],
  lanes: { activityFor(senderId: number): ActiveAssignmentSample['activity'] },
): ActiveAssignmentsSample {
  const sample: ActiveAssignmentSample[] = [];
  for (const a of assignments) {
    if (!a.active || a.senderId === undefined || !declaredSlotIds.includes(a.slotId)) continue;
    sample.push({
      slotId: a.slotId,
      senderId: a.senderId,
      activity: lanes.activityFor(a.senderId),
    });
  }
  return sample;
}
