import { and, eq, isNull, sql } from 'drizzle-orm';
import { Hono, type Context } from 'hono';
import { z } from 'zod';
import type { AppEnv } from '../env';
import { deviceActivations, deviceAuthorizations } from '../db/schema';
import { isUniqueViolation } from '../db/errors';
import { desktopSignInWindowMs, revocation } from '../devices/activations';
import { seatEntitlementFor } from '../entitlements/queries';
import { randomToken, sha256Hex, uuidv7 } from '../lib/ids';
import { isoAfter, isoNow } from '../lib/time';
import { generateUserCode } from './codes';
import { leaseKeys, signLease } from './lease';

const authorizationLifetimeMs = desktopSignInWindowMs;
const pollIntervalSeconds = 5;
// Polls a little faster than the interval are tolerated to absorb network jitter.
const minimumPollGapMs = (pollIntervalSeconds - 1) * 1000;

/** The desktop app's protocol; see `licensing/desktop-protocol.md`. */
export const desktopRoutes = new Hono<AppEnv>();

function oauthError(c: Context<AppEnv>, error: string, status: 400 | 401 = 400) {
  return c.json({ error }, status);
}

const installationSchema = z.object({
  installation_id: z.uuid(),
  display_name: z.string().trim().min(1).max(120),
  platform: z.string().trim().min(1).max(64),
  architecture: z.string().trim().min(1).max(32),
  styrhous_version: z.string().trim().min(1).max(64)
});

desktopRoutes.post('/device/authorize', async (c) => {
  const parsed = installationSchema.safeParse(Object.fromEntries(await formFields(c)));
  if (!parsed.success) return oauthError(c, 'invalid_request');
  const installation = parsed.data;
  const deviceCode = randomToken();
  const deviceCodeHash = await sha256Hex(deviceCode);
  const now = isoNow();

  const insert = (userCode: string) =>
    c.var.db.insert(deviceAuthorizations).values({
      id: uuidv7(),
      deviceCodeHash,
      userCode,
      installationId: installation.installation_id.toLowerCase(),
      displayName: installation.display_name,
      platform: installation.platform,
      architecture: installation.architecture,
      styrhousVersion: installation.styrhous_version,
      status: 'pending',
      createdAt: now,
      expiresAt: isoAfter(now, authorizationLifetimeMs)
    });
  let userCode = generateUserCode();
  try {
    await insert(userCode);
  } catch (error) {
    // User codes are short; draw another one if this one is still in use.
    if (!isUniqueViolation(error)) throw error;
    userCode = generateUserCode();
    await insert(userCode);
  }

  const verificationUri = new URL('/devices/authorize', c.env.PUBLIC_ORIGIN);
  const verificationUriComplete = new URL(verificationUri);
  verificationUriComplete.searchParams.set('user_code', userCode);
  return c.json({
    device_code: deviceCode,
    user_code: userCode,
    verification_uri: verificationUri.toString(),
    verification_uri_complete: verificationUriComplete.toString(),
    expires_in: authorizationLifetimeMs / 1000,
    interval: pollIntervalSeconds
  });
});

desktopRoutes.post('/token', async (c) => {
  const deviceCode = (await formFields(c)).get('device_code');
  if (!deviceCode) return oauthError(c, 'invalid_request');
  const db = c.var.db;
  const [authorization] = await db
    .select()
    .from(deviceAuthorizations)
    .where(eq(deviceAuthorizations.deviceCodeHash, await sha256Hex(deviceCode)));
  if (!authorization || authorization.status === 'consumed') return oauthError(c, 'invalid_grant');
  if (authorization.status === 'denied') return oauthError(c, 'access_denied');

  const now = isoNow();
  if (authorization.expiresAt <= now) return oauthError(c, 'expired_token');

  if (authorization.status === 'pending') {
    const tooFast =
      authorization.lastPolledAt !== null &&
      Date.now() - new Date(authorization.lastPolledAt).getTime() < minimumPollGapMs;
    await db
      .update(deviceAuthorizations)
      .set({ lastPolledAt: now })
      .where(eq(deviceAuthorizations.id, authorization.id));
    return oauthError(c, tooFast ? 'slow_down' : 'authorization_pending');
  }

  const token = randomToken();
  const [consumed, issued] = await db.batch([
    db
      .update(deviceAuthorizations)
      .set({ status: 'consumed' })
      .where(and(eq(deviceAuthorizations.id, authorization.id), eq(deviceAuthorizations.status, 'approved'))),
    db
      .update(deviceActivations)
      .set({ tokenHash: await sha256Hex(token), lastSeenAt: now })
      .where(
        and(
          eq(deviceActivations.id, authorization.activationId!),
          isNull(deviceActivations.revokedAt),
          sql`changes() > 0`
        )
      )
  ]);
  if (consumed.meta.changes === 0) return oauthError(c, 'invalid_grant');
  if (issued.meta.changes === 0) return oauthError(c, 'access_denied');
  return c.json({ installation_token: token });
});

/** Resolves the bearer installation token to its active device activation. */
async function authenticatedActivation(c: Context<AppEnv>) {
  const header = c.req.header('Authorization') ?? '';
  const token = header.startsWith('Bearer ') ? header.slice('Bearer '.length).trim() : '';
  if (!token) return null;
  const [activation] = await c.var.db
    .select()
    .from(deviceActivations)
    .where(and(eq(deviceActivations.tokenHash, await sha256Hex(token)), isNull(deviceActivations.revokedAt)));
  return activation ?? null;
}

desktopRoutes.post('/entitlement', async (c) => {
  const activation = await authenticatedActivation(c);
  if (!activation) {
    c.header('WWW-Authenticate', 'Bearer');
    return c.json({ reasonCode: 'invalid_installation_token' }, 401);
  }

  const db = c.var.db;
  const now = isoNow();
  const entitlement = await seatEntitlementFor(db, activation.seatId, now);
  await db.update(deviceActivations).set({ lastSeenAt: now }).where(eq(deviceActivations.id, activation.id));
  // The installation stays signed in without entitlement; the app shows evaluation status.
  if (!entitlement.isEligible) return c.json({ reasonCode: entitlement.reasonCode }, 403);

  const signed = await signLease(c.env, {
    userId: activation.userId,
    installationId: activation.installationId,
    activationId: activation.id,
    entitlement
  });
  return c.json({
    reasonCode: 'offline_lease_issued',
    lease: signed.lease,
    state: entitlement.state,
    entitlementReasonCode: entitlement.reasonCode,
    expiresAt: signed.expiresAt,
    refreshAfter: signed.refreshAfter
  });
});

desktopRoutes.post('/sign-out', async (c) => {
  const activation = await authenticatedActivation(c);
  if (activation) {
    await c.var.db
      .update(deviceActivations)
      .set(revocation('signed_out', isoNow()))
      .where(eq(deviceActivations.id, activation.id));
  }
  return c.body(null, 204);
});

desktopRoutes.get('/keys', async (c) => c.json(await leaseKeys(c.env)));

async function formFields(c: Context<AppEnv>): Promise<URLSearchParams> {
  const contentType = c.req.header('Content-Type') ?? '';
  if (!contentType.startsWith('application/x-www-form-urlencoded')) return new URLSearchParams();
  const fields = [...(await c.req.formData())].map(([key, value]): [string, string] => [key, String(value)]);
  return new URLSearchParams(fields);
}
