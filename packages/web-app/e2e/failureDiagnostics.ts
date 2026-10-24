// File: packages/web-app/e2e/failureDiagnostics.ts
//
// Story 3 C-C1: the ONE on-failure diagnostics wrapper for the video specs.
//
// A video scenario that fails can fail for three very different reasons, with
// three different owners, and the dump labels each so triage starts in the
// right place:
//
//   1. MH BUDGET REJECTION — the S1 classifier (`mcMetrics.diagnoseMissingSender`
//      over the admission baseline taken BEFORE the joins);
//   2. PENDING EDGES / MISSING KEYFRAME EVENTS — MH's pending gauge and keyframe
//      request counters, MC's video slot-state distribution
//      (`mcMetrics.videoDiagnosticReads`);
//   3. CLIENT FAULT — each page's own bus: drops by the SDK's closed reject
//      reason (`videoBusShape.parseReceiveDrops`) and the last `videoSlots`
//      sample (lumas omitted).
//
// Pod- and fleet-level reads are DIAGNOSTICS ONLY, never assertions. This module
// builds no PromQL (every query string lives in `mcMetrics.ts`).
//
// BOUNDED: the dump must finish before the test's own ceiling, or Playwright
// reports "Test timeout exceeded" and the original failure is lost. Worst case
// ≈ 15 s (the S1 settle) + 5 s (its reads) + 5 s (the parallel MH/MC reads,
// each fetch bounded) + pages × `PAGE_READ_BOUND_MS` (3 s; a wedged renderer is
// reported as TIMED OUT, never waited on) — ≈ 37 s for four pages, well inside
// every video spec's margin (each ceiling is ≥ 3× its passing path).
//
// THE ORIGINAL ERROR IS RETHROWN UNCHANGED — the same object, with the dump
// appended to its message and attached to the test — and a dump that itself
// fails reports that INSIDE the dump text. Nothing here can replace or swallow
// the assertion that failed.

import { test, type Page } from 'playwright/test';
import { latestBusEventsOf } from './fixtures.js';
import type { InstanceCounters } from './instanceCounters.js';
import { diagnoseMissingSender, videoDiagnosticReads } from './mcMetrics.js';
import { parseReceiveDrops, withoutLumas } from './videoBusShape.js';

/** A page whose bus the client-fault section reads. */
export interface DiagnosedPage {
  readonly label: string;
  readonly page: Page;
}

/** Bound on each page's bus read in the dump (a busy or wedged renderer must not stall it). */
const PAGE_READ_BOUND_MS = 3_000;

/** `read`, or a `TIMED OUT` rejection after `ms`. */
async function bounded<T>(read: Promise<T>, ms: number): Promise<T> {
  let timer: ReturnType<typeof setTimeout> | undefined;
  const timeout = new Promise<never>((_, reject) => {
    timer = setTimeout(() => reject(new Error(`TIMED OUT after ${ms}ms`)), ms);
  });
  try {
    return await Promise.race([read, timeout]);
  } finally {
    clearTimeout(timer);
  }
}

async function clientSection(pages: readonly DiagnosedPage[]): Promise<string> {
  const lines: string[] = [];
  for (const { label, page } of pages) {
    try {
      if (page.isClosed()) {
        lines.push(`  ${label}: page closed`);
        continue;
      }
      const { receiveLayers, videoSlots } = await bounded(
        latestBusEventsOf(page, ['receiveLayers', 'videoSlots']),
        PAGE_READ_BOUND_MS,
      );
      const drops =
        receiveLayers === undefined ? [] : parseReceiveDrops(receiveLayers['drops'] ?? []);
      const slots = Array.isArray(videoSlots?.['slots'])
        ? (videoSlots['slots'] as Array<Record<string, unknown>>).map(withoutLumas)
        : [];
      lines.push(
        `  ${label}: drops ${JSON.stringify(drops)}; last videoSlots ${JSON.stringify(slots)}`,
      );
    } catch (err) {
      lines.push(
        `  ${label}: bus read FAILED: ${err instanceof Error ? err.message : String(err)}`,
      );
    }
  }
  return lines.join('\n');
}

/** Build the labelled dump. Never throws. */
export async function videoFailureDump(
  what: string,
  admissionBaseline: InstanceCounters,
  pages: readonly DiagnosedPage[] = [],
): Promise<string> {
  const sections: string[] = [`=== video failure diagnostics: ${what} ===`];
  try {
    const s1 = await diagnoseMissingSender(admissionBaseline, what);
    sections.push(`[1] MH budget rejection (S1): ${s1.report}`);
  } catch (err) {
    sections.push(`[1] MH budget rejection (S1): READ FAILED: ${String(err)}`);
  }
  try {
    const reads = await videoDiagnosticReads();
    sections.push(
      '[2] pending edges / keyframe events / MC video slot states:\n' +
        reads.map((r) => `  ${r.label}: ${r.value}`).join('\n'),
    );
  } catch (err) {
    sections.push(`[2] pending / keyframe reads: READ FAILED: ${String(err)}`);
  }
  sections.push(`[3] client fault (each page's own bus):\n${await clientSection(pages)}`);
  return sections.join('\n');
}

/**
 * Run `fn`; on failure append the labelled dump to the ORIGINAL error and
 * rethrow that same error.
 */
export async function withVideoDiagnostics<T>(
  what: string,
  admissionBaseline: InstanceCounters,
  pages: readonly DiagnosedPage[] | (() => readonly DiagnosedPage[]),
  fn: () => Promise<T>,
): Promise<T> {
  try {
    return await fn();
  } catch (err) {
    let dump: string;
    try {
      dump = await videoFailureDump(
        what,
        admissionBaseline,
        typeof pages === 'function' ? pages() : pages,
      );
    } catch (dumpErr) {
      dump = `video failure diagnostics FAILED: ${String(dumpErr)}`;
    }
    try {
      await test
        .info()
        .attach('video-failure-diagnostics', { body: dump, contentType: 'text/plain' });
    } catch {
      // Outside a running test there is nothing to attach to; the message carries it.
    }
    if (err instanceof Error) {
      err.message = `${err.message}\n\n${dump}`;
      throw err;
    }
    throw err;
  }
}
