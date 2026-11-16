import { env } from 'cloudflare:workers';
import { describe, expect, it } from 'vitest';
import { leaseAudience, leaseKeys, signLease, type LeaseOptions } from '../src/desktop/lease';
import identifierShapeFixture from '../../protocol-fixtures/offline-lease-identifier-shape.json';
import leaseFixture from '../../protocol-fixtures/offline-lease-v2.json';

// Ed25519 signatures are deterministic, so a fixed key, time, and token id reproduce the
// committed fixtures byte for byte. The Rust client verifies the same files.
const issuedAt = 2_000_000_000;
const subject = {
  userId: '01991f2a-1111-7000-8000-000000000001',
  installationId: '01991f2a-1111-7000-8000-000000000004',
  activationId: '01991f2a-1111-7000-8000-000000000005',
  entitlement: {
    seatId: '01991f2a-1111-7000-8000-000000000002',
    billingAccountId: '01991f2a-1111-7000-8000-000000000003',
    state: 'commercial',
    reasonCode: 'active_subscription',
    isEligible: true,
    validFrom: '2033-05-01T00:00:00.000Z',
    validUntil: '2033-07-01T00:00:00.000Z'
  }
} as const;

const sign = async (jti: string) =>
  (await signLease(env, subject, { now: new Date(issuedAt * 1000), jti } satisfies LeaseOptions)).lease;

async function expectedFixture(jti: string) {
  return {
    issuer: `${env.PUBLIC_ORIGIN}/`,
    audience: leaseAudience,
    installationId: subject.installationId,
    keys: await leaseKeys(env),
    lease: await sign(jti),
    expected: {
      state: subject.entitlement.state,
      reasonCode: subject.entitlement.reasonCode,
      issuedAt,
      expiresAt: issuedAt + 7 * 24 * 3600,
      refreshAfter: issuedAt + 24 * 3600
    }
  };
}

describe('desktop protocol fixtures', () => {
  // On an intentional lease format change, replace the fixture files with the printed values.
  it('matches offline-lease-v2.json', async () => {
    const expected = await expectedFixture('01991f2a-1111-7000-8000-000000000006');
    expect(leaseFixture, JSON.stringify(expected, null, 2)).toEqual(expected);
  });

  it('matches offline-lease-identifier-shape.json', async () => {
    const expected = {
      ...(await expectedFixture('6f9619ff-8b86-4d11-b42d-00c04fc964ff')),
      malformedIdentifierLease: await sign('not-a-uuid')
    };
    expect(identifierShapeFixture, JSON.stringify(expected, null, 2)).toEqual(expected);
  });
});
