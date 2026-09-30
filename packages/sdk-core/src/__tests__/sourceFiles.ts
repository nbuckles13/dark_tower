// File: packages/sdk-core/src/__tests__/sourceFiles.ts
//
// The ONE source-tree walker for the SDK's source-scan tests (extracted on third
// use, story 2 task 13). The callers DIFFER in which directories they skip and
// whether `.d.ts` counts, and those differences are deliberate, so both are
// parameters rather than one policy forced on every caller.

import { readdirSync, statSync } from 'node:fs';
import { join } from 'node:path';

/** `.ts` files under `dir`, skipping `skipDirs` by name at any depth. */
export function sourceFiles(
  dir: string,
  options: { readonly skipDirs: readonly string[]; readonly includeDts: boolean },
): string[] {
  const out: string[] = [];
  for (const entry of readdirSync(dir)) {
    const path = join(dir, entry);
    if (statSync(path).isDirectory()) {
      if (options.skipDirs.includes(entry)) continue;
      out.push(...sourceFiles(path, options));
      continue;
    }
    if (!entry.endsWith('.ts')) continue;
    if (!options.includeDts && entry.endsWith('.d.ts')) continue;
    out.push(path);
  }
  return out;
}
