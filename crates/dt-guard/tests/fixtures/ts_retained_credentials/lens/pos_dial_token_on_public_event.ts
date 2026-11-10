// LENS-ONLY (story 4 C1; checks.md §Client Credential Lifetime, UNSAFE (a)). The
// mechanical guard is EXPECTED TO BE SILENT.
//
// POSITIVE: the decoded handler set — dial tokens included — is emitted on a
// public event and kept on a session field, so every app listener, store, e2e
// bus or debug panel can hold or print the credential. The SAFE form projects
// `{version, handlers:[{ref, url}]}` at decode and hands the tokens to the
// transport through a private sink only.
interface WireHandler {
  readonly handlerRef: number;
  readonly mediaHandlerUrl: string;
  readonly mediaDialToken: string;
}

type Listener = (handlers: readonly WireHandler[]) => void;

export class Session {
  lastHandlerSet: readonly WireHandler[] = [];
  readonly #listeners: Listener[] = [];

  on(listener: Listener): void {
    this.#listeners.push(listener);
  }

  onMediaHandlerSet(handlers: readonly WireHandler[]): void {
    this.lastHandlerSet = handlers;
    for (const listener of this.#listeners) listener(handlers);
  }
}
