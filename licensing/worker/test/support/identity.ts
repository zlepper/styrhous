import { expect } from 'vitest';
import { Browser } from './portal.ts';
import { FakeUpstream, json } from './upstream.ts';

export type FakeAccount = Readonly<{ id: number; email: string; verified?: boolean }>;

let nextGitHubId = 1000;

export function newGitHubAccount(email: string): FakeAccount {
  return { id: nextGitHubId++, email };
}

/** Makes the fake GitHub sign in the given account on the next authorization-code exchange. */
export function signInAsGitHubAccount(upstream: FakeUpstream, account: FakeAccount) {
  const token = `gho_${account.id}`;
  upstream
    .on('POST', 'https://github.com/login/oauth/access_token', () =>
      json({ access_token: token, token_type: 'bearer', scope: 'user:email' })
    )
    .on('GET', 'https://api.github.com/user', () => json({ id: account.id }))
    .on('GET', 'https://api.github.com/user/emails', () =>
      json([{ email: account.email, verified: account.verified ?? true, primary: true }])
    );
}

/** An unsigned JWT; the Worker trusts ID tokens received directly from the token endpoint. */
export function idToken(claims: Record<string, unknown>): string {
  const encode = (value: unknown) => btoa(JSON.stringify(value)).replaceAll('=', '').replaceAll('+', '-').replaceAll('/', '_');
  return `${encode({ alg: 'RS256', typ: 'JWT' })}.${encode(claims)}.signature`;
}

/** Makes the fake Google token endpoint return an ID token with these claims. */
export function signInAsGoogleAccount(upstream: FakeUpstream, claims: Record<string, unknown>) {
  upstream.on('POST', 'https://oauth2.googleapis.com/token', () =>
    json({ access_token: 'google-token', token_type: 'Bearer', id_token: idToken(claims) })
  );
}

/** Starts an OAuth round trip and returns the state the provider would echo back. */
export async function startOAuth(browser: Browser, startPath: string): Promise<string> {
  const start = await browser.fetch(startPath);
  expect(start.status).toBe(302);
  return new URL(start.headers.get('Location')!).searchParams.get('state')!;
}

/** Delivers the provider's redirect back to the Worker and returns the final redirect location. */
export async function finishOAuth(browser: Browser, state: string, provider = 'github'): Promise<string> {
  const callback = await browser.fetch(`/auth/callback/${provider}?code=authorization-code&state=${state}`);
  expect(callback.status).toBe(302);
  return callback.headers.get('Location')!;
}

/** Runs the OAuth redirect round trip and returns the final redirect location. */
export async function completeOAuth(browser: Browser, startPath: string, provider = 'github'): Promise<string> {
  return finishOAuth(browser, await startOAuth(browser, startPath), provider);
}

/** Signs a fresh browser in through GitHub as a newly created account. */
export async function signedInBrowser(upstream: FakeUpstream, email: string) {
  const account = newGitHubAccount(email);
  signInAsGitHubAccount(upstream, account);
  const browser = new Browser();
  const location = await completeOAuth(browser, '/auth/sign-in/github?returnUrl=/billing');
  expect(location).toBe('/billing');
  const session = await browser.json<{ userId: string }>('/auth/session');
  return { browser, account, userId: session.body.userId };
}
