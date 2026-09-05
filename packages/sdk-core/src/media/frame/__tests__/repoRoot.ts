// File: packages/sdk-core/src/media/frame/__tests__/repoRoot.ts
//
// The repository-root walk-up for the TEST TIER. One of two DELIBERATE homes,
// with the reason recorded here and at the other — not "the only home", which is
// the claim that gives a reader a reason not to look (the exact defect caught in
// `hex.ts` at task 8).
//
// The second home is `packages/sdk-core/scripts/gen-wire-constants.mjs`, whose
// copy is character-identical. They are NOT unified, for a concrete reason:
// the renderer must run under bare `node` with no TypeScript loader, and this
// `.ts` is what LOCATES that `.mjs` at test time — so the `.mjs` cannot import
// from here (it predates the resolver that would load a `.ts`), and importing
// the `.mjs` from here would require a types shim for a 12-line function. The
// duplication is ~12 lines, both sides change together only if the marker file
// or the workspace layout changes, and each carries this note. Same shape as the
// two deliberate `media.rs` homes in `crates/mc-test-utils` and `crates/env-tests`.
//
// Anchored on `pnpm-workspace.yaml` rather than a fixed `../../../../../..` count,
// so moving a caller one directory does not silently resolve to the wrong tree —
// which would make a loader read a different repository's fixtures and report
// whatever it found there.

import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

/** Walk up from `from` to the directory holding `pnpm-workspace.yaml`. */
export function repoRoot(from: string = dirname(fileURLToPath(import.meta.url))): string {
  let dir = from;
  for (let i = 0; i < 12; i += 1) {
    try {
      readFileSync(join(dir, 'pnpm-workspace.yaml'));
      return dir;
    } catch {
      const parent = dirname(dir);
      if (parent === dir) break;
      dir = parent;
    }
  }
  throw new Error('could not locate repository root (no pnpm-workspace.yaml above this file)');
}
