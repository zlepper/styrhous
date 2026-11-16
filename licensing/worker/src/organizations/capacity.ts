import { and, eq, gt, isNull, ne, sql, type SQL } from 'drizzle-orm';
import type { Database } from '../env';
import { invitations, seats } from '../db/schema';
import { loadBillingContexts } from '../entitlements/queries';
import { organizationSeatCapacity } from '../entitlements/rules';
import { fail } from '../http/errors';

/** An invitation that has been neither accepted nor cancelled. */
export const unanswered = and(isNull(invitations.acceptedAt), isNull(invitations.cancelledAt))!;

/** An unanswered invitation that can still be accepted, optionally excluding one. */
export function liveInvitation(now: string, excludeId: string | null = null): SQL {
  return and(unanswered, gt(invitations.expiresAt, now), excludeId ? ne(invitations.id, excludeId) : undefined)!;
}

/**
 * Product seats in use plus seats reserved by live seat-bearing invitations, evaluated inside
 * the writing statement so concurrent requests cannot both claim the last seat.
 */
export function seatsNeeded(
  billingAccountId: string,
  organizationId: string,
  now: string,
  excludeInvitationId: string | null = null
): SQL {
  const assigned = and(eq(seats.billingAccountId, billingAccountId), eq(seats.productAccess, true));
  const reserved = and(
    eq(invitations.organizationId, organizationId),
    eq(invitations.assignProductSeat, true),
    liveInvitation(now, excludeInvitationId)
  );
  return sql`((SELECT count(*) FROM ${seats} WHERE ${assigned})
    + (SELECT count(*) FROM ${invitations} WHERE ${reserved}))`;
}

type CapacityOptions = Readonly<{
  /** An invitation being resent, which must not count against itself. */
  excludeInvitationId?: string | null;
  /** The seat is already counted by its invitation, so filling it is seat-neutral. */
  alreadyReserved?: boolean;
}>;

/**
 * The write-time condition for taking one more product seat. Fails right away when the
 * organization has no seat capacity at all.
 */
export async function seatCapacityGuard(
  db: Database,
  billingAccountId: string,
  organizationId: string,
  now: string,
  { excludeInvitationId = null, alreadyReserved = false }: CapacityOptions = {}
): Promise<SQL> {
  const context = (await loadBillingContexts(db, [billingAccountId])).get(billingAccountId)!;
  const capacity =
    organizationSeatCapacity(context.subscription, context.trial, now) ?? fail(409, 'no_active_seat_capacity');
  const needed = seatsNeeded(billingAccountId, organizationId, now, excludeInvitationId);
  return alreadyReserved ? sql`${needed} <= ${capacity}` : sql`${needed} < ${capacity}`;
}
