import type { Database } from '../env';
import { billingAccounts, seats, trials } from '../db/schema';
import { uuidv7 } from '../lib/ids';
import { dayMs, isoAfter } from '../lib/time';

const trialDurationMs = 30 * dayMs;

/** Statements creating a new user's personal billing account, product seat, and trial. */
export function personalAccountInserts(db: Database, userId: string, now: string) {
  const billingAccountId = uuidv7();
  return [
    db.insert(billingAccounts).values({
      id: billingAccountId,
      kind: 'personal',
      personalOwnerUserId: userId,
      createdAt: now
    }),
    db.insert(seats).values({
      id: uuidv7(),
      billingAccountId,
      userId,
      productAccess: true,
      createdAt: now
    }),
    db.insert(trials).values({
      id: uuidv7(),
      originatingUserId: userId,
      billingAccountId,
      startedAt: now,
      endsAt: isoAfter(now, trialDurationMs)
    })
  ] as const;
}
