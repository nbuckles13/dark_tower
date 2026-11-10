// LENS-ONLY (story 4 C1; checks.md §Client Credential Lifetime, "Scoped exception:
// the media_dial dial credential"). The mechanical guard is EXPECTED TO BE SILENT.
//
// NEGATIVE (SAFE): the latest token per IN-SET ref, in ONE private field,
// REPLACED WHOLESALE per set (a new Map, never a merge), deleted when the ref
// leaves the set, cleared at session end. The pair of
// `pos_dial_tokens_merged_keeps_dropped_ref.ts`.
interface Intake {
  readonly dialTokens: ReadonlyMap<number, string>;
}

export class Transport {
  /** CREDENTIAL — scoped retention (ADR-0039 §5 "Client retention"). */
  #dialTokens = new Map<number, string>();

  applyHandlerSet(intake: Intake): void {
    this.#dialTokens = new Map(intake.dialTokens);
  }

  removeRef(ref: number): void {
    this.#dialTokens.delete(ref);
  }

  disconnect(): void {
    this.#dialTokens.clear();
  }
}
