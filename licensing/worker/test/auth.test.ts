import { env } from 'cloudflare:workers';
import { beforeEach, describe, expect, it } from 'vitest';
import { sha256Hex } from '../src/lib/ids';
import { countRows, past } from './support/db.ts';
import { Browser } from './support/portal.ts';
import { FakeUpstream, json } from './support/upstream.ts';
import {
  completeOAuth,
  finishOAuth,
  idToken,
  newGitHubAccount,
  signInAsGitHubAccount,
  signInAsGoogleAccount,
  signedInBrowser,
  startOAuth
} from './support/identity.ts';

let upstream: FakeUpstream;

beforeEach(() => {
  upstream = new FakeUpstream().install();
});

const sessionCookie = '__Host-styrhous-session';
const stateCookie = '__Host-styrhous-oauth';

describe('sign-in', () => {
  it('creates an account with a personal seat and trial on first sign-in', async () => {
    const { browser, userId } = await signedInBrowser(upstream, 'ada@example.com');

    const session = await browser.json('/auth/session');
    expect(session.body).toEqual({
      authenticated: true,
      userId,
      email: 'ada@example.com',
      linkedProviders: ['github'],
      configuredProviders: ['github', 'google', 'microsoft'],
      recentlyAuthenticated: true
    });

    const entitlements = await browser.json<{ entitlements: Record<string, unknown>[] }>('/api/entitlements');
    expect(entitlements.body.entitlements).toHaveLength(1);
    expect(entitlements.body.entitlements[0]).toMatchObject({
      state: 'trial',
      reasonCode: 'active_trial',
      isEligible: true
    });
  });

  it('returns the same account on a repeated sign-in', async () => {
    const account = newGitHubAccount('repeat@example.com');
    signInAsGitHubAccount(upstream, account);
    const first = new Browser();
    await completeOAuth(first, '/auth/sign-in/github');
    const second = new Browser();
    await completeOAuth(second, '/auth/sign-in/github');

    const [a, b] = await Promise.all([first.json('/auth/session'), second.json('/auth/session')]);
    expect(a.body.userId).toBe(b.body.userId);
  });

  it('creates one account when the same identity signs in twice at once', async () => {
    signInAsGitHubAccount(upstream, newGitHubAccount('simultaneous@example.com'));
    const browsers = [new Browser(), new Browser()];
    const states = await Promise.all(browsers.map((browser) => startOAuth(browser, '/auth/sign-in/github')));
    await Promise.all(browsers.map((browser, index) => finishOAuth(browser, states[index]!)));

    const sessions = await Promise.all(browsers.map((browser) => browser.json('/auth/session')));
    expect(sessions[0]!.body.userId).toBe(sessions[1]!.body.userId);
    expect(await countRows("SELECT count(*) AS total FROM users WHERE email = 'simultaneous@example.com'")).toBe(1);
  });

  it('issues a hardened session cookie and replaces the browser\'s previous session', async () => {
    signInAsGitHubAccount(upstream, newGitHubAccount('cookie@example.com'));
    const browser = new Browser();
    const state = await startOAuth(browser, '/auth/sign-in/github');
    const callback = await browser.fetch(`/auth/callback/github?code=authorization-code&state=${state}`);
    const issued = callback.headers.getSetCookie().find((cookie) => cookie.startsWith(`${sessionCookie}=`))!;
    expect(issued).toMatch(/Max-Age=2592000/);
    expect(issued).toMatch(/Path=\//);
    expect(issued).toMatch(/HttpOnly/);
    expect(issued).toMatch(/Secure/);
    expect(issued).toMatch(/SameSite=Lax/);
    const firstUserId = (await browser.json('/auth/session')).body.userId;

    signInAsGitHubAccount(upstream, newGitHubAccount('cookie-second@example.com'));
    await completeOAuth(browser, '/auth/sign-in/github');
    expect((await browser.json('/auth/session')).body.email).toBe('cookie-second@example.com');
    expect(await countRows('SELECT count(*) AS total FROM sessions WHERE user_id = ?', firstUserId)).toBe(0);
  });

  it('treats an expired session as signed out', async () => {
    const { browser, userId } = await signedInBrowser(upstream, 'expired-session@example.com');
    await env.DB.prepare('UPDATE sessions SET expires_at = ? WHERE user_id = ?').bind(past, userId).run();
    expect((await browser.json('/auth/session')).body.authenticated).toBe(false);
    expect(await browser.json('/api/entitlements')).toEqual({ status: 401, body: { reasonCode: 'authentication_required' } });
  });

  it('requires an explicit link when another provider presents a known email', async () => {
    await signedInBrowser(upstream, 'grace@example.com');
    signInAsGoogleAccount(upstream, { sub: 'google-grace', email: 'GRACE@example.com', email_verified: true });

    const browser = new Browser();
    const location = await completeOAuth(browser, '/auth/sign-in/google?returnUrl=/account', 'google');
    expect(location).toBe('/account?authenticationError=account_link_required');
    expect((await browser.json('/auth/session')).body.authenticated).toBe(false);
  });

  it('rejects identities without a verified email', async () => {
    signInAsGitHubAccount(upstream, { id: 77, email: 'unverified@example.com', verified: false });
    expect(await completeOAuth(new Browser(), '/auth/sign-in/github?returnUrl=/billing')).toBe(
      '/billing?authenticationError=verified_email_required'
    );

    signInAsGoogleAccount(upstream, { sub: 'google-unverified', email: 'unverified@example.com', email_verified: false });
    expect(await completeOAuth(new Browser(), '/auth/sign-in/google', 'google')).toBe(
      '/?authenticationError=verified_email_required'
    );
  });

  it('prefers the primary verified GitHub email', async () => {
    const account = newGitHubAccount('primary@example.com');
    signInAsGitHubAccount(upstream, account);
    upstream.on('GET', 'https://api.github.com/user/emails', () =>
      json([
        { email: 'unverified-primary@example.com', verified: false, primary: false },
        { email: 'secondary@example.com', verified: true, primary: false },
        { email: 'primary@example.com', verified: true, primary: true }
      ])
    );
    const browser = new Browser();
    await completeOAuth(browser, '/auth/sign-in/github');
    expect((await browser.json('/auth/session')).body.email).toBe('primary@example.com');
  });

  it('only accepts Microsoft emails that Entra vouches for', async () => {
    const tenant = '9188040d-6c67-4c5b-b112-36a304b66dad';
    const tokenUrl = 'https://login.microsoftonline.com/common/oauth2/v2.0/token';
    const signInWith = async (claims: Record<string, unknown>, browser = new Browser()) => {
      upstream.on('POST', tokenUrl, () =>
        json({
          access_token: 'ms-token',
          token_type: 'Bearer',
          id_token: idToken({ iss: `https://login.microsoftonline.com/${tenant}/v2.0`, tid: tenant, ...claims })
        })
      );
      return completeOAuth(browser, '/auth/sign-in/microsoft', 'microsoft');
    };

    expect(await signInWith({ oid: 'object-1', email: 'unvouched@example.com' })).toBe(
      '/?authenticationError=microsoft_verified_email_required'
    );
    expect(await signInWith({ oid: 'object-1', email: 'x@example.com', xms_edov: true, iss: 'https://evil.example/v2.0' })).toBe(
      '/?authenticationError=invalid_external_identity'
    );
    expect(await signInWith({ oid: 'object-1', email: 'x@example.com', xms_edov: true, tid: 'not-a-tenant' })).toBe(
      '/?authenticationError=invalid_external_identity'
    );

    const vouched = new Browser();
    expect(await signInWith({ oid: 'object-1', email: 'owner@example.com', xms_edov: true }, vouched)).toBe('/');
    expect((await vouched.json('/auth/session')).body.email).toBe('owner@example.com');

    const verifiedPrimary = new Browser();
    expect(await signInWith({ oid: 'object-2', verified_primary_email: ['  primary-ms@example.com '] }, verifiedPrimary)).toBe('/');
    expect((await verifiedPrimary.json('/auth/session')).body.email).toBe('primary-ms@example.com');
  });

  it('reports a failing provider token endpoint', async () => {
    upstream.on('POST', 'https://github.com/login/oauth/access_token', () => json({ error: 'server_error' }, 500));
    expect(await completeOAuth(new Browser(), '/auth/sign-in/github')).toBe(
      '/?authenticationError=provider_authentication_failed'
    );
  });
});

describe('OAuth state', () => {
  const invalid = '/?authenticationError=authentication_operation_invalid';

  it('rejects unknown, replayed, and expired states', async () => {
    expect(await finishOAuth(new Browser(), 'forged')).toBe(invalid);

    signInAsGitHubAccount(upstream, newGitHubAccount('replay@example.com'));
    const browser = new Browser();
    const state = await startOAuth(browser, '/auth/sign-in/github');
    expect(await finishOAuth(browser, state)).toBe('/');
    expect(browser.cookie(stateCookie)).toBeUndefined();
    // Even with the browser binding restored, a consumed state cannot be used again.
    browser.setCookie(stateCookie, state);
    expect(await finishOAuth(browser, state)).toBe(invalid);

    const late = new Browser();
    const lateState = await startOAuth(late, '/auth/sign-in/github');
    await env.DB.prepare('UPDATE oauth_states SET expires_at = ? WHERE state_hash = ?').bind(past, await sha256Hex(lateState)).run();
    expect(await finishOAuth(late, lateState)).toBe(invalid);
  });

  it('only completes in the browser that started the sign-in', async () => {
    signInAsGitHubAccount(upstream, newGitHubAccount('login-csrf@example.com'));
    const attacker = new Browser();
    const state = await startOAuth(attacker, '/auth/sign-in/github');

    const victim = new Browser();
    expect(await finishOAuth(victim, state)).toBe(invalid);
    expect((await victim.json('/auth/session')).body.authenticated).toBe(false);
  });

  it('rejects a callback for a different provider', async () => {
    const browser = new Browser();
    const state = await startOAuth(browser, '/auth/sign-in/github?returnUrl=/account');
    expect(await finishOAuth(browser, state, 'google')).toBe(
      '/account?authenticationError=authentication_provider_changed'
    );
  });

  it('only returns to paths on the portal origin', async () => {
    signInAsGitHubAccount(upstream, newGitHubAccount('redirect@example.com'));
    const returnTo = async (returnUrl: string) =>
      completeOAuth(new Browser(), `/auth/sign-in/github?returnUrl=${encodeURIComponent(returnUrl)}`);

    for (const unsafe of ['//evil.example', '/\t/evil.example/x', '/\\evil.example', 'https://evil.example/', 'billing']) {
      expect(await returnTo(unsafe), JSON.stringify(unsafe)).toBe('/');
    }
    expect(await returnTo('/billing?tab=seats#top')).toBe('/billing?tab=seats#top');
  });
});

describe('linked providers', () => {
  it('links a provider with a matching verified email and refuses to unlink the last one', async () => {
    const { browser } = await signedInBrowser(upstream, 'linker@example.com');
    signInAsGoogleAccount(upstream, { sub: 'google-linker', email: 'linker@example.com', email_verified: true });
    expect(await completeOAuth(browser, '/auth/link/google?returnUrl=/account', 'google')).toBe('/account');
    expect((await browser.json('/auth/session')).body.linkedProviders).toEqual(['github', 'google']);

    expect((await browser.json('/auth/providers/google', 'DELETE')).body).toEqual({ reasonCode: 'provider_unlinked' });
    expect(await browser.json('/auth/providers/github', 'DELETE')).toEqual({
      status: 409,
      body: { reasonCode: 'last_provider_required' }
    });
    expect(await browser.json('/auth/providers/google', 'DELETE')).toEqual({
      status: 404,
      body: { reasonCode: 'provider_not_linked' }
    });
    expect(await browser.json('/auth/providers/bogus', 'DELETE')).toEqual({
      status: 404,
      body: { reasonCode: 'provider_not_linked' }
    });
  });

  it('refuses to link a provider whose email belongs to nobody on this account', async () => {
    const { browser } = await signedInBrowser(upstream, 'mismatch@example.com');
    signInAsGoogleAccount(upstream, { sub: 'google-other', email: 'other@example.com', email_verified: true });
    expect(await completeOAuth(browser, '/auth/link/google?returnUrl=/account', 'google')).toBe(
      '/account?authenticationError=verified_email_mismatch'
    );
  });

  it('refuses to link an identity that belongs to another account', async () => {
    signInAsGoogleAccount(upstream, { sub: 'google-taken', email: 'taken@example.com', email_verified: true });
    await completeOAuth(new Browser(), '/auth/sign-in/google', 'google');

    const { browser } = await signedInBrowser(upstream, 'claimant@example.com');
    signInAsGoogleAccount(upstream, { sub: 'google-taken', email: 'claimant@example.com', email_verified: true });
    expect(await completeOAuth(browser, '/auth/link/google', 'google')).toBe('/?authenticationError=provider_already_linked');
  });

  it('only finishes a link in the session that started it', async () => {
    const { browser } = await signedInBrowser(upstream, 'switcher@example.com');
    const state = await startOAuth(browser, '/auth/link/google');
    await browser.fetch('/auth/sign-out', { method: 'POST' });
    signInAsGoogleAccount(upstream, { sub: 'google-switcher', email: 'switcher@example.com', email_verified: true });
    expect(await finishOAuth(browser, state, 'google')).toBe('/?authenticationError=authentication_session_changed');
  });

  it('requires recent authentication to change providers', async () => {
    const { browser, userId } = await signedInBrowser(upstream, 'stale@example.com');
    await env.DB.prepare('UPDATE sessions SET auth_time = ? WHERE user_id = ?').bind(past, userId).run();
    expect((await browser.json('/auth/link/google')).body).toEqual({ reasonCode: 'recent_authentication_required' });
    expect(await browser.json('/auth/providers/github', 'DELETE')).toEqual({
      status: 403,
      body: { reasonCode: 'recent_authentication_required' }
    });
    expect((await browser.json('/auth/session')).body.recentlyAuthenticated).toBe(false);

    // Re-authenticating as someone else does not count.
    signInAsGitHubAccount(upstream, newGitHubAccount('stale@example.com'));
    expect(await completeOAuth(browser, '/auth/reauth/github?returnUrl=/account')).toBe(
      '/account?authenticationError=provider_not_linked'
    );

    // Re-authenticating with the linked provider restores the window.
    signInAsGitHubAccount(upstream, { id: await providerSubject(userId), email: 'stale@example.com' });
    expect(await completeOAuth(browser, '/auth/reauth/github?returnUrl=/account')).toBe('/account');
    expect((await browser.json('/auth/session')).body.recentlyAuthenticated).toBe(true);
  });
});

describe('browser protections', () => {
  it('rejects cookie-authenticated mutations from another origin', async () => {
    const { browser } = await signedInBrowser(upstream, 'csrf@example.com');
    const response = await browser.fetch('/auth/sign-out', { method: 'POST', headers: { Origin: 'https://evil.example' } });
    expect(response.status).toBe(403);
    expect((await browser.json('/auth/session')).body.authenticated).toBe(true);
  });

  it('ends the session on sign-out', async () => {
    const { browser } = await signedInBrowser(upstream, 'bye@example.com');
    const signOut = await browser.fetch('/auth/sign-out', { method: 'POST' });
    expect(signOut.status).toBe(204);
    expect(signOut.headers.getSetCookie().find((cookie) => cookie.startsWith(`${sessionCookie}=`))).toMatch(/Max-Age=0/);
    expect((await browser.json('/auth/session')).body.authenticated).toBe(false);
    expect((await browser.json('/api/entitlements')).status).toBe(401);
  });

  it('sets hardening and no-store headers on API responses', async () => {
    const response = await new Browser().fetch('/auth/providers');
    expect(response.headers.get('Cache-Control')).toBe('no-store');
    expect(response.headers.get('X-Frame-Options')).toBe('DENY');
    expect(response.headers.get('Content-Security-Policy')).toBe("frame-ancestors 'none'");
  });
});

async function providerSubject(userId: string): Promise<number> {
  const row = await env.DB.prepare("SELECT subject FROM external_identities WHERE user_id = ? AND provider = 'github'")
    .bind(userId)
    .first<{ subject: string }>();
  return Number(row!.subject);
}
