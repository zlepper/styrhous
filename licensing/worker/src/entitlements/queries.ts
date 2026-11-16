import { and, eq, inArray, isNotNull } from 'drizzle-orm';
import type { Database } from '../env';
import { memberships, organizations, seats, subscriptions, trials } from '../db/schema';
import { fundedSeatIds, isEligible, seatEntitlement, type SeatEntitlement } from './rules';

export type BillingContext = Readonly<{
  subscription: typeof subscriptions.$inferSelect | null;
  trial: typeof trials.$inferSelect | null;
  fundedSeatIds: ReadonlySet<string>;
}>;

/** The seat columns that entitlement resolution needs. */
export const seatColumns = {
  id: seats.id,
  billingAccountId: seats.billingAccountId,
  productAccess: seats.productAccess
};

export type SeatRow = Readonly<{ id: string; billingAccountId: string; productAccess: boolean }>;

/** Loads subscription, trial, and seat-funding state for each billing account. */
export async function loadBillingContexts(
  db: Database,
  billingAccountIds: readonly string[]
): Promise<Map<string, BillingContext>> {
  const ids = [...new Set(billingAccountIds)];
  if (ids.length === 0) return new Map();

  const [subscriptionRows, trialRows, candidateRows] = await db.batch([
    db.select().from(subscriptions).where(inArray(subscriptions.billingAccountId, ids)),
    db.select().from(trials).where(inArray(trials.billingAccountId, ids)),
    db
      .select({
        seatId: seats.id,
        billingAccountId: seats.billingAccountId,
        createdAt: seats.createdAt,
        isOwner: isNotNull(memberships.id).mapWith(Boolean)
      })
      .from(seats)
      .leftJoin(organizations, eq(organizations.billingAccountId, seats.billingAccountId))
      .leftJoin(
        memberships,
        and(
          eq(memberships.organizationId, organizations.id),
          eq(memberships.userId, seats.userId),
          eq(memberships.role, 'owner')
        )
      )
      .where(and(inArray(seats.billingAccountId, ids), eq(seats.productAccess, true)))
  ]);

  return new Map(
    ids.map((id) => {
      const subscription = subscriptionRows.find((row) => row.billingAccountId === id) ?? null;
      const candidates = candidateRows.filter((row) => row.billingAccountId === id);
      return [
        id,
        {
          subscription,
          trial: trialRows.find((row) => row.billingAccountId === id) ?? null,
          fundedSeatIds: subscription
            ? fundedSeatIds(candidates, subscription.seatQuantity)
            : new Set<string>()
        }
      ];
    })
  );
}

/** The entitlement a seat grants under its billing account's current state. */
export function entitlementFor(seat: SeatRow, context: BillingContext, now: string): SeatEntitlement {
  const entitlement = seatEntitlement(
    {
      productAccess: seat.productAccess,
      subscription: context.subscription,
      trial: context.trial,
      funded: context.fundedSeatIds.has(seat.id)
    },
    now
  );
  return {
    seatId: seat.id,
    billingAccountId: seat.billingAccountId,
    ...entitlement,
    isEligible: isEligible(entitlement)
  };
}

export async function resolveSeatEntitlements(
  db: Database,
  seatRows: readonly SeatRow[],
  now: string
): Promise<SeatEntitlement[]> {
  const contexts = await loadBillingContexts(
    db,
    seatRows.map((seat) => seat.billingAccountId)
  );
  return seatRows.map((seat) => entitlementFor(seat, contexts.get(seat.billingAccountId)!, now));
}

/** The entitlement of one existing seat. */
export async function seatEntitlementFor(db: Database, seatId: string, now: string): Promise<SeatEntitlement> {
  const [seat] = await db.select(seatColumns).from(seats).where(eq(seats.id, seatId));
  if (!seat) throw new Error(`Seat ${seatId} does not exist.`);
  const [entitlement] = await resolveSeatEntitlements(db, [seat], now);
  return entitlement!;
}

export async function userSeats(db: Database, userId: string): Promise<SeatRow[]> {
  return db
    .select(seatColumns)
    .from(seats)
    .where(eq(seats.userId, userId))
    .orderBy(seats.createdAt, seats.id);
}
