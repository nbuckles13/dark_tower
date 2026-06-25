// File: packages/sdk-core/src/http/origin.ts
//
// R-11: build the subdomain-qualified AC origin. AC is addressed per-org by subdomain
// (ADR-0020), e.g. `https://demo.localhost:8443`. The subdomain is validated against
// the anchored DNS-label regex BEFORE interpolation (injection prevention) — the
// validation lives in `validation/limits.ts` (single source of the rule).
//
// The origin TEMPLATE is supplied by the caller (config), never hardcoded here. The
// template uses a literal `{subdomain}` placeholder, e.g.
// `https://{subdomain}.localhost:8443`. We substitute only after the subdomain has
// passed `validateSubdomain`, so the substituted value can contain no URL-control
// characters (the regex admits only `[a-z0-9-]`).

import { validateSubdomain } from '../validation/limits.js';

/** The placeholder the AC origin template must contain. */
const SUBDOMAIN_PLACEHOLDER = '{subdomain}';

/**
 * Resolve the subdomain-qualified AC origin from a template + subdomain.
 * Validates the subdomain first (throws `ValidationError` on a bad subdomain; throws
 * a plain `Error` when the template is missing the `{subdomain}` placeholder — that's
 * a config bug, not user input).
 */
export function resolveAcOrigin(template: string, subdomain: string): string {
  validateSubdomain(subdomain);
  if (!template.includes(SUBDOMAIN_PLACEHOLDER)) {
    // Misconfiguration, not user input — surface clearly.
    throw new Error(`AC origin template must contain the ${SUBDOMAIN_PLACEHOLDER} placeholder`);
  }
  return template.split(SUBDOMAIN_PLACEHOLDER).join(subdomain);
}
