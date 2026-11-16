import { and, eq, isNull } from 'drizzle-orm';
import { Hono } from 'hono';
import type { AppEnv } from '../env';
import { auditor } from '../audit';
import { requireSession } from '../auth/session';
import { deviceActivations, seats } from '../db/schema';
import { fail } from '../http/errors';
import { isUuid } from '../lib/ids';
import { isoNow } from '../lib/time';
import { activeDevices, revocation } from './activations';

export const deviceRoutes = new Hono<AppEnv>();

deviceRoutes.get('/seats/:seatId/devices', async (c) => {
  const session = requireSession(c);
  const seatId = c.req.param('seatId');
  if (!isUuid(seatId)) fail(404, 'seat_not_found');
  const [seat] = await c.var.db
    .select({ id: seats.id, deviceLimit: seats.deviceLimit })
    .from(seats)
    .where(and(eq(seats.id, seatId), eq(seats.userId, session.userId)));
  if (!seat) fail(404, 'seat_not_found');
  return c.json({
    reasonCode: 'devices_listed',
    seatId: seat.id,
    deviceLimit: seat.deviceLimit,
    activeDevices: await activeDevices(c.var.db, seat.id)
  });
});

deviceRoutes.delete('/device-activations/:activationId', async (c) => {
  const session = requireSession(c);
  const db = c.var.db;
  const activationId = c.req.param('activationId');
  if (!isUuid(activationId)) fail(404, 'device_not_active');
  const now = isoNow();
  const audit = auditor(db, { actorUserId: session.userId, occurredAt: now });
  const [revoked] = await db.batch([
    db
      .update(deviceActivations)
      .set(revocation('revoked_by_user', now))
      .where(
        and(
          eq(deviceActivations.id, activationId),
          eq(deviceActivations.userId, session.userId),
          isNull(deviceActivations.revokedAt)
        )
      ),
    audit.insert('device_revoked', 'device_activation', activationId)
  ]);
  if (revoked.meta.changes === 0) fail(404, 'device_not_active');
  return c.json({ reasonCode: 'device_revoked', activationId, correlationId: audit.correlationId });
});
