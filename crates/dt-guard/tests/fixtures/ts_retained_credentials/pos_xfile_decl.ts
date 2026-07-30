// Cross-file positive, DECLARATION half. Mirrors packages/web-app/src/lib/types.ts
// pre-fix. Scanned ALONE this file must be SILENT: it declares a credential-bearing
// type but retains nothing. That silence is a precision assertion, not an absence of
// coverage — the retention half lives in pos_xfile_retention.svelte.
export interface AuthResult {
  readonly subdomain: string;
  readonly email: string;
  readonly password: string;
  readonly userToken: string;
}
