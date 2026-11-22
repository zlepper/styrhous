import { env, exports } from 'cloudflare:workers';
import { calculateJwkThumbprint, createLocalJWKSet, decodeJwt, exportJWK, generateKeyPair, jwtVerify } from 'jose';
import { beforeEach, describe, expect, it } from 'vitest';
import { leaseAudience, leaseKeys, signLease } from '../src/desktop/lease';
import { countRows, expireUserTrial, past } from './support/db.ts';
import { signedInBrowser } from './support/identity.ts';
import { createOrganization, licensedBrowser } from './support/organizations.ts';
import {
  approveDesktop,
  desktop,
  newInstallation,
  pollToken,
  signInDesktop,
  startAuthorization
} from './support/desktop.ts';
import { origin } from './support/portal.ts';
import { FakeUpstream } from './support/upstream.ts';
import { testLeaseKey } from './test-config.ts';

let upstream: FakeUpstream;

beforeEach(() => {
  upstream = new FakeUpstream().install();
});

type Devices = { activeDevices: { activationId: string; displayName: string }[] };

describe('device authorization', () => {
  it('signs a desktop installation in after browser approval and issues a verifiable lease', async () => {
    const { browser, userId } = await licensedBrowser(upstream, 'desktop@example.com');
    const installation = newInstallation('Laptop');
    const authorization = await startAuthorization(installation);
    expect(authorization.user_code).toMatch(/^[BCDFGHJKLMNPQRSTVWXZ]{4}-[BCDFGHJKLMNPQRSTVWXZ]{4}$/);
    expect(authorization.verification_uri_complete).toBe(`${origin}/devices/authorize?user_code=${authorization.user_code}`);

    expect((await pollToken(authorization.device_code)).body).toEqual({ error: 'authorization_pending' });
    expect((await pollToken(authorization.device_code)).body).toEqual({ error: 'slow_down' });

    const approval = await browser.json(`/api/device-authorizations/${authorization.user_code.toLowerCase().replace('-', '')}`);
    expect(approval.body).toMatchObject({
      reasonCode: 'device_authorization_awaiting_approval',
      installation: { installationId: installation.installation_id, displayName: 'Laptop' },
      eligibleSeats: [{ name: 'Acme', entitlementState: 'trial', canActivate: true, activeDevices: [] }]
    });
    const seatId = approval.body.selectedSeatId;
    const decision = await browser.json(`/api/device-authorizations/${authorization.user_code}`, 'POST', { decision: 'approve', seatId });
    expect(decision.status).toBe(204);

    const token = await pollToken(authorization.device_code);
    expect(Object.keys(token.body)).toEqual(['installation_token']);
    expect((await pollToken(authorization.device_code)).body).toEqual({ error: 'invalid_grant' });

    const entitlement = await desktop('/entitlement', { token: token.body.installation_token });
    expect(entitlement.status).toBe(200);
    expect(entitlement.body).toMatchObject({ reasonCode: 'offline_lease_issued', state: 'trial', entitlementReasonCode: 'active_trial' });

    const keys = await desktop('/keys');
    const { payload, protectedHeader } = await jwtVerify(entitlement.body.lease, createLocalJWKSet(keys.body as any), {
      issuer: `${origin}/`,
      audience: leaseAudience
    });
    expect(protectedHeader).toMatchObject({ alg: 'EdDSA', typ: 'JWT', kid: keys.body.keys[0].kid });
    expect(payload).toMatchObject({
      schema_version: 2,
      sub: userId,
      seat_id: seatId,
      installation_id: installation.installation_id,
      state: 'trial',
      reason_code: 'active_trial',
      signing_key_id: keys.body.keys[0].kid
    });
    expect(payload.exp! - payload.iat!).toBe(7 * 24 * 3600);
    expect((payload.refresh_after as number) - payload.iat!).toBe(24 * 3600);

    const devices = await browser.json<Devices>(`/api/seats/${seatId}/devices`);
    expect(devices.body.activeDevices.map((device) => device.displayName)).toEqual(['Laptop']);
  });

  it('reports denial and expiry to the polling client', async () => {
    const { browser } = await signedInBrowser(upstream, 'deny@example.com');
    const denied = await startAuthorization();
    expect((await browser.json(`/api/device-authorizations/${denied.user_code}`, 'POST', { decision: 'deny' })).status).toBe(204);
    expect((await pollToken(denied.device_code)).body).toEqual({ error: 'access_denied' });

    const expired = await startAuthorization();
    await env.DB.prepare('UPDATE device_authorizations SET expires_at = ? WHERE user_code = ?').bind(past, expired.user_code).run();
    expect((await pollToken(expired.device_code)).body).toEqual({ error: 'expired_token' });
    expect((await browser.json(`/api/device-authorizations/${expired.user_code}`)).body).toEqual({ reasonCode: 'device_authorization_not_found' });
  });

  it('validates installation metadata and token requests', async () => {
    const response = await desktop('/device/authorize', { form: { ...newInstallation(), installation_id: 'nope' } });
    expect(response).toMatchObject({ status: 400, body: { error: 'invalid_request' } });

    expect(await desktop('/token', { form: {} })).toMatchObject({ status: 400, body: { error: 'invalid_request' } });
    const authorization = await startAuthorization();
    const asJson = await exports.default.fetch(`${origin}/desktop/v2/token`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ device_code: authorization.device_code })
    });
    expect(asJson.status).toBe(400);
    expect(await asJson.json()).toEqual({ error: 'invalid_request' });
  });

  it('denies the token when the device was revoked before the client collected it', async () => {
    const { browser } = await licensedBrowser(upstream, 'revoked-early@example.com');
    const authorization = await startAuthorization(newInstallation('Early'));
    const seatId = await approveDesktop(browser, authorization.user_code);
    const devices = await browser.json<Devices>(`/api/seats/${seatId}/devices`);
    await browser.json(`/api/device-activations/${devices.body.activeDevices[0]!.activationId}`, 'DELETE');
    expect((await pollToken(authorization.device_code)).body).toEqual({ error: 'access_denied' });
  });

  it('requires an eligible seat of the caller for approval', async () => {
    const owner = await signedInBrowser(upstream, 'seat-choice@example.com');
    const other = await signedInBrowser(upstream, 'seat-other@example.com');
    const [personalSeat] = (await owner.browser.json<{ entitlements: { seatId: string }[] }>('/api/entitlements')).body.entitlements;
    const [otherSeat] = (await other.browser.json<{ entitlements: { seatId: string }[] }>('/api/entitlements')).body.entitlements;
    const authorization = await startAuthorization();
    const path = `/api/device-authorizations/${authorization.user_code}`;
    // Free use needs no license, so a personal seat alone offers nothing to approve with.
    expect((await owner.browser.json(path)).body).toMatchObject({ eligibleSeats: [], selectedSeatId: null });

    // The first organization starts a trial, making its seat the only eligible one.
    const organization = await createOrganization(owner.browser);

    const approval = await owner.browser.json<{ eligibleSeats: { seatId: string; billingAccountId: string }[] }>(path);
    expect(approval.body.eligibleSeats.map((seat) => seat.billingAccountId)).toEqual([organization.billingAccountId]);

    expect(await owner.browser.json(path, 'POST', { decision: 'approve' })).toEqual({
      status: 409,
      body: { reasonCode: 'device_authorization_seat_required' }
    });
    for (const seatId of [personalSeat!.seatId, otherSeat!.seatId]) {
      expect(await owner.browser.json(path, 'POST', { decision: 'approve', seatId })).toEqual({
        status: 409,
        body: { reasonCode: 'device_authorization_seat_not_eligible' }
      });
    }
  });

  it('enforces the device limit, replacing only stale devices', async () => {
    const { browser } = await licensedBrowser(upstream, 'limit@example.com');
    const signedIn = [];
    for (const name of ['One', 'Two', 'Three']) signedIn.push(await signInDesktop(browser, newInstallation(name)));

    const fourth = await startAuthorization(newInstallation('Four'));
    const seatId = signedIn[0]!.seatId;
    const blocked = await browser.json(`/api/device-authorizations/${fourth.user_code}`, 'POST', { decision: 'approve', seatId });
    expect(blocked.status).toBe(409);
    expect(blocked.body).toMatchObject({ reasonCode: 'device_limit_reached', activeDevices: expect.any(Array) });

    await env.DB.prepare("UPDATE device_activations SET last_seen_at = ? WHERE seat_id = ? AND display_name = 'Two'").bind(past, seatId).run();
    expect((await browser.json(`/api/device-authorizations/${fourth.user_code}`, 'POST', { decision: 'approve', seatId })).status).toBe(204);
    const devices = await browser.json<Devices>(`/api/seats/${seatId}/devices`);
    expect(devices.body.activeDevices.map((device) => device.displayName)).toEqual(['One', 'Three', 'Four']);
    expect((await desktop('/entitlement', { token: signedIn[1]!.token })).status).toBe(401);
  });

  it('frees the slot of an approved device that never collected its token', async () => {
    const { browser } = await licensedBrowser(upstream, 'abandoned@example.com');
    const one = await signInDesktop(browser, newInstallation('One'));
    await signInDesktop(browser, newInstallation('Two'));
    const abandoned = await startAuthorization(newInstallation('Abandoned'));
    await approveDesktop(browser, abandoned.user_code, one.seatId);

    const fourth = await startAuthorization(newInstallation('Four'));
    const path = `/api/device-authorizations/${fourth.user_code}`;
    // A sign-in that is still within its window keeps its slot.
    expect((await browser.json(path, 'POST', { decision: 'approve', seatId: one.seatId })).body).toMatchObject({
      reasonCode: 'device_limit_reached'
    });

    const elevenMinutesAgo = new Date(Date.now() - 11 * 60_000).toISOString();
    await env.DB.prepare("UPDATE device_activations SET last_seen_at = ? WHERE seat_id = ? AND display_name = 'Abandoned'")
      .bind(elevenMinutesAgo, one.seatId)
      .run();
    expect((await browser.json(path, 'POST', { decision: 'approve', seatId: one.seatId })).status).toBe(204);
    const devices = await browser.json<Devices>(`/api/seats/${one.seatId}/devices`);
    expect(devices.body.activeDevices.map((device) => device.displayName)).toEqual(['One', 'Two', 'Four']);
  });

  it('lets only one of two concurrent approvals take the last device slot', async () => {
    const { browser } = await licensedBrowser(upstream, 'last-slot@example.com');
    const { seatId } = await signInDesktop(browser, newInstallation('One'));
    await signInDesktop(browser, newInstallation('Two'));
    const pending = [await startAuthorization(newInstallation('Three')), await startAuthorization(newInstallation('Four'))];

    const results = await Promise.all(
      pending.map((authorization) =>
        browser.json(`/api/device-authorizations/${authorization.user_code}`, 'POST', { decision: 'approve', seatId })
      )
    );
    expect(results.map((result) => result.status).sort()).toEqual([204, 409]);
    expect(['device_limit_reached', 'device_authorization_concurrent_modification']).toContain(
      results.find((result) => result.status === 409)!.body.reasonCode
    );
    expect((await browser.json<Devices>(`/api/seats/${seatId}/devices`)).body.activeDevices).toHaveLength(3);
  });

  it('approves a sign-in only once when it is approved twice at once', async () => {
    const { browser } = await licensedBrowser(upstream, 'double-approve@example.com');
    const authorization = await startAuthorization();
    const path = `/api/device-authorizations/${authorization.user_code}`;
    const seatId = (await browser.json(path)).body.selectedSeatId;

    const results = await Promise.all([1, 2].map(() => browser.json(path, 'POST', { decision: 'approve', seatId })));
    const [approved, rejected] = [...results].sort((a, b) => a.status - b.status);
    expect(approved!.status).toBe(204);
    expect([404, 409]).toContain(rejected!.status);
    expect(await countRows('SELECT count(*) AS total FROM device_activations WHERE seat_id = ? AND revoked_at IS NULL', seatId)).toBe(1);
  });

  it('re-approving an installation reuses its activation and invalidates the old token', async () => {
    const { browser } = await licensedBrowser(upstream, 'reapprove@example.com');
    const installation = newInstallation('Desk');
    const first = await signInDesktop(browser, installation);
    const second = await signInDesktop(browser, installation);
    expect((await desktop('/entitlement', { token: first.token })).status).toBe(401);
    expect((await desktop('/entitlement', { token: second.token })).status).toBe(200);
    const devices = await browser.json<Devices>(`/api/seats/${second.seatId}/devices`);
    expect(devices.body.activeDevices).toHaveLength(1);
  });
});

describe('installation tokens', () => {
  it('rejects unknown tokens and stops working after portal revocation or sign-out', async () => {
    const unknown = await desktop('/entitlement', { token: 'not-a-token' });
    expect(unknown).toMatchObject({ status: 401, body: { reasonCode: 'invalid_installation_token' } });
    expect(unknown.headers.get('WWW-Authenticate')).toBe('Bearer');
    expect((await desktop('/sign-out', { token: 'not-a-token' })).status).toBe(204);

    const { browser } = await licensedBrowser(upstream, 'revoke@example.com');
    const revoked = await signInDesktop(browser, newInstallation('Revoked'));
    const devices = await browser.json<Devices>(`/api/seats/${revoked.seatId}/devices`);
    const revocation = await browser.json(`/api/device-activations/${devices.body.activeDevices[0]!.activationId}`, 'DELETE');
    expect(revocation.body).toMatchObject({ reasonCode: 'device_revoked' });
    expect((await desktop('/entitlement', { token: revoked.token })).status).toBe(401);

    const signedOut = await signInDesktop(browser, newInstallation('Signed out'));
    expect((await desktop('/sign-out', { token: signedOut.token })).status).toBe(204);
    expect((await desktop('/entitlement', { token: signedOut.token })).status).toBe(401);
  });

  it("refuses to list or revoke another user's devices", async () => {
    const owner = await licensedBrowser(upstream, 'device-owner@example.com');
    const intruder = await signedInBrowser(upstream, 'device-intruder@example.com');
    const signedIn = await signInDesktop(owner.browser);
    const [device] = (await owner.browser.json<Devices>(`/api/seats/${signedIn.seatId}/devices`)).body.activeDevices;

    expect(await intruder.browser.json(`/api/seats/${signedIn.seatId}/devices`)).toEqual({
      status: 404,
      body: { reasonCode: 'seat_not_found' }
    });
    expect(await intruder.browser.json(`/api/device-activations/${device!.activationId}`, 'DELETE')).toEqual({
      status: 404,
      body: { reasonCode: 'device_not_active' }
    });
    expect((await desktop('/entitlement', { token: signedIn.token })).status).toBe(200);
    expect(
      await countRows("SELECT count(*) AS total FROM audit_records WHERE action = 'device_revoked' AND target_id = ?", device!.activationId)
    ).toBe(0);
  });

  it('keeps the installation signed in but refuses a lease without entitlement', async () => {
    const { browser, userId } = await licensedBrowser(upstream, 'expired-desktop@example.com');
    const signedIn = await signInDesktop(browser);
    await expireUserTrial(userId);
    expect(await desktop('/entitlement', { token: signedIn.token })).toMatchObject({ status: 403, body: { reasonCode: 'trial_expired' } });
  });

  it('ends the lease with the entitlement', async () => {
    const { browser, userId } = await licensedBrowser(upstream, 'short-trial@example.com');
    const signedIn = await signInDesktop(browser);
    const trialEnd = new Date(Date.now() + 2 * 24 * 3600_000).toISOString();
    await env.DB.prepare('UPDATE trials SET ends_at = ? WHERE originating_user_id = ?').bind(trialEnd, userId).run();

    const entitlement = await desktop('/entitlement', { token: signedIn.token });
    expect(entitlement.body.expiresAt).toBe(trialEnd);
    const claims = decodeJwt(entitlement.body.lease);
    expect(claims.exp).toBe(Math.floor(new Date(trialEnd).getTime() / 1000));
    expect(claims.refresh_after as number).toBeLessThanOrEqual(claims.exp!);
  });
});

describe('lease keys', () => {
  it('publishes the current key first, followed by retired keys', async () => {
    const keys = (await desktop('/keys')).body.keys;
    expect(keys).toEqual([expect.objectContaining({ kty: 'OKP', crv: 'Ed25519', alg: 'EdDSA', use: 'sig', kid: expect.any(String) })]);
    expect(keys[0]).not.toHaveProperty('d');

    // Rotate: a new signing key, with the old public key retired.
    const { privateKey } = await generateKeyPair('EdDSA', { crv: 'Ed25519', extractable: true });
    const retired = { kty: testLeaseKey.kty, crv: testLeaseKey.crv, x: testLeaseKey.x };
    const rotated = {
      ...env,
      LEASE_SIGNING_KEY: JSON.stringify(await exportJWK(privateKey)),
      LEASE_PREVIOUS_PUBLIC_KEYS: JSON.stringify([retired])
    };
    const rotatedKeys = await leaseKeys(rotated);
    expect(rotatedKeys.keys).toHaveLength(2);
    expect(rotatedKeys.keys[0]!.kid).not.toBe(keys[0].kid);
    expect(rotatedKeys.keys[1]).toEqual({ ...retired, use: 'sig', alg: 'EdDSA', kid: await calculateJwkThumbprint(retired) });
    expect(rotatedKeys.keys.every((key) => !('d' in key))).toBe(true);

    // A lease cached before the rotation still verifies, and so does a new one.
    const subject = {
      userId: crypto.randomUUID(),
      installationId: crypto.randomUUID(),
      activationId: crypto.randomUUID(),
      entitlement: {
        seatId: crypto.randomUUID(),
        billingAccountId: crypto.randomUUID(),
        state: 'trial',
        reasonCode: 'active_trial',
        isEligible: true,
        validFrom: null,
        validUntil: null
      }
    } as const;
    const jwks = createLocalJWKSet(rotatedKeys);
    for (const signingEnv of [env, rotated]) {
      const { lease } = await signLease(signingEnv, subject);
      await expect(jwtVerify(lease, jwks, { issuer: `${origin}/`, audience: leaseAudience })).resolves.toBeDefined();
    }
  });
});
