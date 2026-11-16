import { and, count, eq, exists, gt, isNull, lt } from 'drizzle-orm';
import { Hono, type Context } from 'hono';
import { z } from 'zod';
import type { AppEnv, Database } from '../env';
import { auditor } from '../audit';
import { requireSession } from '../auth/session';
import { deviceActivations, deviceAuthorizations, organizations, seats } from '../db/schema';
import { insertWhere, runBatch } from '../db/client';
import { isUniqueViolation } from '../db/errors';
import { resolveSeatEntitlements, seatColumns } from '../entitlements/queries';
import { fail } from '../http/errors';
import { validate } from '../http/validation';
import { uuidv7 } from '../lib/ids';
import { isoNow } from '../lib/time';
import {
  activeDevicesBySeat,
  installationOf,
  planActivation,
  replaceableActivation,
  revocation
} from '../devices/activations';
import { normalizeUserCode } from './codes';

/** Browser approval of a pending desktop sign-in, mounted at `/api/device-authorizations`. */
export const deviceApprovalRoutes = new Hono<AppEnv>();

/** A device authorization that is still waiting for the user's decision. */
function awaitingDecision(now: string) {
  return and(eq(deviceAuthorizations.status, 'pending'), gt(deviceAuthorizations.expiresAt, now))!;
}

async function pendingAuthorization(c: Context<AppEnv>, now: string) {
  const [authorization] = await c.var.db
    .select()
    .from(deviceAuthorizations)
    .where(
      and(eq(deviceAuthorizations.userCode, normalizeUserCode(c.req.param('userCode') ?? '')), awaitingDecision(now))
    );
  return authorization ?? fail(404, 'device_authorization_not_found');
}

/** The caller's seats that currently grant product use, with their device state. */
async function eligibleSeats(db: Database, userId: string, installationId: string, now: string) {
  const rows = await db
    .select({ ...seatColumns, deviceLimit: seats.deviceLimit, organizationName: organizations.name })
    .from(seats)
    .leftJoin(organizations, eq(organizations.billingAccountId, seats.billingAccountId))
    .where(eq(seats.userId, userId))
    .orderBy(seats.createdAt, seats.id);
  const entitlements = await resolveSeatEntitlements(db, rows, now);
  const eligible = rows
    .map((row, index) => ({ row, entitlement: entitlements[index]! }))
    .filter(({ entitlement }) => entitlement.isEligible);
  const devices = await activeDevicesBySeat(
    db,
    eligible.map(({ row }) => row.id)
  );
  return eligible.map(({ row, entitlement }) => {
    const activeDevices = devices.get(row.id)!;
    return {
      seatId: row.id,
      billingAccountId: row.billingAccountId,
      name: row.organizationName ?? 'Personal',
      entitlementState: entitlement.state,
      entitlementReasonCode: entitlement.reasonCode,
      deviceLimit: row.deviceLimit,
      canActivate: planActivation(activeDevices, installationId, row.deviceLimit, now).kind !== 'limit_reached',
      activeDevices
    };
  });
}

deviceApprovalRoutes.get('/:userCode', async (c) => {
  const session = requireSession(c);
  const now = isoNow();
  const authorization = await pendingAuthorization(c, now);
  const seatsForApproval = await eligibleSeats(c.var.db, session.userId, authorization.installationId, now);
  const alreadyActive = seatsForApproval.find((seat) =>
    seat.activeDevices.some((device) => device.installationId === authorization.installationId)
  );
  return c.json({
    reasonCode: 'device_authorization_awaiting_approval',
    installation: installationOf(authorization),
    eligibleSeats: seatsForApproval,
    selectedSeatId: (alreadyActive ?? seatsForApproval.find((seat) => seat.canActivate))?.seatId ?? null
  });
});

deviceApprovalRoutes.post(
  '/:userCode',
  validate(
    'json',
    z.discriminatedUnion('decision', [
      z.object({ decision: z.literal('deny') }),
      z.object({ decision: z.literal('approve'), seatId: z.uuid().nullish() })
    ])
  ),
  async (c) => {
    const session = requireSession(c);
    const db = c.var.db;
    const now = isoNow();
    const authorization = await pendingAuthorization(c, now);
    const body = c.req.valid('json');
    const stillPending = and(eq(deviceAuthorizations.id, authorization.id), awaitingDecision(now));

    if (body.decision === 'deny') {
      const result = await db.update(deviceAuthorizations).set({ status: 'denied' }).where(stillPending);
      if (result.meta.changes === 0) fail(404, 'device_authorization_not_found');
      return c.body(null, 204);
    }

    if (!body.seatId) fail(409, 'device_authorization_seat_required');
    const seat = (await eligibleSeats(db, session.userId, authorization.installationId, now)).find(
      (candidate) => candidate.seatId === body.seatId
    );
    if (!seat) fail(409, 'device_authorization_seat_not_eligible');

    const plan = planActivation(seat.activeDevices, authorization.installationId, seat.deviceLimit, now);
    if (plan.kind === 'limit_reached') {
      fail(409, 'device_limit_reached', { activeDevices: seat.activeDevices });
    }

    const activationId = plan.kind === 'reuse' ? plan.activationId : uuidv7();
    const pendingGuard = exists(
      db.select({ id: deviceAuthorizations.id }).from(deviceAuthorizations).where(stillPending)
    );
    const installation = installationOf(authorization);
    const audit = auditor(db, { actorUserId: session.userId, occurredAt: now });
    const activeOnSeat = db
      .select({ total: count() })
      .from(deviceActivations)
      .where(and(eq(deviceActivations.seatId, seat.seatId), isNull(deviceActivations.revokedAt)));

    const activationSteps =
      plan.kind === 'reuse'
        ? [
            db
              .update(deviceActivations)
              .set({ ...installation, lastSeenAt: now })
              .where(and(eq(deviceActivations.id, activationId), isNull(deviceActivations.revokedAt), pendingGuard))
          ]
        : [
            ...(plan.kind === 'replace'
              ? [
                  db
                    .update(deviceActivations)
                    .set(revocation('replaced_stale', now))
                    .where(
                      and(
                        eq(deviceActivations.id, plan.staleActivationId),
                        isNull(deviceActivations.revokedAt),
                        replaceableActivation(now),
                        pendingGuard
                      )
                    ),
                  audit.insert('device_revoked_stale', 'device_activation', plan.staleActivationId)
                ]
              : []),
            insertWhere(
              db,
              deviceActivations,
              { id: activationId, seatId: seat.seatId, userId: session.userId, ...installation, activatedAt: now, lastSeenAt: now },
              and(pendingGuard, lt(activeOnSeat, seat.deviceLimit))!
            ),
            audit.insert('device_activated', 'device_activation', activationId)
          ];

    try {
      const results = await runBatch(db, [
        ...activationSteps,
        db
          .update(deviceAuthorizations)
          .set({ status: 'approved', activationId })
          .where(
            and(
              stillPending,
              exists(
                db
                  .select({ id: deviceActivations.id })
                  .from(deviceActivations)
                  .where(and(eq(deviceActivations.id, activationId), isNull(deviceActivations.revokedAt)))
              )
            )
          )
      ]);
      if (results.at(-1)!.meta.changes === 0) fail(409, 'device_authorization_concurrent_modification');
    } catch (error) {
      if (isUniqueViolation(error)) fail(409, 'device_authorization_concurrent_modification');
      throw error;
    }
    return c.body(null, 204);
  }
);
