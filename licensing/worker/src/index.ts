import { Hono } from 'hono';
import type { AppEnv } from './env';
import { authRoutes } from './auth/routes';
import { loadSession } from './auth/session';
import { billingRoutes } from './billing/routes';
import { stripeWebhookRoutes } from './billing/webhook';
import { deleteExpiredRecords } from './cleanup';
import { createDatabase, database } from './db/client';
import { deviceApprovalRoutes } from './desktop/approval';
import { desktopRoutes } from './desktop/routes';
import { deviceRoutes } from './devices/routes';
import { entitlementRoutes } from './entitlements/routes';
import { ApiError, errorResponse } from './http/errors';
import { noStore, sameOriginMutations, securityHeaders } from './http/security';
import { invitationAcceptanceRoutes } from './organizations/invitations';
import { organizationRoutes } from './organizations/routes';

/** Paths the Worker answers itself; everything else is the static portal. */
const backendPrefixes = ['/api', '/auth', '/desktop'] as const;

const app = new Hono<AppEnv>();

app.use(securityHeaders);
for (const prefix of backendPrefixes) app.use(`${prefix}/*`, noStore, database);

app.get('/health', (c) => c.json({ status: 'healthy' }));

// Stripe authenticates webhooks with signatures; they carry no session or Origin.
app.route('/api/webhooks/stripe', stripeWebhookRoutes);

// The desktop protocol uses bearer installation tokens rather than cookies.
app.route('/desktop/v2', desktopRoutes);

// The portal's API uses the session cookie. Portal pages and assets never touch the session.
const browser = new Hono<AppEnv>();
for (const prefix of ['/api', '/auth']) browser.use(`${prefix}/*`, sameOriginMutations, loadSession);
browser.route('/auth', authRoutes);
browser.route('/api/entitlements', entitlementRoutes);
browser.route('/api/organizations', organizationRoutes);
browser.route('/api/invitations', invitationAcceptanceRoutes);
browser.route('/api/device-authorizations', deviceApprovalRoutes);
browser.route('/api', billingRoutes);
browser.route('/api', deviceRoutes);
app.route('/', browser);

const isBackendPath = (path: string) =>
  backendPrefixes.some((prefix) => path === prefix || path.startsWith(`${prefix}/`));

// Unknown backend paths are JSON 404s; everything else is the portal (with SPA fallback).
app.notFound((c) =>
  isBackendPath(c.req.path)
    ? c.json({ reasonCode: 'not_found' }, 404)
    : c.env.ASSETS.fetch(c.req.raw)
);

app.onError((error, c) => {
  if (error instanceof ApiError) return errorResponse(c, error);
  console.error('Unhandled request failure', { path: c.req.path, error: String(error) });
  return c.json({ reasonCode: 'internal_error' }, 500);
});

export default {
  fetch: app.fetch,
  async scheduled(_controller, env) {
    await deleteExpiredRecords(createDatabase(env.DB));
  }
} satisfies ExportedHandler<Env>;
