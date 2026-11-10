// LENS-ONLY (story 4 C1; checks.md §Client Credential Lifetime, UNSAFE (c)). The
// mechanical guard is EXPECTED TO BE SILENT: a closure capture is not one of its
// enumerated retention idioms.
//
// POSITIVE: the redial timer captures the dial TOKEN STRING, so the credential
// outlives its ref's removal (the timer still holds it) and a later refresh is
// ignored (the stale token is dialed).
export class Transport {
  #dialTokens = new Map<number, string>();

  scheduleRedial(ref: number, url: string, delayMs: number): void {
    const token = this.#dialTokens.get(ref) ?? '';
    setTimeout(() => this.#dial(url, token), delayMs);
  }

  #dial(url: string, mediaDialToken: string): void {
    void url;
    void mediaDialToken;
  }
}
