import { describe, expect, it } from 'vitest';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { HOST_ROLE, readIsHost } from '../meetingRole.js';

function jwt(payload: unknown): string {
  const seg = (v: unknown): string =>
    btoa(JSON.stringify(v)).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
  return `${seg({ alg: 'EdDSA', typ: 'JWT' })}.${seg(payload)}.c2ln`;
}

describe('readIsHost', () => {
  it('is host only for the exact serde spelling "host"', () => {
    expect(readIsHost(jwt({ role: 'host', sub: 'u' }))).toEqual({ isHost: true });
    expect(readIsHost(jwt({ role: 'participant' }))).toEqual({ isHost: false });
    expect(readIsHost(jwt({ role: 'Host' }))).toEqual({ isHost: false });
    expect(readIsHost(jwt({ role: 'host ' }))).toEqual({ isHost: false });
  });

  it.each([
    ['', 'empty'],
    ['x'.repeat(9000), 'oversized'],
    ['a.b', 'not_a_jwt'],
    ['a.!!!.c', 'undecodable'],
    [`a.${btoa('not json')}.c`, 'undecodable'],
  ])('fails closed (not host) with a bounded reason: %#', (token, failure) => {
    expect(readIsHost(token)).toEqual({ isHost: false, failure });
  });

  it('a payload without a string role is not host', () => {
    expect(readIsHost(jwt({ sub: 'u' }))).toEqual({ isHost: false, failure: 'no_role_claim' });
    expect(readIsHost(jwt({ role: 1 }))).toEqual({ isHost: false, failure: 'no_role_claim' });
    expect(readIsHost(jwt(null))).toEqual({ isHost: false, failure: 'no_role_claim' });
  });

  it('returns only the boolean and a reason: no claim values leak out', () => {
    const result = readIsHost(jwt({ role: 'host', sub: 'user-secret', email: 'a@b.c' }));
    expect(JSON.stringify(result)).not.toMatch(/user-secret|a@b\.c/);
  });
});

describe('HOST_ROLE drift guard — the Rust enum is the source of truth', () => {
  const REPO = resolve(__dirname, '../../../../..');
  const JWT_RS = resolve(REPO, 'crates/common/src/jwt.rs');

  /** The serde spelling of `MeetingRole::Host`, derived from the Rust source. */
  function rustHostSpelling(source: string): string {
    const m = /#\[serde\(rename_all = "([a-z_]+)"\)\]\s*pub enum MeetingRole \{([^}]*)\}/.exec(
      source,
    );
    if (m === null) throw new Error(`enum-not-found: no serde-annotated MeetingRole in ${JWT_RS}`);
    const [, rule, body] = m as unknown as [string, string, string];
    if (!/\bHost\b/.test(body))
      throw new Error('variant-not-found: MeetingRole has no Host variant');
    if (rule !== 'lowercase')
      throw new Error(`unhandled rename_all rule "${rule}": extend this test`);
    return 'Host'.toLowerCase();
  }

  it('POSITIVE CONTROL: the extractor finds the enum, and refuses a source without it', () => {
    expect(() => rustHostSpelling('pub enum Other {}')).toThrow(/enum-not-found/);
  });

  it('HOST_ROLE equals the serde spelling of MeetingRole::Host', () => {
    expect(HOST_ROLE).toBe(rustHostSpelling(readFileSync(JWT_RS, 'utf8')));
  });

  it("the test-utils token builder's host role agrees too", () => {
    const tokenClaims = readFileSync(
      resolve(REPO, 'packages/test-utils/src/token-claims.ts'),
      'utf8',
    );
    expect(tokenClaims).toContain(`'${HOST_ROLE}'`);
  });
});
