import { generateCodeVerifier, generateState } from 'arctic';
import { and, eq, gt } from 'drizzle-orm';
import { Hono, type Context } from 'hono';
import { deleteCookie, getCookie, setCookie } from 'hono/cookie';
import type { AppEnv } from '../env';
import { oauthStates, type OAuthOperation } from '../db/schema';
import { fail } from '../http/errors';
import { sha256Hex } from '../lib/ids';
import { isoAfter, isoNow, minuteMs } from '../lib/time';
import {
  assertIdentityLinked,
  linkIdentity,
  linkedProviders,
  signIn,
  unlinkProvider
} from './identities';
import {
  AuthenticationFailure,
  configuredProviders,
  isProviderName,
  provider,
  type ProviderName
} from './providers';
import {
  endSession,
  isRecentlyAuthenticated,
  refreshAuthenticationTime,
  requireSession,
  startSession
} from './session';

const stateLifetimeMs = 10 * minuteMs;
/** Binds an OAuth round trip to the browser that started it, preventing login CSRF. */
const stateCookie = '__Host-styrhous-oauth';
const invalidOperation = '/?authenticationError=authentication_operation_invalid';

export const authRoutes = new Hono<AppEnv>();

authRoutes.get('/providers', (c) => c.json({ providers: configuredProviders(c.env) }));

authRoutes.get('/session', async (c) => {
  const configured = configuredProviders(c.env);
  const session = c.var.session;
  if (!session) return c.json({ authenticated: false, configuredProviders: configured });
  return c.json({
    authenticated: true,
    userId: session.userId,
    email: session.email,
    linkedProviders: await linkedProviders(c.var.db, session.userId),
    configuredProviders: configured,
    recentlyAuthenticated: isRecentlyAuthenticated(session)
  });
});

authRoutes.get('/sign-in/:provider', (c) => beginAuthentication(c, 'sign-in'));

authRoutes.get('/link/:provider', (c) => {
  if (!isRecentlyAuthenticated(requireSession(c))) fail(403, 'recent_authentication_required');
  return beginAuthentication(c, 'link');
});

authRoutes.get('/reauth/:provider', (c) => {
  requireSession(c);
  return beginAuthentication(c, 'reauth');
});

authRoutes.get('/callback/:provider', async (c) => {
  const state = c.req.query('state');
  const browserState = getCookie(c, stateCookie);
  deleteCookie(c, stateCookie, { secure: true, path: '/' });
  if (!state || state !== browserState) return c.redirect(invalidOperation);
  const [pending] = await c.var.db
    .delete(oauthStates)
    .where(and(eq(oauthStates.stateHash, await sha256Hex(state)), gt(oauthStates.expiresAt, isoNow())))
    .returning();
  if (!pending) return c.redirect(invalidOperation);

  const returnUrl = pending.returnUrl;
  try {
    if (pending.provider !== c.req.param('provider')) {
      throw new AuthenticationFailure('authentication_provider_changed');
    }
    const name = pending.provider as ProviderName;
    const session = c.var.session;
    if (pending.operation !== 'sign-in' && session?.id !== pending.sessionId) {
      throw new AuthenticationFailure('authentication_session_changed');
    }

    const code = c.req.query('code');
    const client = provider(c.env, name);
    if (!code || !client) throw new AuthenticationFailure('provider_authentication_failed');
    const identity = await client.identify(code, pending.codeVerifier).catch((error: unknown) => {
      if (error instanceof AuthenticationFailure) throw error;
      console.warn('External authentication failed', { provider: name, error: String(error) });
      throw new AuthenticationFailure('provider_authentication_failed');
    });

    switch (pending.operation) {
      case 'sign-in':
        await startSession(c, await signIn(c.var.db, name, identity), name);
        break;
      case 'link':
        await linkIdentity(c.var.db, session!.userId, name, identity);
        break;
      case 'reauth':
        await assertIdentityLinked(c.var.db, session!.userId, name, identity);
        await refreshAuthenticationTime(c.var.db, session!, name);
        break;
    }
    return c.redirect(returnUrl);
  } catch (error) {
    if (!(error instanceof AuthenticationFailure)) throw error;
    return c.redirect(withQuery(returnUrl, 'authenticationError', error.reasonCode));
  }
});

authRoutes.delete('/providers/:provider', async (c) => {
  const session = requireSession(c);
  const name = c.req.param('provider');
  if (!isRecentlyAuthenticated(session)) fail(403, 'recent_authentication_required');
  if (!isProviderName(name)) fail(404, 'provider_not_linked');
  await unlinkProvider(c.var.db, session.userId, name);
  return c.json({ reasonCode: 'provider_unlinked' });
});

authRoutes.post('/sign-out', async (c) => {
  await endSession(c, requireSession(c));
  return c.body(null, 204);
});

async function beginAuthentication(c: Context<AppEnv>, operation: OAuthOperation) {
  const name = c.req.param('provider') ?? '';
  const client = isProviderName(name) ? provider(c.env, name) : null;
  if (!client) fail(404, 'provider_not_configured');

  const state = generateState();
  const codeVerifier = generateCodeVerifier();
  await c.var.db.insert(oauthStates).values({
    stateHash: await sha256Hex(state),
    codeVerifier,
    provider: name,
    operation,
    returnUrl: safeReturnUrl(c.req.query('returnUrl'), c.env.PUBLIC_ORIGIN),
    sessionId: c.var.session?.id ?? null,
    expiresAt: isoAfter(Date.now(), stateLifetimeMs)
  });
  setCookie(c, stateCookie, state, {
    httpOnly: true,
    secure: true,
    sameSite: 'Lax',
    path: '/',
    maxAge: stateLifetimeMs / 1000
  });
  return c.redirect(client.authorizationUrl(state, codeVerifier).toString());
}

// Browsers drop tabs and newlines from URLs and treat backslashes as slashes.
const ambiguousUrlCharacters = /[\u0000-\u001f\u007f\\]/;

/** Only paths on the portal's own origin are accepted as post-authentication destinations. */
function safeReturnUrl(value: string | undefined, origin: string): string {
  if (!value?.startsWith('/') || ambiguousUrlCharacters.test(value)) return '/';
  const url = new URL(value, origin);
  return url.origin === origin ? `${url.pathname}${url.search}${url.hash}` : '/';
}

function withQuery(path: string, key: string, value: string): string {
  const url = new URL(path, 'https://portal.invalid');
  url.searchParams.set(key, value);
  return `${url.pathname}${url.search}${url.hash}`;
}
