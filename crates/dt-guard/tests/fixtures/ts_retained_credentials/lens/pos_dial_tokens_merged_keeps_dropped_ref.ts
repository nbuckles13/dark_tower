// LENS-ONLY (story 4 C1; checks.md §Client Credential Lifetime, "Scoped exception:
// the media_dial dial credential", UNSAFE (a)/(b)). The mechanical guard is
// EXPECTED TO BE SILENT here: `mediaDialToken`-style names segment into
// SESSION_TOKENS, not CREDENTIAL_TOKENS, so retention of a dial token is
// mechanically invisible and this lens is the only enforcement.
//
// POSITIVE: each set is MERGED into the held tokens instead of replacing them,
// so a ref that LEFT the set keeps its dial token for the rest of the session —
// retention past the set state the scoped exception allows.
interface Intake {
  readonly dialTokens: ReadonlyMap<number, string>;
}

export class Transport {
  #dialTokens = new Map<number, string>();

  applyHandlerSet(intake: Intake): void {
    for (const [ref, token] of intake.dialTokens) this.#dialTokens.set(ref, token);
  }
}
