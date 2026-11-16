import { createScheduledController } from 'cloudflare:test';
import { env } from 'cloudflare:workers';
import worker from '../src/index';
import { sha256Hex } from '../src/lib/ids';
import { beforeEach, expect, it } from 'vitest';
import { countRows, past } from './support/db.ts';
import { signedInBrowser, startOAuth } from './support/identity.ts';
import { startAuthorization } from './support/desktop.ts';
import { captureEmails, createOrganization } from './support/organizations.ts';
import { Browser } from './support/portal.ts';
import { FakeUpstream } from './support/upstream.ts';

let upstream: FakeUpstream;

beforeEach(() => {
  upstream = new FakeUpstream().install();
});

const daysAgo = (days: number) => new Date(Date.now() - days * 24 * 3600_000).toISOString();

it('deletes expired short-lived records on the daily schedule', async () => {
  const live = await signedInBrowser(upstream, 'live@example.com');
  const expired = await signedInBrowser(upstream, 'expired@example.com');
  const liveAuthorization = await startAuthorization();
  const expiredAuthorization = await startAuthorization();
  const liveState = await startOAuth(new Browser(), '/auth/sign-in/github');
  const expiredState = await startOAuth(new Browser(), '/auth/sign-in/github');

  const organization = await createOrganization(live.browser);
  captureEmails();
  const invite = async (email: string) =>
    (await live.browser.json<{ invitationId: string }>(`/api/organizations/${organization.organizationId}/invitations`, 'POST', {
      email,
      role: 'member',
      assignProductSeat: false
    })).body.invitationId;
  const longExpired = await invite('long-expired@example.com');
  const recentlyExpired = await invite('recently-expired@example.com');
  const acceptedLongAgo = await invite('accepted@example.com');

  await env.DB.batch([
    env.DB.prepare('UPDATE sessions SET expires_at = ? WHERE user_id = ?').bind(past, expired.userId),
    env.DB.prepare('UPDATE device_authorizations SET expires_at = ? WHERE user_code = ?').bind(past, expiredAuthorization.user_code),
    env.DB.prepare('UPDATE oauth_states SET expires_at = ? WHERE state_hash = ?').bind(past, await sha256Hex(expiredState)),
    env.DB.prepare('UPDATE invitations SET expires_at = ? WHERE id IN (?, ?)').bind(daysAgo(31), longExpired, acceptedLongAgo),
    env.DB.prepare('UPDATE invitations SET accepted_at = ?, accepted_by_user_id = ? WHERE id = ?').bind(daysAgo(35), live.userId, acceptedLongAgo),
    env.DB.prepare('UPDATE invitations SET expires_at = ? WHERE id = ?').bind(daysAgo(29), recentlyExpired)
  ]);

  await worker.scheduled(createScheduledController(), env);

  expect((await expired.browser.json('/auth/session')).body.authenticated).toBe(false);
  expect((await live.browser.json('/auth/session')).body.authenticated).toBe(true);
  const userCodes = 'SELECT count(*) AS total FROM device_authorizations WHERE user_code = ?';
  expect(await countRows(userCodes, expiredAuthorization.user_code)).toBe(0);
  expect(await countRows(userCodes, liveAuthorization.user_code)).toBe(1);
  const states = 'SELECT count(*) AS total FROM oauth_states WHERE state_hash = ?';
  expect(await countRows(states, await sha256Hex(expiredState))).toBe(0);
  expect(await countRows(states, await sha256Hex(liveState))).toBe(1);
  const invitation = 'SELECT count(*) AS total FROM invitations WHERE id = ?';
  expect(await countRows(invitation, longExpired)).toBe(0);
  expect(await countRows(invitation, recentlyExpired)).toBe(1);
  expect(await countRows(invitation, acceptedLongAgo)).toBe(1);
});
