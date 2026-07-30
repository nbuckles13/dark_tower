// Declaration half for pos_retained_union_alias.svelte. `JoinCredentials` is the type
// task #58 ADDS, which is what makes this the most plausible future retention in the
// repo — and it was invisible to the guard until the closure resolved aliases.
export interface TokenCredentials {
  readonly mode: 'token';
  readonly userToken: string;
}
export interface LoginCredentials {
  readonly mode: 'login';
  readonly email: string;
  readonly password: string;
}
export type JoinCredentials = TokenCredentials | LoginCredentials;
