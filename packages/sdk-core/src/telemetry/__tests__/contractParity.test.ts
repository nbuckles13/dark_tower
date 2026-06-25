// File: packages/sdk-core/src/telemetry/__tests__/contractParity.test.ts
//
// ANTI-DRIFT lock for the Pattern A `MetricsSink` duplication (task #12, R-24).
// The canonical contract lives in sdk-core (`../MetricsSink.ts`); a parallel
// structural copy lives in `@darktower/test-utils`
// (`src/contracts/MetricsSink.ts`) because a re-export would make the Nx build
// graph circular (sdk-core devDepends test-utils; the reverse edge closes a
// cycle Nx rejects). These COMPILE-TIME assertions fail `tsc` if the two copies
// drift apart in shape — so the duplication cannot silently diverge.
//
// ENFORCEMENT BOUNDARY — read carefully: the parity guarantee rests SOLELY on
// `tsc --noEmit` (the package `lint` / `typecheck` target, which type-checks
// `__tests__/` and runs in CI via `nx -t lint`). It is NOT enforced by
// `vitest`/`test:unit`: Vitest transpiles via esbuild, which STRIPS types
// WITHOUT type-checking, so the type-level assignments below are erased and the
// runtime `expect(...)` body always passes REGARDLESS of drift. In other words,
// a drifted contract would still show a green `test:unit` run — only `tsc`
// catches it. Do not treat a passing test suite as proof of parity; the lint/
// typecheck step is the gate. The runtime `it(...)` block exists only to give
// the file a test and to reference the type-witness constants so they are not
// flagged unused; it is NOT the assertion.

import { describe, expect, it } from 'vitest';

import type {
  MetricLabels as CanonicalLabels,
  MetricsSink as CanonicalSink,
} from '../MetricsSink.js';
import type {
  MetricLabels as TestUtilsLabels,
  MetricsSink as TestUtilsSink,
} from '@darktower/test-utils';

// Mutual-assignability: each interface must be assignable to the other. If a
// method signature or `MetricLabels` shape changes in only one copy, one of
// these four assignments stops compiling.
type _SinkCanonicalToTestUtils = CanonicalSink extends TestUtilsSink ? true : never;
type _SinkTestUtilsToCanonical = TestUtilsSink extends CanonicalSink ? true : never;
type _LabelsCanonicalToTestUtils = CanonicalLabels extends TestUtilsLabels ? true : never;
type _LabelsTestUtilsToCanonical = TestUtilsLabels extends CanonicalLabels ? true : never;

// These constants only compile if the conditional types above resolved to
// `true` (i.e. the shapes are mutually assignable). A drift collapses the type
// to `never` and the assignment fails to compile.
const sinkParityAToB: _SinkCanonicalToTestUtils = true;
const sinkParityBToA: _SinkTestUtilsToCanonical = true;
const labelsParityAToB: _LabelsCanonicalToTestUtils = true;
const labelsParityBToA: _LabelsTestUtilsToCanonical = true;

describe('MetricsSink contract parity (Pattern A anti-drift, enforced by tsc)', () => {
  it('references the compile-time type witnesses (parity is checked by tsc, NOT here)', () => {
    // NOT THE ASSERTION. The real parity check is the type-level assignments
    // above, evaluated by `tsc --noEmit` (lint), not by this runtime body —
    // Vitest strips types, so this always passes even on drift. This block
    // only references the witness constants so they are not unused.
    expect([sinkParityAToB, sinkParityBToA, labelsParityAToB, labelsParityBToA]).toEqual([
      true,
      true,
      true,
      true,
    ]);
  });
});
