// File: packages/sdk-core/src/__tests__/serverMessageSinkScan.test.ts
//
// THE TS-SIDE KEK SINK CONTROL'S MECHANICAL HALF.
//
// ---------------------------------------------------------------------------
// WHAT IT COVERS AND WHY IT IS NEEDED
// ---------------------------------------------------------------------------
//
// protobuf-es has no `skip_debug`, so a decoded `ServerMessage` carrying
// `meeting_kek` prints the meeting KEK in the clear under `JSON.stringify`,
// under `structuredClone` into a worker, and under any template interpolation.
// The Rust side suppresses derived `Debug` for those messages; TypeScript has no
// equivalent, and `scripts/guards/semantic/checks.md`'s items 11-13 are Rust-only
// by their own scope line.
//
// `signaling/kekIntake.ts` closes the WIRE-DECODE half structurally: the key
// leaves the decoded message the moment it arrives. This scan closes the
// RESIDENCY half — the KEK is then alive inside the seam for the meeting's whole
// life, and a sink reached from THAT object is a different leak on a different
// path, reached from a different noun. Same for a transmit key in the manager
// and a roster key in the resolver.
//
// ---------------------------------------------------------------------------
// TWO SIDES: SUBJECTS AND SINKS
// ---------------------------------------------------------------------------
//
// SUBJECTS are the wire message types AND the seam objects that hold key
// material past the decode boundary. SINKS are the serialising and cross-context
// exits, plus the storage APIs — the last because ADR-0036 §4 scopes identity
// keys to ONE MEETING, so persisting one is a linkability failure rather than a
// confidentiality one.
//
// ---------------------------------------------------------------------------
// HOW THIS NARROWS, STATED SO IT IS NOT CITED AS MORE THAN IT IS
// ---------------------------------------------------------------------------
//
// This is an enumerated scan and an enumerated list narrows silently on
// refactor while continuing to report green. It catches the direct spellings
// below; it does NOT catch a subject renamed to a local alias first, and no
// name-based matcher can, because the property being protected is not a property
// of names. The structural control is `kekIntake.ts` (nothing to leak) and the
// allow-list label constructor (nothing to label); this is the supplement, not
// the thing standing between a key and a log line.

import { readdirSync, readFileSync, statSync } from 'node:fs';
import { join } from 'node:path';
import { describe, expect, it } from 'vitest';

const SRC_ROOT = new URL('..', import.meta.url).pathname;

/** Nouns that hold, or have just held, key material. */
const SUBJECTS = [
  // Wire types carrying `meeting_kek`.
  'serverMessage',
  'joinResponse',
  'meetingKekUpdate',
  // Seam objects holding material past the decode boundary.
  'kekSource',
  'kekSink',
  'transmitKeys',
  'rosterKeys',
  'identity',
  'signingKey',
];

/** Serialising, cross-context and persisting exits. */
const SINK_PATTERNS: readonly { readonly name: string; readonly re: RegExp }[] = [
  { name: 'JSON.stringify', re: /JSON\.stringify\s*\(/ },
  { name: 'structuredClone', re: /structuredClone\s*\(/ },
  { name: 'postMessage', re: /\.postMessage\s*\(/ },
  { name: 'console.*', re: /\bconsole\s*\.\w+\s*\(/ },
  { name: 'logger.*', re: /\blogger\s*\.\w+\s*\(/ },
  { name: 'localStorage', re: /\blocalStorage\b/ },
  { name: 'sessionStorage', re: /\bsessionStorage\b/ },
  { name: 'indexedDB', re: /\bindexedDB\b/ },
  // The sink this file's own opening paragraph names first and the list
  // originally omitted: a `${subject}` interpolation into a string, an error
  // message, or the DOM prints exactly what `JSON.stringify` would. It matters
  // most for the SUBJECTS the intake zeroing does NOT neutralise — the live seam
  // objects — where a `throw new Error(\`...\${transmitKeys}\`)` would ship green.
  { name: 'template-interpolation', re: /\$\{/ },
];

/**
 * Blank out the TEXT of string and template literals, keeping `${...}`
 * expressions.
 *
 * Without this the scan matches a subject NAME appearing inside a message —
 * e.g. the bounded debug line `ignoring unhandled ServerMessage variant
 * '${message.case}'`, which interpolates a case name and never the message. That
 * is a false positive of the worst kind for a control like this one: it points
 * at innocent code, gets dismissed, and teaches the next reader to weaken the
 * scan rather than trust it.
 *
 * Interpolated EXPRESSIONS are deliberately preserved, because `${transmitKeys}`
 * is exactly the thing being looked for.
 */
function blankLiteralText(line: string): string {
  let out = '';
  let quote: "'" | '"' | '`' | undefined;
  let depth = 0;
  for (let i = 0; i < line.length; i += 1) {
    const ch = line[i] as string;
    if (quote === undefined) {
      if (ch === "'" || ch === '"' || ch === '`') {
        quote = ch as "'" | '"' | '`';
        out += ch;
        continue;
      }
      out += ch;
      continue;
    }
    if (ch === '\\') {
      out += '  ';
      i += 1;
      continue;
    }
    if (quote === '`' && ch === '$' && line[i + 1] === '{') {
      // Enter an interpolated expression: real code, kept verbatim.
      depth += 1;
      out += '${';
      i += 1;
      continue;
    }
    if (depth > 0) {
      if (ch === '}') depth -= 1;
      out += ch;
      continue;
    }
    if (ch === quote) {
      quote = undefined;
      out += ch;
      continue;
    }
    out += ' ';
  }
  return out;
}

function sourceFiles(dir: string): string[] {
  const out: string[] = [];
  for (const entry of readdirSync(dir)) {
    const path = join(dir, entry);
    if (statSync(path).isDirectory()) {
      // Generated protobuf code is not hand-written and contains no sinks; test
      // sources legitimately construct and inspect these types.
      if (entry === '__tests__' || entry === 'proto') continue;
      out.push(...sourceFiles(path));
      continue;
    }
    if (entry.endsWith('.ts')) out.push(path);
  }
  return out;
}

/**
 * Blank out comments while PRESERVING LINE NUMBERS.
 *
 * A block comment is replaced by its own newlines rather than deleted, so a
 * reported `file:line` points at the real source. A scan that names the wrong
 * line sends its reader to innocent code and gets dismissed as a false positive
 * — which is how a true finding is lost.
 */
function stripComments(source: string): string {
  return source
    .replace(/\/\*[\s\S]*?\*\//g, (block) => block.replace(/[^\n]/g, ' '))
    .replace(/(^|[^:])\/\/.*$/gm, '$1');
}

/**
 * Every `(sink, subject)` hit in one source text.
 *
 * Extracted so the matcher can be DEMONSTRATED on synthetic offending lines
 * rather than only run over a tree it is expected to find nothing in. A scan
 * whose only evidence is a green run over clean code is indistinguishable from a
 * scan that matches nothing at all.
 */
function scan(path: string, source: string): string[] {
  const findings: string[] = [];
  stripComments(source)
    .split('\n')
    .forEach((raw, index) => {
      // Sinks are matched on the RAW line (a sink is a call). Subjects are
      // matched with literal TEXT blanked, so a subject name inside a message is
      // not mistaken for a subject value.
      const code = blankLiteralText(raw);
      for (const sink of SINK_PATTERNS) {
        if (!sink.re.test(raw)) continue;
        for (const subject of SUBJECTS) {
          const re = new RegExp(`\\b${subject}\\b`, 'i');
          if (re.test(code)) {
            findings.push(`${path}:${index + 1}: ${sink.name} reached by \`${subject}\``);
          }
        }
      }
    });
  return findings;
}

describe('no key-bearing value reaches a serialising or persisting sink', () => {
  it('scans a non-empty set of production sources — the scope fails itself', () => {
    // A scan that silently covers nothing is worse than no scan: it reports
    // clean while the offending line ships, which ADR-0036 §11 names as strictly
    // worse than an absent control, because an absence gets noticed.
    expect(sourceFiles(SRC_ROOT).length).toBeGreaterThan(20);
  });

  it('never passes a key-bearing subject to a sink on the same line', () => {
    const findings: string[] = [];
    for (const file of sourceFiles(SRC_ROOT)) {
      findings.push(...scan(file, readFileSync(file, 'utf8')));
    }
    expect(findings, findings.join('\n')).toEqual([]);
  });

  it('DEMONSTRATES that it fires — the matcher on synthetic offending lines', () => {
    // A control's coverage must be demonstrated, not asserted. Without this, the
    // literal-text blanking added to kill one false positive could have made the
    // whole scan vacuous and the suite would have stayed green.
    const offending = [
      'logger.debug(`state ${transmitKeys}`);',
      'console.error(JSON.stringify(kekSource));',
      'worker.postMessage(structuredClone(identity));',
      'localStorage.setItem("k", signingKey);',
      'throw new Error(`roster ${rosterKeys}`);',
    ].join('\n');
    const hits = scan('synthetic.ts', offending);
    // Assert every LINE was caught, not the hit count: several lines match more
    // than one sink family (`console.error(JSON.stringify(...))` is both), and a
    // count would be a brittle assertion about the sink list's shape rather than
    // about the property.
    const linesCaught = new Set(hits.map((h) => h.split(':')[1]));
    expect(linesCaught.size).toBe(5);
    expect(hits.some((h) => h.includes('template-interpolation'))).toBe(true);
    expect(hits.some((h) => h.includes('JSON.stringify'))).toBe(true);
    expect(hits.some((h) => h.includes('localStorage'))).toBe(true);
  });

  it('does NOT fire on a subject NAME appearing inside a message', () => {
    // The false positive that motivated blanking literal text: a bounded debug
    // line that interpolates a case name and never the message. Pinned so the
    // blanking is not later removed as unnecessary.
    expect(
      scan(
        'synthetic.ts',
        "this.#logger.debug(`ignoring unhandled ServerMessage variant '${message.case}'`);",
      ),
    ).toEqual([]);
  });

  it('persists no identity signing key — identity keys are scoped to ONE meeting', () => {
    // ADR-0036 §4: "a key reused across meetings makes a participant linkable by
    // public key regardless of display name, which matters most for the guests
    // who have the least identity assurance to begin with." So the property is
    // not confidentiality — the public half is public — it is that no storage
    // API is touched at all on this path.
    for (const file of sourceFiles(SRC_ROOT)) {
      const source = stripComments(readFileSync(file, 'utf8'));
      for (const api of ['localStorage', 'sessionStorage', 'indexedDB']) {
        expect(source, `${file}: the SDK persists nothing; ${api} must not appear`).not.toContain(
          api,
        );
      }
    }
  });
});
