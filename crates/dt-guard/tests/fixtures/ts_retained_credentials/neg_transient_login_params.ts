// The lens-4 case: a credential-bearing type used ONLY as a call-scoped parameter.
// Must be SILENT. This is the fixture that distinguishes a real guard from one with a
// hole cut for its own subject — `LoginCredentials` survives in the shipped
// `JoinCredentials` union on the merits, with NO allowlist entry, precisely because a
// function-parameter annotation is not a retention site.
export interface LoginParams {
  readonly email: string;
  readonly password: string;
}

export function authenticate(params: LoginParams): string {
  return params.email;
}

export function alsoTransient(
  first: string,
  params: LoginParams,
): string {
  return first + params.email;
}
