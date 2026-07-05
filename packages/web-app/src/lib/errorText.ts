// File: packages/web-app/src/lib/errorText.ts
//
// R-23: render a BOUNDED, non-secret string for a caught error. Typed SDK errors
// expose an SDK-authored `code` + bounded `message` (never a token / raw body);
// anything else collapses to a generic string. Never `String(err)` / `err.stack`.

import { SdkError } from '@darktower/sdk-core';

/** A safe, bounded display string for any caught error. */
export function errorText(err: unknown): string {
  if (err instanceof SdkError) {
    return `${err.code}: ${err.message}`;
  }
  return 'Unexpected error';
}
