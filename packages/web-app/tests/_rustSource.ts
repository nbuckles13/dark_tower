// File: packages/web-app/tests/_rustSource.ts
//
// TEST-ONLY: read constants out of their ONE Rust home, for the drift pins of
// the browser twins (`mhLeg.test.ts`, `mcLoki.test.ts`). Hoisted from
// `mhLeg.test.ts` on its second consumer. Not a test file: no `.test.ts`
// suffix, so vitest's `tests/**/*.test.ts` include does not collect it.
//
// Every extractor FAILS with its own token ({@link RUST_CONST_NOT_FOUND}) when
// the const is missing or is not a single plain string literal (no escapes,
// no line continuation, no `concat!`): a moved, renamed or reshaped Rust const
// must turn the pin red, never read as `undefined` or garbage that a sloppy
// assertion could still accept.

import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';

/** Reason token of every extraction failure. */
export const RUST_CONST_NOT_FOUND = 'rust-const-not-found';

const REPO = fileURLToPath(new URL('../../../', import.meta.url));

/** A repo file's text (`rel` from the repo root). */
export function readRepo(rel: string): string {
  return readFileSync(join(REPO, rel), 'utf8');
}

/** A plain Rust string literal's body: no quote, no backslash. */
const PLAIN = '([^"\\\\\\n]*)';

/**
 * `pub const NAME: &str = "…";` — the literal's value.
 *
 * @throws {@link RUST_CONST_NOT_FOUND} when absent or not one plain literal.
 */
export function strConst(src: string, name: string): string {
  const m = new RegExp(`pub const ${name}: &str = "${PLAIN}";`).exec(src);
  if (m?.[1] === undefined) {
    throw new Error(
      `${RUST_CONST_NOT_FOUND}: no \`pub const ${name}: &str = "<plain literal>";\` in the Rust ` +
        'source (moved, renamed, or no longer a single plain string literal)',
    );
  }
  return m[1];
}

/** A Rust `LogLine { app, target, message }` const (`fixtures/loki.rs`). */
export interface RustLogLine {
  readonly app: string;
  readonly target: string;
  readonly message: string;
}

/**
 * `pub const NAME: LogLine = LogLine { app: "…", target: "…", message: "…", };`
 *
 * @throws {@link RUST_CONST_NOT_FOUND} when absent or any field is not one
 *   plain literal.
 */
export function logLineConst(src: string, name: string): RustLogLine {
  const m = new RegExp(
    `pub const ${name}: LogLine = LogLine \\{\\s*app: "${PLAIN}",\\s*target: "${PLAIN}",\\s*` +
      `message: "${PLAIN}",\\s*\\};`,
  ).exec(src);
  if (m?.[1] === undefined || m[2] === undefined || m[3] === undefined) {
    throw new Error(
      `${RUST_CONST_NOT_FOUND}: no \`pub const ${name}: LogLine = LogLine { app, target, ` +
        'message }` of plain string literals in the Rust source',
    );
  }
  return { app: m[1], target: m[2], message: m[3] };
}
