import { describe, expect, it } from 'vitest';
import { TestTokenBuilder } from '../TestTokenBuilder.js';

describe('TestTokenBuilder', () => {
  it('produces UserClaims matching the canonical wire shape with synthetic defaults', () => {
    const claims = TestTokenBuilder.userClaims();
    expect(claims).toMatchObject({
      sub: expect.any(String),
      org_id: expect.any(String),
      email: expect.stringContaining('@example.test'),
      roles: ['user'],
      iat: expect.any(Number),
      exp: expect.any(Number),
      jti: expect.any(String),
    });
    expect(claims.exp).toBeGreaterThan(claims.iat);
    // Defaults must use synthetic, NOT real-looking PII.
    expect(claims.email.endsWith('@example.test')).toBe(true);
  });

  it('produces MeetingClaims with correct token_type discriminator and omits home_org_id by default', () => {
    const claims = TestTokenBuilder.meetingClaims({ role: 'host' });
    expect(claims.token_type).toBe('meeting');
    expect(claims.role).toBe('host');
    expect(claims.participant_type).toBe('member');
    expect(claims.capabilities).toEqual(['video', 'audio']);
    expect(claims.exp).toBeGreaterThan(claims.iat);
    expect('home_org_id' in claims).toBe(false);

    // Explicit override populates the optional field.
    const withHome = TestTokenBuilder.meetingClaims({ home_org_id: 'org-other' });
    expect(withHome.home_org_id).toBe('org-other');
  });
});
