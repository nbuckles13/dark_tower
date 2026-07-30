// Controlled experiment for verification item (a). Mirrors the REAL B2 change, so it
// differs from pos_xfile_decl.ts in THREE ways — stated because a reader diffing the
// pair otherwise cannot tell which one the test turns on (@paired-infrastructure):
//
//   1. `password` removed        <- THE LOAD-BEARING VARIABLE
//   2. `email` removed           <- inert here: CATEGORY_B (PII), not in CREDENTIAL_TOKENS
//   3. renamed AuthResult -> AuthSession  <- inert: Rule 2 keys on credential-bearing-ness,
//                                            never on the type's name
//
// Only (1) can change this guard's verdict, so the experiment is still controlled. The
// retention site in the paired .svelte file is left INTACT — if the negative dropped it
// too, the pair would pass for the wrong reason, which is the tautological-fixture
// failure one level subtler than the double-exemption bug.
export interface AuthSession {
  readonly subdomain: string;
  readonly displayName?: string;
  readonly userToken: string;
}
