import { and, asc, eq, exists, isNull, lte, not, sql, type SQL } from 'drizzle-orm';
import { Hono, type Context } from 'hono';
import { z } from 'zod';
import type { AppEnv, Database } from '../env';
import { auditor } from '../audit';
import { requireSession } from '../auth/session';
import { insertWhere } from '../db/client';
import { invitationRoles, invitations, memberships, organizations, seats, verifiedEmails } from '../db/schema';
import { fail } from '../http/errors';
import { validate } from '../http/validation';
import { isUuid, randomToken, sha256Hex, uuidv7 } from '../lib/ids';
import { dayMs, isoAfter, isoNow } from '../lib/time';
import { organizationAccess, requireRole, type OrganizationAccess } from './access';
import { liveInvitation, seatCapacityGuard, unanswered } from './capacity';
import { sendInvitationEmail, type InvitationEmail } from './invitation-email';

const invitationLifetimeMs = 7 * dayMs;

/** Organization-scoped invitation management, mounted under `/api/organizations/:id`. */
export const invitationRoutes = new Hono<AppEnv>();

invitationRoutes.get('/', async (c) => {
  const access = await manager(c);
  const rows = await c.var.db
    .select({
      invitationId: invitations.id,
      createdByUserId: invitations.createdByUserId,
      email: invitations.email,
      role: invitations.role,
      assignProductSeat: invitations.assignProductSeat,
      createdAt: invitations.createdAt,
      lastSentAt: invitations.lastSentAt,
      expiresAt: invitations.expiresAt,
      emailDelivered: isNull(invitations.lastDeliveryError).mapWith(Boolean)
    })
    .from(invitations)
    .where(and(eq(invitations.organizationId, access.organizationId), unanswered))
    .orderBy(asc(invitations.createdAt), asc(invitations.id));
  return c.json({ reasonCode: 'invitations_listed', organizationId: access.organizationId, invitations: rows });
});

invitationRoutes.post(
  '/',
  validate(
    'json',
    z.object({
      email: z.email().trim().max(320),
      role: z.enum(invitationRoles),
      assignProductSeat: z.boolean().default(true)
    })
  ),
  async (c) => {
    const session = requireSession(c);
    const db = c.var.db;
    const access = await manager(c);
    const { email, role, assignProductSeat } = c.req.valid('json');
    const now = isoNow();
    const invitationId = uuidv7();
    const secret = randomToken();
    const expiresAt = isoAfter(now, invitationLifetimeMs);
    const audit = auditor(db, { actorUserId: session.userId, occurredAt: now });
    const guard = await eligibilityGuard(db, access, email, assignProductSeat, now, null);

    const [insert] = await db.batch([
      insertWhere(
        db,
        invitations,
        {
          id: invitationId,
          organizationId: access.organizationId,
          createdByUserId: session.userId,
          email,
          role,
          assignProductSeat,
          secretHash: await sha256Hex(secret),
          createdAt: now,
          lastSentAt: now,
          expiresAt
        },
        guard
      ),
      audit.insert('organization_invitation_created', 'invitation', invitationId)
    ]);
    if (insert.meta.changes === 0) await failIneligible(db, access, email, now, null);

    const emailDelivered = await deliver(c, invitationId, {
      kind: 'created',
      to: email,
      organizationName: access.name,
      role,
      secret,
      expiresAt
    });
    return c.json(
      { reasonCode: 'invitation_created', invitationId, correlationId: audit.correlationId, expiresAt, emailDelivered },
      201
    );
  }
);

invitationRoutes.delete('/:invitationId', async (c) => {
  const session = requireSession(c);
  const db = c.var.db;
  const access = await manager(c);
  const invitationId = invitationIdParam(c);
  const now = isoNow();
  const audit = auditor(db, { actorUserId: session.userId, occurredAt: now });

  const [update] = await db.batch([
    db
      .update(invitations)
      .set({ cancelledAt: now })
      .where(and(eq(invitations.id, invitationId), eq(invitations.organizationId, access.organizationId), unanswered)),
    audit.insert('organization_invitation_cancelled', 'invitation', invitationId)
  ]);
  if (update.meta.changes === 0) fail(404, 'invitation_not_found');

  return c.json({
    reasonCode: 'invitation_cancelled',
    invitationId,
    organizationId: access.organizationId,
    correlationId: audit.correlationId,
    cancelledAt: now
  });
});

invitationRoutes.post('/:invitationId/resend', async (c) => {
  const session = requireSession(c);
  const db = c.var.db;
  const access = await manager(c);
  const invitationId = invitationIdParam(c);
  const [invitation] = await db
    .select()
    .from(invitations)
    .where(and(eq(invitations.id, invitationId), eq(invitations.organizationId, access.organizationId), unanswered));
  if (!invitation) fail(404, 'invitation_not_found');

  const now = isoNow();
  const secret = randomToken();
  const expiresAt = isoAfter(now, invitationLifetimeMs);
  const audit = auditor(db, { actorUserId: session.userId, occurredAt: now });
  const guard = await eligibilityGuard(
    db,
    access,
    invitation.email,
    invitation.assignProductSeat,
    now,
    invitationId
  );

  const [update] = await db.batch([
    db
      .update(invitations)
      .set({
        secretHash: await sha256Hex(secret),
        lastSentAt: now,
        expiresAt,
        lastDeliveryError: null
      })
      .where(and(eq(invitations.id, invitationId), unanswered, guard)),
    audit.insert('organization_invitation_resent', 'invitation', invitationId)
  ]);
  if (update.meta.changes === 0) {
    await failIneligible(db, access, invitation.email, now, invitationId);
  }

  const emailDelivered = await deliver(c, invitationId, {
    kind: 'resent',
    to: invitation.email,
    organizationName: access.name,
    role: invitation.role,
    secret,
    expiresAt
  });
  return c.json({
    reasonCode: 'invitation_resent',
    invitationId,
    organizationId: access.organizationId,
    correlationId: audit.correlationId,
    expiresAt,
    emailDelivered
  });
});

/** Invitation acceptance by the invitee, mounted at `/api/invitations`. */
export const invitationAcceptanceRoutes = new Hono<AppEnv>();

invitationAcceptanceRoutes.post(
  '/accept',
  validate('json', z.object({ secret: z.string().min(1).max(256) })),
  async (c) => {
    const session = requireSession(c);
    const db = c.var.db;
    const now = isoNow();
    const [invitation] = await db
      .select()
      .from(invitations)
      .where(
        and(
          eq(invitations.secretHash, await sha256Hex(c.req.valid('json').secret)),
          liveInvitation(now),
          lte(invitations.lastSentAt, now)
        )
      );
    if (!invitation) fail(404, 'invitation_not_found');

    const [ownsEmail] = await db
      .select({ id: verifiedEmails.id })
      .from(verifiedEmails)
      .where(and(eq(verifiedEmails.userId, session.userId), eq(verifiedEmails.email, invitation.email)));
    if (!ownsEmail) fail(403, 'invitation_email_mismatch');

    const billingAccountId = await organizationBillingAccount(db, invitation.organizationId);
    const isMember = exists(
      db
        .select({ id: memberships.id })
        .from(memberships)
        .where(and(eq(memberships.organizationId, invitation.organizationId), eq(memberships.userId, session.userId)))
    );
    const capacityGuard = invitation.assignProductSeat
      ? await seatCapacityGuard(db, billingAccountId, invitation.organizationId, now, { alreadyReserved: true })
      : undefined;
    const accepted = exists(
      db
        .select({ id: invitations.id })
        .from(invitations)
        .where(and(eq(invitations.id, invitation.id), eq(invitations.acceptedByUserId, session.userId)))
    );
    const membershipId = uuidv7();
    const seatId = uuidv7();
    const audit = auditor(db, { actorUserId: session.userId, occurredAt: now });

    const [acceptance] = await db.batch([
      db
        .update(invitations)
        .set({ acceptedAt: now, acceptedByUserId: session.userId })
        .where(and(eq(invitations.id, invitation.id), liveInvitation(now), not(isMember), capacityGuard)),
      insertWhere(
        db,
        memberships,
        { id: membershipId, organizationId: invitation.organizationId, userId: session.userId, role: invitation.role, createdAt: now },
        accepted
      ),
      insertWhere(
        db,
        seats,
        {
          id: seatId,
          billingAccountId,
          userId: session.userId,
          productAccess: invitation.assignProductSeat,
          createdAt: now
        },
        accepted
      ),
      audit.insert('organization_invitation_accepted', 'invitation', invitation.id)
    ]);
    if (acceptance.meta.changes === 0) {
      const [member] = await db.all<{ found: number }>(sql`SELECT ${isMember} AS found`);
      if (member?.found) fail(409, 'already_member');
      fail(409, invitation.assignProductSeat ? 'seat_capacity_reached' : 'invitation_not_found');
    }

    return c.json({
      reasonCode: 'invitation_accepted',
      invitationId: invitation.id,
      organizationId: invitation.organizationId,
      membershipId,
      seatId,
      productSeatAssigned: invitation.assignProductSeat,
      role: invitation.role,
      correlationId: audit.correlationId,
      acceptedAt: now
    });
  }
);

async function manager(c: Context<AppEnv>): Promise<OrganizationAccess> {
  const session = requireSession(c);
  const access = await organizationAccess(c.var.db, c.req.param('organizationId') ?? '', session.userId);
  requireRole(access, 'owner', 'admin');
  return access;
}

async function organizationBillingAccount(db: Database, organizationId: string): Promise<string> {
  const [row] = await db
    .select({ billingAccountId: organizations.billingAccountId })
    .from(organizations)
    .where(eq(organizations.id, organizationId));
  return row?.billingAccountId ?? fail(404, 'invitation_not_found');
}

function invitationIdParam(c: Context<AppEnv>): string {
  const invitationId = c.req.param('invitationId') ?? '';
  return isUuid(invitationId) ? invitationId : fail(404, 'invitation_not_found');
}

function alreadyMember(db: Database, organizationId: string, email: string): SQL {
  return exists(
    db
      .select({ id: memberships.id })
      .from(memberships)
      .innerJoin(verifiedEmails, eq(verifiedEmails.userId, memberships.userId))
      .where(and(eq(memberships.organizationId, organizationId), eq(verifiedEmails.email, email)))
  );
}

function otherLiveInvitation(
  db: Database,
  organizationId: string,
  email: string,
  now: string,
  excludeId: string | null
): SQL {
  return exists(
    db
      .select({ id: invitations.id })
      .from(invitations)
      .where(
        and(eq(invitations.organizationId, organizationId), eq(invitations.email, email), liveInvitation(now, excludeId))
      )
  );
}

/**
 * The write-time condition for a (re)sent invitation: the invitee is not a member, no other
 * live invitation targets them, and a seat-bearing invitation fits in the seat capacity.
 */
async function eligibilityGuard(
  db: Database,
  access: OrganizationAccess,
  email: string,
  assignProductSeat: boolean,
  now: string,
  excludeId: string | null
): Promise<SQL> {
  await failIneligible(db, access, email, now, excludeId, false);
  const capacity = assignProductSeat
    ? await seatCapacityGuard(db, access.billingAccountId, access.organizationId, now, { excludeInvitationId: excludeId })
    : undefined;
  return and(
    not(alreadyMember(db, access.organizationId, email)),
    not(otherLiveInvitation(db, access.organizationId, email, now, excludeId)),
    capacity
  )!;
}

/** Reports why an invitation is ineligible; `afterWrite` covers a lost capacity race. */
async function failIneligible(
  db: Database,
  access: OrganizationAccess,
  email: string,
  now: string,
  excludeId: string | null,
  afterWrite = true
) {
  const [row] = await db.all<{ member: number; pending: number }>(sql`SELECT
    ${alreadyMember(db, access.organizationId, email)} AS member,
    ${otherLiveInvitation(db, access.organizationId, email, now, excludeId)} AS pending`);
  if (row?.member) fail(409, 'already_member');
  if (row?.pending) fail(409, 'invitation_already_pending');
  if (afterWrite) fail(409, 'seat_capacity_reached');
}

async function deliver(
  c: Context<AppEnv>,
  invitationId: string,
  email: InvitationEmail
): Promise<boolean> {
  const failure = await sendInvitationEmail(c.env, invitationId, email);
  if (failure) {
    await c.var.db
      .update(invitations)
      .set({ lastDeliveryError: failure })
      .where(eq(invitations.id, invitationId));
  }
  return failure === null;
}
