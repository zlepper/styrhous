import { and, count, eq, sql } from 'drizzle-orm';
import type { Database } from '../env';
import { externalIdentities, users, verifiedEmails } from '../db/schema';
import { isUniqueViolation } from '../db/errors';
import { personalAccountInserts } from '../accounts/personal-account';
import { fail } from '../http/errors';
import { uuidv7 } from '../lib/ids';
import { isoNow } from '../lib/time';
import { AuthenticationFailure, type ExternalIdentity, type ProviderName } from './providers';

async function findIdentityOwner(db: Database, provider: ProviderName, subject: string) {
  const [row] = await db
    .select({ userId: externalIdentities.userId })
    .from(externalIdentities)
    .where(and(eq(externalIdentities.provider, provider), eq(externalIdentities.subject, subject)));
  return row?.userId ?? null;
}

async function findEmailOwner(db: Database, email: string) {
  const [row] = await db
    .select({ userId: verifiedEmails.userId })
    .from(verifiedEmails)
    .where(eq(verifiedEmails.email, email));
  return row?.userId ?? null;
}

/**
 * Resolves the account for a sign-in, creating a new account (with a personal seat)
 * for a first-time identity. An email that already belongs to another account must be linked
 * explicitly from that account instead of being merged automatically.
 */
export async function signIn(
  db: Database,
  provider: ProviderName,
  identity: ExternalIdentity
): Promise<string> {
  try {
    return await resolveSignIn(db, provider, identity);
  } catch (error) {
    // A concurrent first sign-in for the same identity or email won the race; resolve again.
    if (!isUniqueViolation(error)) throw error;
    return await resolveSignIn(db, provider, identity);
  }
}

async function resolveSignIn(db: Database, provider: ProviderName, identity: ExternalIdentity) {
  const existingUserId = await findIdentityOwner(db, provider, identity.subject);
  if (existingUserId) {
    await claimEmail(db, existingUserId, identity.email);
    return existingUserId;
  }

  if (await findEmailOwner(db, identity.email)) {
    throw new AuthenticationFailure('account_link_required');
  }

  const userId = uuidv7();
  const now = isoNow();
  await db.batch([
    db.insert(users).values({ id: userId, email: identity.email, createdAt: now }),
    db.insert(externalIdentities).values({
      id: uuidv7(),
      userId,
      provider,
      subject: identity.subject,
      createdAt: now
    }),
    db.insert(verifiedEmails).values({
      id: uuidv7(),
      userId,
      email: identity.email,
      createdAt: now
    }),
    ...personalAccountInserts(db, userId, now)
  ]);
  return userId;
}

/** Records a (possibly changed) provider email as verified for its existing owner. */
async function claimEmail(db: Database, userId: string, email: string) {
  await db
    .insert(verifiedEmails)
    .values({ id: uuidv7(), userId, email, createdAt: isoNow() })
    .onConflictDoNothing();
  if ((await findEmailOwner(db, email)) !== userId) {
    throw new AuthenticationFailure('account_link_required');
  }
  await db.update(users).set({ email }).where(eq(users.id, userId));
}

export async function linkIdentity(
  db: Database,
  userId: string,
  provider: ProviderName,
  identity: ExternalIdentity
) {
  const owner = await findIdentityOwner(db, provider, identity.subject);
  if (owner === userId) return;
  if (owner) throw new AuthenticationFailure('provider_already_linked');
  if ((await findEmailOwner(db, identity.email)) !== userId) {
    throw new AuthenticationFailure('verified_email_mismatch');
  }
  try {
    await db.insert(externalIdentities).values({
      id: uuidv7(),
      userId,
      provider,
      subject: identity.subject,
      createdAt: isoNow()
    });
  } catch (error) {
    // Either the account already uses this provider or the identity was linked concurrently.
    if (isUniqueViolation(error)) throw new AuthenticationFailure('provider_already_linked');
    throw error;
  }
}

export async function assertIdentityLinked(
  db: Database,
  userId: string,
  provider: ProviderName,
  identity: ExternalIdentity
) {
  if ((await findIdentityOwner(db, provider, identity.subject)) !== userId) {
    throw new AuthenticationFailure('provider_not_linked');
  }
}

export async function linkedProviders(db: Database, userId: string): Promise<ProviderName[]> {
  const rows = await db
    .select({ provider: externalIdentities.provider })
    .from(externalIdentities)
    .where(eq(externalIdentities.userId, userId))
    .orderBy(externalIdentities.provider);
  return rows.map((row) => row.provider as ProviderName);
}

/** Removes a provider unless it is the account's last remaining sign-in method. */
export async function unlinkProvider(db: Database, userId: string, provider: ProviderName) {
  const result = await db
    .delete(externalIdentities)
    .where(
      and(
        eq(externalIdentities.userId, userId),
        eq(externalIdentities.provider, provider),
        sql`(SELECT count(*) FROM external_identities WHERE user_id = ${userId}) > 1`
      )
    );
  if (result.meta.changes > 0) return;

  const [linked] = await db
    .select({ total: count() })
    .from(externalIdentities)
    .where(and(eq(externalIdentities.userId, userId), eq(externalIdentities.provider, provider)));
  if (!linked?.total) fail(404, 'provider_not_linked');
  fail(409, 'last_provider_required');
}
