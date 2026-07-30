// Nested-object case: `readonly creds: { password: string }` must attribute to the
// ENCLOSING declaration. Also the .tsx coverage for the extension list — the scope
// statement claims .tsx, so a case backs the claim rather than the claim standing alone.
export interface NestedAuthState {
  readonly creds: { password: string };
  readonly accessToken: string;
}

export class NestedHolder {
  readonly #state: NestedAuthState;

  constructor(state: NestedAuthState) {
    this.#state = state;
  }
}
