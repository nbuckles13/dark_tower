// File: packages/web-app/e2e/cohort.ts
//
// Story 2 R-30: the N+1 distinct-user COHORT for the multi-party browser specs,
// and the AC auth-rate budget it must fit. Pure — no `./env`, no Playwright —
// so the node unit tier (`tests/cohort.test.ts`) exercises it hermetically, the
// same split as `./instanceCounters`.
//
// ---------------------------------------------------------------------------
// WHERE THE COHORT IS REGISTERED, AND WHY THERE
// ---------------------------------------------------------------------------
//
// ONCE per run, in `global-setup.ts`, NOT in a worker-module memo like
// `fixtures.ts`'s `sharedRegistration`. Playwright discards and restarts the
// worker process after any failing test, so a worker-module memo is a per-run
// singleton only on a green run: a cohort memo there would re-register all N+1
// accounts after every failure. Global setup runs once per run in the runner
// process (after the `webServer` plugin is up), and hands the credentials to
// every worker through {@link COHORT_ENV_VAR} — Playwright's documented
// globalSetup→worker channel. `workers: 1` (playwright.config.ts) still holds:
// escalation is by shards, and see e2e/README.md §Budgets for why shards only
// add headroom when each targets its own cluster.
//
// ---------------------------------------------------------------------------
// WHAT THE AC "REGISTRATION" LIMIT ACTUALLY COUNTS
// ---------------------------------------------------------------------------
//
// SUCCESSFUL TOKEN ISSUES per source IP per window — not registrations:
// `crates/ac-service/src/services/user_service.rs` `count_registrations_from_ip`
// counts `auth_events` rows with `event_type='user_login' AND success=true`,
// and BOTH `/register` and `/user/token` write one. So every sign-in spends
// budget too. The limit's SSoT for the Kind cluster is
// `infra/services/ac-service/config.env`, READ by {@link parseAcAuthRateLimit}
// rather than restated here.

import { parseConfigEnv, requirePositiveInt } from './configEnv.js';
import type { TestCredentials } from './fixtures.js';

/**
 * The suite's receive-slot count N — THE one place the multi-party suite's N is
 * written. Locked at 3 (four participants, 12 egress edges; the Kind MH egress
 * budget is sized for N=5 — see the Kind overlay patch
 * `infra/kubernetes/overlays/kind/services/mh-service/configmap-egress-budget-patch.yaml`).
 * Raising it is a budget decision (e2e/README.md §Budgets), not a knob.
 */
export const SUITE_RECEIVE_SLOTS = 3;

/**
 * Environment variable carrying the cohort from global setup to the workers.
 * Throwaway, per-run, per-run-org credentials (same class as `SHARED_USER`);
 * never logged.
 */
export const COHORT_ENV_VAR = 'E2E_COHORT_CREDENTIALS';

/** N+1: every participant hears the other N. Throws on anything but an integer ≥ 1. */
export function cohortSize(receiveSlots: number): number {
  if (!Number.isInteger(receiveSlots) || receiveSlots < 1) {
    throw new RangeError(`receive-slot count N must be an integer >= 1 (got ${receiveSlots})`);
  }
  return receiveSlots + 1;
}

/** Serialise the cohort for {@link COHORT_ENV_VAR}. */
export function encodeCohort(cohort: readonly TestCredentials[]): string {
  return JSON.stringify(
    cohort.map((c) => ({ email: c.email, password: c.password, displayName: c.displayName })),
  );
}

const isNonEmptyString = (v: unknown): v is string => typeof v === 'string' && v.length > 0;

/**
 * Parse {@link COHORT_ENV_VAR}. Fail-loud on every anomaly — a missing, short,
 * malformed or duplicated cohort would otherwise surface as N confusing sign-in
 * failures, or worse, as two participants sharing an account (which reads as a
 * routing bug in a multi-party spec).
 */
export function decodeCohort(raw: string | undefined, expectedSize: number): TestCredentials[] {
  if (raw === undefined || raw === '') {
    throw new Error(
      `${COHORT_ENV_VAR} is not set: global-setup.ts registers the N+1 cohort once per run and ` +
        `exports it to the workers. A spec reached the cohort without global setup having run ` +
        `(or global setup failed before registering it) — check the run's global-setup output.`,
    );
  }
  let parsed: unknown;
  try {
    parsed = JSON.parse(raw);
  } catch {
    throw new Error(`${COHORT_ENV_VAR} is not valid JSON`);
  }
  if (!Array.isArray(parsed)) {
    throw new Error(`${COHORT_ENV_VAR} must be a JSON array of credentials`);
  }
  if (parsed.length !== expectedSize) {
    throw new Error(
      `${COHORT_ENV_VAR} carries ${parsed.length} members, expected ${expectedSize} (N+1)`,
    );
  }
  const cohort: TestCredentials[] = parsed.map((entry: unknown, i) => {
    const e = entry as Record<string, unknown> | null;
    if (
      e === null ||
      typeof e !== 'object' ||
      !isNonEmptyString(e['email']) ||
      !isNonEmptyString(e['password']) ||
      !isNonEmptyString(e['displayName'])
    ) {
      // Never echo the entry: it holds a password.
      throw new Error(`${COHORT_ENV_VAR} member ${i} is malformed (email/password/displayName)`);
    }
    return { email: e['email'], password: e['password'], displayName: e['displayName'] };
  });
  const emails = new Set(cohort.map((c) => c.email));
  if (emails.size !== cohort.length) {
    throw new Error(`${COHORT_ENV_VAR} members are not distinct accounts (duplicate email)`);
  }
  return cohort;
}

/** The AC per-IP auth-rate limit, as the target cluster's config declares it. */
export interface AcAuthRateLimit {
  readonly maxAttempts: number;
  readonly windowMinutes: number;
}

/** The two keys, named once. */
export const AC_REGISTRATION_RATE_LIMIT_MAX_KEY = 'AC_REGISTRATION_RATE_LIMIT_MAX_ATTEMPTS';
export const AC_REGISTRATION_RATE_LIMIT_WINDOW_KEY = 'AC_REGISTRATION_RATE_LIMIT_WINDOW_MINUTES';

/**
 * Read the limit from the text of `infra/services/ac-service/config.env`
 * (`source` names it in diagnostics). Both keys REQUIRED and positive integers;
 * a missing or malformed key THROWS — defaulting would size the suite against a
 * number the cluster does not run. Comments and blank lines are ignored; a key
 * named only in a comment is not a key.
 */
export function parseAcAuthRateLimit(configEnvText: string, source: string): AcAuthRateLimit {
  const values = parseConfigEnv(configEnvText);
  const purpose = "cannot size the suite's AC auth budget";
  return {
    maxAttempts: requirePositiveInt(values, AC_REGISTRATION_RATE_LIMIT_MAX_KEY, source, purpose),
    windowMinutes: requirePositiveInt(
      values,
      AC_REGISTRATION_RATE_LIMIT_WINDOW_KEY,
      source,
      purpose,
    ),
  };
}

/**
 * The worst-case successful AC token issues that can share ONE window with the
 * cohort's creation, as named terms rather than a bare factor:
 *   - `registrations`: global setup registers all N+1 back-to-back (each
 *     `/register` is a successful `user_login` row);
 *   - `firstTestSignIns`: the first multi-party test signs every member in (one
 *     browsing context each — the app's auth session is per-context in-memory
 *     state, so there is no sign-in-free way in), and it can start within the
 *     same window as global setup.
 */
export function cohortWindowSpend(receiveSlots: number): {
  readonly registrations: number;
  readonly firstTestSignIns: number;
  readonly total: number;
} {
  const registrations = cohortSize(receiveSlots);
  const firstTestSignIns = cohortSize(receiveSlots);
  return { registrations, firstTestSignIns, total: registrations + firstTestSignIns };
}

/**
 * Fail at SETUP, not as mid-suite 429s, when the cohort cannot fit the window
 * ({@link cohortWindowSpend} names the terms).
 *
 * NECESSARY, NOT SUFFICIENT — the per-IP bucket is shared with every other
 * spec's sign-ins and with the Rust env-tests run from the same host just
 * before this suite (their logins from the preceding window count too, and no
 * static check can see them; e2e/README.md §Budgets). This catches the
 * structural misfit (N raised past the limit, or a prod-default AC config)
 * before any account is created.
 */
export function assertCohortFitsAuthWindow(receiveSlots: number, limit: AcAuthRateLimit): void {
  const spend = cohortWindowSpend(receiveSlots);
  if (spend.total > limit.maxAttempts) {
    throw new Error(
      `the N+1 cohort (N=${receiveSlots}) needs ${spend.total} successful AC token issues in one ` +
        `${limit.windowMinutes}-minute window (${spend.registrations} registrations + ` +
        `${spend.firstTestSignIns} first-test sign-ins), above the configured ` +
        `${AC_REGISTRATION_RATE_LIMIT_MAX_KEY}=${limit.maxAttempts}. Lower N or target a cluster whose AC ` +
        `config allows it — never raise workers (e2e/README.md §Budgets).`,
    );
  }
}
