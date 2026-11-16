import { and, isNull, lt } from 'drizzle-orm';
import type { Database } from './env';
import { deviceAuthorizations, invitations, oauthStates, sessions } from './db/schema';
import { dayMs, isoAfter, isoNow } from './lib/time';

/** Daily removal of expired, short-lived records. */
export async function deleteExpiredRecords(db: Database, now = isoNow()) {
  const invitationRetentionCutoff = isoAfter(now, -30 * dayMs);
  await db.batch([
    db.delete(sessions).where(lt(sessions.expiresAt, now)),
    db.delete(oauthStates).where(lt(oauthStates.expiresAt, now)),
    db
      .delete(deviceAuthorizations)
      .where(lt(deviceAuthorizations.expiresAt, now)),
    db
      .delete(invitations)
      .where(
        and(
          isNull(invitations.acceptedAt),
          lt(invitations.expiresAt, invitationRetentionCutoff)
        )
      )
  ]);
}
