import { Hono } from 'hono';
import type { AppEnv } from '../env';
import { requireSession } from '../auth/session';
import { isoNow } from '../lib/time';
import { resolveSeatEntitlements, userSeats } from './queries';

export const entitlementRoutes = new Hono<AppEnv>();

entitlementRoutes.get('/', async (c) => {
  const session = requireSession(c);
  const seats = await userSeats(c.var.db, session.userId);
  return c.json({ entitlements: await resolveSeatEntitlements(c.var.db, seats, isoNow()) });
});
