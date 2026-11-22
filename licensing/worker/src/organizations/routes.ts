import { and, asc, eq, exists, inArray, ne, notExists } from 'drizzle-orm';
import { Hono } from 'hono';
import { z } from 'zod';
import type { AppEnv } from '../env';
import { auditor } from '../audit';
import { requireSession } from '../auth/session';
import { runBatch } from '../db/client';
import {
  billingAccounts,
  deviceActivations,
  deviceAuthorizations,
  invitationRoles,
  memberships,
  organizations,
  seats,
  trials,
  users
} from '../db/schema';
import { fail } from '../http/errors';
import { validate } from '../http/validation';
import { uuidv7 } from '../lib/ids';
import { isoAfter, isoNow } from '../lib/time';
import { trialDurationMs } from '../entitlements/rules';
import { organizationAccess, organizationMember, requireRole } from './access';
import { seatCapacityGuard } from './capacity';
import { invitationRoutes } from './invitations';

export const organizationRoutes = new Hono<AppEnv>();

organizationRoutes.route('/:organizationId/invitations', invitationRoutes);

organizationRoutes.get('/', async (c) => {
  const session = requireSession(c);
  const rows = await c.var.db
    .select({
      organizationId: organizations.id,
      billingAccountId: organizations.billingAccountId,
      membershipId: memberships.id,
      seatId: seats.id,
      name: organizations.name,
      role: memberships.role,
      productSeatAssigned: seats.productAccess,
      deviceLimit: seats.deviceLimit,
      joinedAt: memberships.createdAt
    })
    .from(memberships)
    .innerJoin(organizations, eq(organizations.id, memberships.organizationId))
    .innerJoin(
      seats,
      and(eq(seats.billingAccountId, organizations.billingAccountId), eq(seats.userId, memberships.userId))
    )
    .where(eq(memberships.userId, session.userId))
    .orderBy(asc(organizations.name), asc(organizations.id));
  return c.json({ reasonCode: 'organizations_listed', organizations: rows });
});

organizationRoutes.post(
  '/',
  validate('json', z.object({ name: z.string().trim().min(1).max(120) })),
  async (c) => {
    const session = requireSession(c);
    const db = c.var.db;
    const { name } = c.req.valid('json');
    const now = isoNow();
    const audit = auditor(db, { actorUserId: session.userId, occurredAt: now });
    const ids = {
      organizationId: uuidv7(),
      billingAccountId: uuidv7(),
      ownerMembershipId: uuidv7(),
      seatId: uuidv7()
    };
    // Each user starts one trial, with the first organization they create.
    const trialStart = db
      .insert(trials)
      .values({
        id: uuidv7(),
        originatingUserId: session.userId,
        billingAccountId: ids.billingAccountId,
        startedAt: now,
        endsAt: isoAfter(now, trialDurationMs)
      })
      .onConflictDoNothing({ target: trials.originatingUserId });

    const statements = [
      db.insert(billingAccounts).values({ id: ids.billingAccountId, kind: 'organization', createdAt: now }),
      db.insert(organizations).values({
        id: ids.organizationId,
        billingAccountId: ids.billingAccountId,
        createdByUserId: session.userId,
        name,
        createdAt: now
      }),
      audit.insert('organization_created', 'organization', ids.organizationId),
      db.insert(memberships).values({
        id: ids.ownerMembershipId,
        organizationId: ids.organizationId,
        userId: session.userId,
        role: 'owner',
        createdAt: now
      }),
      audit.insert('organization_owner_assigned', 'membership', ids.ownerMembershipId),
      db.insert(seats).values({
        id: ids.seatId,
        billingAccountId: ids.billingAccountId,
        userId: session.userId,
        productAccess: true,
        createdAt: now
      }),
      audit.insert('seat_assigned', 'seat', ids.seatId),
      trialStart,
      audit.insert('trial_started', 'organization', ids.organizationId)
    ];
    const results = await runBatch(db, statements);
    const trialStarted = results[statements.indexOf(trialStart)]!.meta.changes > 0;

    return c.json(
      { userId: session.userId, ...ids, correlationId: audit.correlationId, trialStarted },
      201
    );
  }
);

organizationRoutes.get('/:organizationId/members', async (c) => {
  const session = requireSession(c);
  const access = await organizationAccess(c.var.db, c.req.param('organizationId'), session.userId);
  const members = await c.var.db
    .select({
      membershipId: memberships.id,
      userId: memberships.userId,
      seatId: seats.id,
      email: users.email,
      role: memberships.role,
      productSeatAssigned: seats.productAccess,
      deviceLimit: seats.deviceLimit,
      joinedAt: memberships.createdAt
    })
    .from(memberships)
    .innerJoin(users, eq(users.id, memberships.userId))
    .innerJoin(
      seats,
      and(eq(seats.userId, memberships.userId), eq(seats.billingAccountId, access.billingAccountId))
    )
    .where(eq(memberships.organizationId, access.organizationId))
    .orderBy(asc(memberships.createdAt), asc(memberships.id));
  return c.json({ reasonCode: 'organization_members_listed', organizationId: access.organizationId, members });
});

organizationRoutes.delete('/:organizationId/members/:membershipId', async (c) => {
  const session = requireSession(c);
  const db = c.var.db;
  const access = await organizationAccess(db, c.req.param('organizationId'), session.userId);
  const target = await organizationMember(db, access, c.req.param('membershipId'));

  if (target.role === 'owner') fail(409, 'ownership_transfer_required');
  const isSelf = target.membershipId === access.membershipId;
  const mayRemove =
    isSelf || access.role === 'owner' || (access.role === 'admin' && target.role === 'member');
  if (!mayRemove) fail(403, 'insufficient_permission');

  const now = isoNow();
  const audit = auditor(db, { actorUserId: session.userId, occurredAt: now });
  const membershipGone = notExists(
    db.select({ id: memberships.id }).from(memberships).where(eq(memberships.id, target.membershipId))
  );
  const activationIds = db
    .select({ id: deviceActivations.id })
    .from(deviceActivations)
    .where(eq(deviceActivations.seatId, target.seatId));
  const [removal] = await db.batch([
    db
      .delete(memberships)
      .where(and(eq(memberships.id, target.membershipId), eq(memberships.role, target.role))),
    db
      .update(deviceAuthorizations)
      .set({ activationId: null })
      .where(and(inArray(deviceAuthorizations.activationId, activationIds), membershipGone)),
    db.delete(deviceActivations).where(and(eq(deviceActivations.seatId, target.seatId), membershipGone)),
    db.delete(seats).where(and(eq(seats.id, target.seatId), membershipGone)),
    audit.insert('organization_member_removed', 'membership', target.membershipId)
  ]);
  if (removal.meta.changes === 0) fail(409, 'organization_members_changed');

  return c.json({
    reasonCode: 'organization_member_removed',
    organizationId: access.organizationId,
    membershipId: target.membershipId,
    userId: target.userId,
    correlationId: audit.correlationId,
    removedAt: now
  });
});

organizationRoutes.patch(
  '/:organizationId/members/:membershipId/role',
  validate('json', z.object({ role: z.enum(invitationRoles) })),
  async (c) => {
    const session = requireSession(c);
    const db = c.var.db;
    const access = await organizationAccess(db, c.req.param('organizationId'), session.userId);
    const target = await organizationMember(db, access, c.req.param('membershipId'));
    const { role } = c.req.valid('json');

    requireRole(access, 'owner');
    if (target.role === 'owner') fail(409, 'ownership_transfer_required');
    if (target.role === role) fail(409, 'organization_member_role_unchanged');

    const now = isoNow();
    const audit = auditor(db, { actorUserId: session.userId, occurredAt: now });
    const [update] = await db.batch([
      db
        .update(memberships)
        .set({ role })
        .where(and(eq(memberships.id, target.membershipId), eq(memberships.role, target.role))),
      audit.insert('organization_member_role_changed', 'membership', target.membershipId)
    ]);
    if (update.meta.changes === 0) fail(409, 'organization_members_changed');

    return c.json({
      reasonCode: 'organization_member_role_changed',
      organizationId: access.organizationId,
      membershipId: target.membershipId,
      userId: target.userId,
      previousRole: target.role,
      role,
      correlationId: audit.correlationId,
      changedAt: now
    });
  }
);

organizationRoutes.patch(
  '/:organizationId/members/:membershipId/seat',
  validate('json', z.object({ assigned: z.boolean() })),
  async (c) => {
    const session = requireSession(c);
    const db = c.var.db;
    const access = await organizationAccess(db, c.req.param('organizationId'), session.userId);
    const target = await organizationMember(db, access, c.req.param('membershipId'));
    const { assigned } = c.req.valid('json');

    requireRole(access, 'owner', 'admin');
    if (target.productAccess === assigned) fail(409, 'organization_member_seat_unchanged');

    const now = isoNow();
    const audit = auditor(db, { actorUserId: session.userId, occurredAt: now });
    const capacityGuard = assigned
      ? await seatCapacityGuard(db, access.billingAccountId, access.organizationId, now)
      : undefined;
    const [update] = await db.batch([
      db
        .update(seats)
        .set({ productAccess: assigned })
        .where(and(eq(seats.id, target.seatId), eq(seats.productAccess, !assigned), capacityGuard)),
      audit.insert(assigned ? 'seat_assigned' : 'seat_unassigned', 'seat', target.seatId)
    ]);
    if (update.meta.changes === 0) {
      fail(409, assigned ? 'seat_capacity_reached' : 'organization_members_changed');
    }

    return c.json({
      reasonCode: assigned ? 'organization_member_seat_assigned' : 'organization_member_seat_unassigned',
      organizationId: access.organizationId,
      membershipId: target.membershipId,
      userId: target.userId,
      seatId: target.seatId,
      assigned,
      deviceLimit: target.deviceLimit,
      correlationId: audit.correlationId,
      changedAt: now
    });
  }
);

organizationRoutes.post('/:organizationId/members/:membershipId/transfer-ownership', async (c) => {
  const session = requireSession(c);
  const db = c.var.db;
  const access = await organizationAccess(db, c.req.param('organizationId'), session.userId);
  const target = await organizationMember(db, access, c.req.param('membershipId'));

  requireRole(access, 'owner');
  if (target.membershipId === access.membershipId || target.role === 'owner') {
    fail(409, 'ownership_transfer_target_invalid');
  }

  const now = isoNow();
  const audit = auditor(db, { actorUserId: session.userId, occurredAt: now });
  // The batch is one transaction: the demotion only applies while the target is still a
  // non-owner member, so the promotion that follows always succeeds with it.
  const [demotion] = await db.batch([
    db
      .update(memberships)
      .set({ role: 'admin' })
      .where(
        and(
          eq(memberships.id, access.membershipId),
          eq(memberships.role, 'owner'),
          exists(
            db
              .select({ id: memberships.id })
              .from(memberships)
              .where(and(eq(memberships.id, target.membershipId), ne(memberships.role, 'owner')))
          )
        )
      ),
    db
      .update(memberships)
      .set({ role: 'owner' })
      .where(
        and(
          eq(memberships.id, target.membershipId),
          notExists(
            db
              .select({ id: memberships.id })
              .from(memberships)
              .where(and(eq(memberships.organizationId, access.organizationId), eq(memberships.role, 'owner')))
          )
        )
      ),
    audit.insert('organization_owner_assigned', 'membership', target.membershipId)
  ]);
  if (demotion.meta.changes === 0) fail(409, 'organization_members_changed');

  return c.json({
    reasonCode: 'organization_ownership_transferred',
    organizationId: access.organizationId,
    previousOwnerMembershipId: access.membershipId,
    previousOwnerUserId: session.userId,
    ownerMembershipId: target.membershipId,
    ownerUserId: target.userId,
    correlationId: audit.correlationId,
    transferredAt: now
  });
});
