import Stripe from 'stripe';
import { and, eq, exists, gt, inArray, isNull, or } from 'drizzle-orm';
import type { Database } from '../env';
import { runBatch } from '../db/client';
import {
  billingAccounts,
  finishedStatuses,
  liveStatuses,
  subscriptionStatuses,
  subscriptions,
  trials,
  type SubscriptionStatus
} from '../db/schema';
import { ApiError, fail } from '../http/errors';
import { uuidv7 } from '../lib/ids';
import { isoNow } from '../lib/time';

export const cadences = ['monthly', 'annual'] as const;
export type Cadence = (typeof cadences)[number];

export function stripeClient(env: Env): Stripe {
  return new Stripe(env.STRIPE_SECRET_KEY, {
    httpClient: Stripe.createFetchHttpClient((input, init) => fetch(input, init)),
    maxNetworkRetries: 2
  });
}

export function priceId(env: Env, cadence: Cadence): string {
  return cadence === 'monthly' ? env.STRIPE_MONTHLY_PRICE_ID : env.STRIPE_ANNUAL_PRICE_ID;
}

function isKnownPrice(env: Env, id: string): boolean {
  return id === env.STRIPE_MONTHLY_PRICE_ID || id === env.STRIPE_ANNUAL_PRICE_ID;
}

export function fromUnix(seconds: number): string {
  return new Date(seconds * 1000).toISOString();
}

/** The id of an expandable Stripe reference. */
export function stripeId(value: string | { id: string } | null | undefined): string | null {
  return typeof value === 'string' ? value : (value?.id ?? null);
}

/**
 * Runs Stripe calls for a request, reporting Stripe failures as a 503 with `reasonCode`.
 * Only identifiers go into `logContext`.
 */
export async function withStripe<T>(
  reasonCode: string,
  logContext: Readonly<Record<string, string>>,
  operation: () => Promise<T>
): Promise<T> {
  try {
    return await operation();
  } catch (error) {
    if (error instanceof ApiError) throw error;
    console.error('Stripe request failed', { reasonCode, ...logContext, error: String(error) });
    fail(503, reasonCode);
  }
}

/** The single licensed item of a Styrhous subscription. */
export function licenseItem(env: Env, subscription: Stripe.Subscription): Stripe.SubscriptionItem | null {
  const items = subscription.items.data;
  return items.length === 1 && isKnownPrice(env, items[0]!.price.id) ? items[0]! : null;
}

/**
 * Stores Stripe's current view of a subscription. Always reading the subscription from Stripe
 * (rather than trusting event payloads) makes repeated or reordered webhooks harmless.
 */
export async function storeSubscription(
  env: Env,
  db: Database,
  subscription: Stripe.Subscription
): Promise<'stored' | 'ignored'> {
  const billingAccountId = subscription.metadata.billing_account_id;
  const item = licenseItem(env, subscription);
  const status = subscription.status as SubscriptionStatus;
  if (!billingAccountId || !item || !subscriptionStatuses.includes(status)) {
    console.warn('Ignoring a subscription that is not a Styrhous license', { subscriptionId: subscription.id });
    return 'ignored';
  }

  const now = isoNow();
  const values = {
    stripeCustomerId: stripeId(subscription.customer)!,
    stripeSubscriptionId: subscription.id,
    stripePriceId: item.price.id,
    status,
    seatQuantity: item.quantity ?? 1,
    cancelAtPeriodEnd: subscription.cancel_at_period_end,
    currentPeriodStart: fromUnix(item.current_period_start),
    currentPeriodEnd: fromUnix(item.current_period_end),
    syncedAt: now
  };
  const isStored = exists(
    db
      .select({ id: subscriptions.id })
      .from(subscriptions)
      .where(eq(subscriptions.stripeSubscriptionId, subscription.id))
  );

  // A different subscription only replaces the stored one once that one has finished.
  const [upsert] = await runBatch(db, [
    db
      .insert(subscriptions)
      .values({ id: uuidv7(), billingAccountId, ...values })
      .onConflictDoUpdate({
        target: subscriptions.billingAccountId,
        set: values,
        setWhere: or(
          eq(subscriptions.stripeSubscriptionId, subscription.id),
          inArray(subscriptions.status, finishedStatuses)
        )
      }),
    db
      .update(billingAccounts)
      .set({ stripeCustomerId: values.stripeCustomerId, openCheckoutSessionId: null, openCheckoutExpiresAt: null })
      .where(and(eq(billingAccounts.id, billingAccountId), isStored)),
    // Paid commercial entitlement supersedes a running trial.
    ...(liveStatuses.includes(status)
      ? [
          db
            .update(trials)
            .set({ terminatedAt: now })
            .where(
              and(eq(trials.billingAccountId, billingAccountId), isNull(trials.terminatedAt), gt(trials.endsAt, now), isStored)
            )
        ]
      : [])
  ]);
  if (upsert!.meta.changes === 0) {
    // The customer paid for a second subscription that grants nothing; refund or cancel it.
    console.error('Ignored a second live subscription for a billing account; cancel it in Stripe', {
      billingAccountId,
      subscriptionId: subscription.id
    });
    return 'ignored';
  }
  return 'stored';
}

export async function syncSubscription(env: Env, db: Database, stripe: Stripe, subscriptionId: string) {
  return storeSubscription(env, db, await stripe.subscriptions.retrieve(subscriptionId));
}
