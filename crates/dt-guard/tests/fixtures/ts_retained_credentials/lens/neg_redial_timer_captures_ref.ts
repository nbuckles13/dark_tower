// LENS-ONLY (story 4 C1; checks.md §Client Credential Lifetime, UNSAFE (c)). The
// mechanical guard is EXPECTED TO BE SILENT.
//
// NEGATIVE (SAFE): the timer captures the REF and reads the token from the set
// state when it fires; an absent ref (removed meanwhile) means no dial. The pair
// of `pos_redial_timer_captures_token.ts`.
export class Transport {
  #dialTokens = new Map<number, string>();
  readonly #urls = new Map<number, string>();

  scheduleRedial(ref: number, delayMs: number): void {
    setTimeout(() => {
      const url = this.#urls.get(ref);
      if (url === undefined) return;
      this.#dial(url, this.#dialTokens.get(ref) ?? '');
    }, delayMs);
  }

  #dial(url: string, mediaDialToken: string): void {
    void url;
    void mediaDialToken;
  }
}
