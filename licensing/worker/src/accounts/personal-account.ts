import type { Database } from '../env';
import { billingAccounts, seats } from '../db/schema';
import { uuidv7 } from '../lib/ids';

/**
 * Statements creating a new user's personal billing account and product seat. Free use needs no
 * account, so a personal account starts without a trial; trials start with an organization.
 */
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
    })
  ] as const;
}
