import { and, eq, inArray, isNotNull, or, sql } from 'drizzle-orm';
import { Hono, type Context } from 'hono';
import type Stripe from 'stripe';
import { z } from 'zod';
import type { AppEnv, Database } from '../env';
import { auditor } from '../audit';
import { requireSession } from '../auth/session';
import {
  billingAccounts,
  finishedStatuses,
  liveStatuses,
  memberships,
  organizations,
  seats,
  subscriptions
} from '../db/schema';
import { entitlementFor, loadBillingContexts } from '../entitlements/queries';
import { isEligible, trialEntitlement, unassignedSeat } from '../entitlements/rules';
import { fail } from '../http/errors';
import { validate } from '../http/validation';
import { isUuid } from '../lib/ids';
import { hourMs, isoNow } from '../lib/time';
import { seatsNeeded } from '../organizations/capacity';
import {
  cadences,
  fromUnix,
  licenseItem,
  priceId,
  storeSubscription,
  stripeClient,
  stripeId,
  syncSubscription,
  withStripe,
  type Cadence
} from './stripe';

export const billingRoutes = new Hono<AppEnv>();

const seatQuantity = z.int().min(1).max(10_000);

/** Billing accounts the user can see: their personal account and their organizations'. */
function accountQuery(db: Database, userId: string, billingAccountId?: string) {
  return db
    .select({
      billingAccountId: billingAccounts.id,
      accountKind: billingAccounts.kind,
      organizationId: organizations.id,
      organizationName: organizations.name,
      organizationRole: memberships.role,
      stripeCustomerId: billingAccounts.stripeCustomerId,
      openCheckoutSessionId: billingAccounts.openCheckoutSessionId
    })
    .from(billingAccounts)
    .leftJoin(organizations, eq(organizations.billingAccountId, billingAccounts.id))
    .leftJoin(
      memberships,
      and(eq(memberships.organizationId, organizations.id), eq(memberships.userId, userId))
    )
    .where(
      and(
        or(eq(billingAccounts.personalOwnerUserId, userId), isNotNull(memberships.id)),
        billingAccountId ? eq(billingAccounts.id, billingAccountId) : undefined
      )
    );
}

type AccountRow = Awaited<ReturnType<typeof accountQuery>>[number];

function canManage(account: AccountRow): boolean {
  return account.accountKind === 'personal' || account.organizationRole === 'owner';
}

/** An account the caller owns; members see 403 and everyone else 404. */
async function managedAccount(c: Context<AppEnv>): Promise<AccountRow> {
  const session = requireSession(c);
  const billingAccountId = c.req.param('billingAccountId') ?? '';
  if (!isUuid(billingAccountId)) fail(404, 'billing_account_not_found');
  const [account] = await accountQuery(c.var.db, session.userId, billingAccountId);
  if (!account) fail(404, 'billing_account_not_found');
  if (!canManage(account)) fail(403, 'insufficient_permission');
  return account;
}

/** Validates a requested quantity against the seats in use and seat-bearing invitations. */
async function assertSeatQuantity(db: Database, account: AccountRow, quantity: number, now: string) {
  if (account.accountKind === 'personal') {
    if (quantity !== 1) fail(409, 'personal_seat_quantity_invalid');
    return;
  }
  const [row] = await db.all<{ needed: number }>(
    sql`SELECT ${seatsNeeded(account.billingAccountId, account.organizationId!, now)} AS needed`
  );
  const required = Math.max(1, row?.needed ?? 1);
  if (quantity < required) fail(409, 'seat_quantity_too_small', { requiredSeatQuantity: required });
}

async function storedSubscription(db: Database, billingAccountId: string) {
  const [row] = await db.select().from(subscriptions).where(eq(subscriptions.billingAccountId, billingAccountId));
  return row ?? null;
}

billingRoutes.get('/billing-accounts', async (c) => {
  const session = requireSession(c);
  const db = c.var.db;
  const now = isoNow();
  const accounts = await accountQuery(db, session.userId).orderBy(billingAccounts.createdAt);
  const ids = accounts.map((account) => account.billingAccountId);
  const [seatRows, contexts] = await Promise.all([
    ids.length === 0
      ? []
      : db
          .select({ id: seats.id, billingAccountId: seats.billingAccountId, productAccess: seats.productAccess, userId: seats.userId })
          .from(seats)
          .where(inArray(seats.billingAccountId, ids)),
    loadBillingContexts(db, ids)
  ]);

  return c.json({
    reasonCode: 'billing_accounts_listed',
    accounts: accounts.map((account) => {
      const id = account.billingAccountId;
      const context = contexts.get(id)!;
      const { trial, subscription } = context;
      const ownSeat = seatRows.find((seat) => seat.billingAccountId === id && seat.userId === session.userId);
      const manageable = canManage(account);
      return {
        billingAccountId: id,
        accountKind: account.accountKind,
        organizationId: account.organizationId,
        organizationName: account.organizationName,
        organizationRole: account.organizationRole,
        canManageBilling: manageable,
        canStartCheckout: manageable && (!subscription || finishedStatuses.includes(subscription.status)),
        assignedSeatCount: seatRows.filter((seat) => seat.billingAccountId === id && seat.productAccess).length,
        // Administrators without a product seat still see the account's state.
        entitlement: ownSeat
          ? entitlementFor(ownSeat, context, now)
          : { seatId: null, billingAccountId: id, ...unassignedSeat, isEligible: false },
        trial: trial && {
          trialId: trial.id,
          startedAt: trial.startedAt,
          endsAt: trial.endsAt,
          terminatedAt: trial.terminatedAt,
          isActive: isEligible(trialEntitlement(trial, now))
        },
        subscription: subscription && {
          subscriptionId: subscription.id,
          status: subscription.status,
          seatQuantity: subscription.seatQuantity,
          cancelAtPeriodEnd: subscription.cancelAtPeriodEnd,
          currentPeriodStartedAt: subscription.currentPeriodStart,
          currentPeriodEndsAt: subscription.currentPeriodEnd,
          projectedAt: subscription.syncedAt
        }
      };
    })
  });
});

billingRoutes.get('/billing-prices', async (c) => {
  requireSession(c);
  const stripe = stripeClient(c.env);
  const prices = await withStripe('billing_prices_unavailable', {}, () =>
    Promise.all(
      cadences.map(async (cadence) => {
        const price = await stripe.prices.retrieve(priceId(c.env, cadence));
        if (price.unit_amount === null || price.transform_quantity) throw new Error(`Unsupported ${cadence} price`);
        return {
          cadence,
          unitAmount: price.unit_amount,
          currency: price.currency,
          taxIncluded:
            price.tax_behavior === 'inclusive' ? true : price.tax_behavior === 'exclusive' ? false : null
        };
      })
    )
  );
  return c.json({ prices });
});

billingRoutes.post(
  '/billing-accounts/:billingAccountId/checkout-sessions',
  validate('json', z.object({ cadence: z.enum(cadences), seatQuantity })),
  async (c) => {
    const session = requireSession(c);
    const db = c.var.db;
    const account = await managedAccount(c);
    const { cadence, seatQuantity } = c.req.valid('json');
    const now = isoNow();
    const stripe = stripeClient(c.env);
    const logContext = { billingAccountId: account.billingAccountId };

    const existing = await storedSubscription(db, account.billingAccountId);
    if (existing && !finishedStatuses.includes(existing.status)) fail(409, 'subscription_already_exists');
    await assertSeatQuantity(db, account, seatQuantity, now);

    const reused = await withStripe('checkout_provider_unavailable', logContext, () =>
      reuseOpenCheckout(c.env, db, stripe, account, cadence, seatQuantity)
    );
    if (reused) return c.json({ reasonCode: 'checkout_session_created', redirectUrl: reused });

    const metadata = { billing_account_id: account.billingAccountId };
    const checkout = await withStripe('checkout_provider_unavailable', logContext, () =>
      stripe.checkout.sessions.create({
        mode: 'subscription',
        line_items: [{ price: priceId(c.env, cadence), quantity: seatQuantity }],
        // Stripe is the merchant of record: it calculates and remits tax, so Checkout
        // takes no tax or address-update parameters.
        managed_payments: { enabled: true },
        ...(account.stripeCustomerId ? { customer: account.stripeCustomerId } : { customer_email: session.email }),
        client_reference_id: account.billingAccountId,
        metadata,
        subscription_data: { metadata },
        success_url: `${c.env.PUBLIC_ORIGIN}/billing?checkout=success`,
        cancel_url: `${c.env.PUBLIC_ORIGIN}/billing?checkout=cancelled`,
        expires_at: Math.floor((Date.now() + hourMs) / 1000)
      })
    );

    const audit = auditor(db, { actorUserId: session.userId, occurredAt: now });
    const [claim] = await db.batch([
      db
        .update(billingAccounts)
        .set({ openCheckoutSessionId: checkout.id, openCheckoutExpiresAt: fromUnix(checkout.expires_at) })
        .where(
          and(
            eq(billingAccounts.id, account.billingAccountId),
            sql`${billingAccounts.openCheckoutSessionId} IS ${account.openCheckoutSessionId}`
          )
        ),
      audit.insert('billing_checkout_started', 'billing_account', account.billingAccountId)
    ]);
    if (claim.meta.changes === 0) {
      // A concurrent request stored its own Checkout first; keep only one open.
      await stripe.checkout.sessions.expire(checkout.id).catch(() => undefined);
      fail(409, 'checkout_operation_in_progress');
    }
    return c.json({ reasonCode: 'checkout_session_created', redirectUrl: checkout.url });
  }
);

/**
 * Returns the account's open Checkout when it matches the request. A different open Checkout
 * is expired first so an account never has two ways to subscribe, and a completed one whose
 * webhook has not arrived yet is synced instead of offering a second purchase.
 */
async function reuseOpenCheckout(
  env: Env,
  db: Database,
  stripe: Stripe,
  account: AccountRow,
  cadence: Cadence,
  quantity: number
): Promise<string | null> {
  if (!account.openCheckoutSessionId) return null;
  const open = await stripe.checkout.sessions.retrieve(account.openCheckoutSessionId, { expand: ['line_items'] });
  const subscriptionId = stripeId(open.subscription);
  if (open.status === 'complete' && subscriptionId) {
    await syncSubscription(env, db, stripe, subscriptionId);
    fail(409, 'subscription_already_exists');
  }
  if (open.status !== 'open') return null;
  const line = open.line_items?.data[0];
  if (line?.price?.id === priceId(env, cadence) && line.quantity === quantity && open.url) return open.url;
  await stripe.checkout.sessions.expire(open.id);
  return null;
}

billingRoutes.post('/billing-accounts/:billingAccountId/customer-portal-sessions', async (c) => {
  const account = await managedAccount(c);
  const subscription = await storedSubscription(c.var.db, account.billingAccountId);
  if (!subscription) fail(409, 'subscription_not_found');
  const portal = await withStripe(
    'customer_portal_provider_unavailable',
    { billingAccountId: account.billingAccountId },
    () =>
      stripeClient(c.env).billingPortal.sessions.create({
        customer: subscription.stripeCustomerId,
        configuration: c.env.STRIPE_PORTAL_CONFIGURATION_ID,
        return_url: `${c.env.PUBLIC_ORIGIN}/billing`
      })
  );
  return c.json({ reasonCode: 'customer_portal_session_created', redirectUrl: portal.url });
});

billingRoutes.patch(
  '/billing-accounts/:billingAccountId/seat-quantity',
  validate('json', z.object({ seatQuantity })),
  async (c) => {
    const session = requireSession(c);
    const db = c.var.db;
    const account = await managedAccount(c);
    const { seatQuantity } = c.req.valid('json');
    const now = isoNow();

    const subscription = await storedSubscription(db, account.billingAccountId);
    if (!subscription) fail(409, 'subscription_not_found');
    if (!liveStatuses.includes(subscription.status)) fail(409, 'subscription_inactive');
    if (subscription.seatQuantity === seatQuantity) fail(409, 'seat_quantity_unchanged');
    await assertSeatQuantity(db, account, seatQuantity, now);

    const stripe = stripeClient(c.env);
    const increase = seatQuantity > subscription.seatQuantity;
    const updated = await withStripe(
      'seat_quantity_provider_unavailable',
      { billingAccountId: account.billingAccountId },
      async () => {
        const current = await stripe.subscriptions.retrieve(subscription.stripeSubscriptionId);
        const item = licenseItem(c.env, current);
        if (!item) fail(409, 'subscription_inactive');
        if ((item.quantity ?? 1) !== subscription.seatQuantity) {
          await storeSubscription(c.env, db, current);
          fail(409, 'subscription_quantity_changed', { seatQuantity: item.quantity ?? 1 });
        }
        // Added seats are invoiced right away and only take effect once that invoice is paid.
        return stripe.subscriptions.update(current.id, {
          items: [{ id: item.id, quantity: seatQuantity }],
          proration_behavior: increase ? 'always_invoice' : 'none',
          ...(increase ? { payment_behavior: 'pending_if_incomplete' as const, expand: ['latest_invoice'] } : {})
        });
      }
    );

    // A pending update leaves the stored quantity, and so the seat capacity, unchanged.
    await storeSubscription(c.env, db, updated);
    // Only this request's increase can be pending; a decrease never waits for payment.
    const pending = increase && updated.pending_update !== null;
    await auditor(db, { actorUserId: session.userId, occurredAt: now }).record(
      pending ? 'billing_seat_quantity_payment_pending' : 'billing_seat_quantity_changed',
      'billing_account',
      account.billingAccountId
    );
    if (pending) {
      const invoice = updated.latest_invoice;
      return c.json({
        reasonCode: 'seat_quantity_pending_payment',
        seatQuantity,
        paymentUrl: typeof invoice === 'object' ? (invoice?.hosted_invoice_url ?? null) : null
      });
    }
    return c.json({ reasonCode: 'seat_quantity_changed', seatQuantity });
  }
);
