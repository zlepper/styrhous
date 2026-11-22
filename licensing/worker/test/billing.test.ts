import { env, exports } from 'cloudflare:workers';
import { beforeEach, describe, expect, it } from 'vitest';
import { countRows } from './support/db.ts';
import { signedInBrowser } from './support/identity.ts';
import { addMember, createOrganization } from './support/organizations.ts';
import { origin, type Browser } from './support/portal.ts';
import { FakeStripe, signedWebhook, stripeError } from './support/stripe.ts';
import { FakeUpstream } from './support/upstream.ts';

let upstream: FakeUpstream;
let stripe: FakeStripe;

beforeEach(() => {
  upstream = new FakeUpstream().install();
  stripe = new FakeStripe(upstream);
});

const stripeApi = 'https://api.stripe.com/v1';

type Account = {
  billingAccountId: string;
  accountKind: string;
  canStartCheckout: boolean;
  entitlement: { state: string; reasonCode: string };
  trial: { isActive: boolean; terminatedAt: string | null } | null;
  subscription: { seatQuantity: number; status: string } | null;
};

async function deliver(event: Record<string, unknown>) {
  const { payload, signature } = await signedWebhook(event);
  return exports.default.fetch(`${origin}/api/webhooks/stripe`, {
    method: 'POST',
    headers: { 'Stripe-Signature': signature, 'Content-Type': 'application/json' },
    body: payload
  });
}

function subscriptionEvent(type: string, subscriptionId: string) {
  return deliver({ id: `evt_${type}_${subscriptionId}`, type, data: { object: { id: subscriptionId, object: 'subscription' } } });
}

function checkoutSessionEvent(type: string, subscriptionId: string) {
  return deliver({ id: `evt_${type}_${subscriptionId}`, type, data: { object: { id: 'cs_async', object: 'checkout.session', subscription: subscriptionId } } });
}

async function personalAccount(browser: Browser): Promise<Account> {
  return (await browser.json<{ accounts: Account[] }>('/api/billing-accounts')).body.accounts[0]!;
}

async function billingAccount(browser: Browser, billingAccountId: string): Promise<Account> {
  const { accounts } = (await browser.json<{ accounts: Account[] }>('/api/billing-accounts')).body;
  return accounts.find((account) => account.billingAccountId === billingAccountId)!;
}

async function startCheckout(browser: Browser, billingAccountId: string, seatQuantity = 1, cadence = 'monthly') {
  return browser.json<{ redirectUrl: string; reasonCode: string }>(`/api/billing-accounts/${billingAccountId}/checkout-sessions`, 'POST', {
    cadence,
    seatQuantity
  });
}

const checkoutIdOf = (redirectUrl: string) => redirectUrl.split('/').at(-1)!;

/** Asserts a Checkout uses Managed Payments, so Stripe (not Styrhous) handles tax. */
function expectManagedPaymentsCheckout(form: URLSearchParams) {
  expect(form.get('managed_payments[enabled]')).toBe('true');
  expect([...form.keys()].filter((key) => key.startsWith('automatic_tax') || key.startsWith('customer_update'))).toEqual([]);
}

const latestCheckoutForm = () => [...stripe.checkouts.values()].at(-1)!.form;

async function subscribe(
  browser: Browser,
  billingAccountId: string,
  seatQuantity = 1,
  overrides: Parameters<FakeStripe['completeCheckout']>[1] = {}
) {
  const checkout = await startCheckout(browser, billingAccountId, seatQuantity);
  expect(checkout.status).toBe(200);
  const checkoutId = checkoutIdOf(checkout.body.redirectUrl);
  const subscription = stripe.completeCheckout(checkoutId, overrides);
  const response = await deliver({ id: `evt_${checkoutId}`, type: 'checkout.session.completed', data: { object: { id: checkoutId, object: 'checkout.session', subscription: subscription.id } } });
  expect(response.status).toBe(200);
  return subscription;
}

describe('personal billing', () => {
  it('lists the personal account without a trial, and the prices', async () => {
    const { browser } = await signedInBrowser(upstream, 'billing@example.com');
    const accounts = await browser.json<{ accounts: Account[] }>('/api/billing-accounts');
    expect(accounts.body.accounts).toEqual([
      expect.objectContaining({ accountKind: 'personal', canStartCheckout: true, subscription: null, trial: null })
    ]);
    const prices = await browser.json('/api/billing-prices');
    expect(prices.body).toEqual({
      prices: [
        { cadence: 'monthly', unitAmount: 900, currency: 'eur', taxIncluded: false },
        { cadence: 'annual', unitAmount: 9000, currency: 'eur', taxIncluded: false }
      ]
    });
  });

  it('subscribes a personal account through Checkout', async () => {
    const { browser } = await signedInBrowser(upstream, 'subscriber@example.com');
    const account = await personalAccount(browser);
    expect((await startCheckout(browser, account.billingAccountId, 2, 'annual')).body).toEqual({ reasonCode: 'personal_seat_quantity_invalid' });

    await subscribe(browser, account.billingAccountId);

    const form = latestCheckoutForm();
    expectManagedPaymentsCheckout(form);
    expect(form.get('customer_email')).toBe('subscriber@example.com');
    expect(form.get('customer')).toBeNull();

    expect(await personalAccount(browser)).toMatchObject({
      canStartCheckout: false,
      entitlement: { state: 'commercial', reasonCode: 'active_subscription' },
      subscription: { seatQuantity: 1, status: 'active' },
      trial: null
    });
    expect((await startCheckout(browser, account.billingAccountId)).body).toEqual({ reasonCode: 'subscription_already_exists' });
    expect((await browser.json(`/api/billing-accounts/${account.billingAccountId}/customer-portal-sessions`, 'POST')).body).toEqual({
      reasonCode: 'customer_portal_session_created',
      redirectUrl: 'https://billing.stripe.com/p/session/1'
    });
  });

  it('reuses a matching open Checkout and replaces a different one', async () => {
    const { browser } = await signedInBrowser(upstream, 'reuse@example.com');
    const { billingAccountId } = await personalAccount(browser);
    const first = await startCheckout(browser, billingAccountId);
    const again = await startCheckout(browser, billingAccountId);
    expect(again.body.redirectUrl).toBe(first.body.redirectUrl);

    const annual = await startCheckout(browser, billingAccountId, 1, 'annual');
    expect(annual.body.redirectUrl).not.toBe(first.body.redirectUrl);
    expect(stripe.checkouts.get(checkoutIdOf(first.body.redirectUrl))!.status).toBe('expired');
  });

  it('does not offer a second purchase when the open Checkout completed before its webhook', async () => {
    const { browser } = await signedInBrowser(upstream, 'webhook-late@example.com');
    const { billingAccountId } = await personalAccount(browser);
    const checkout = await startCheckout(browser, billingAccountId);
    stripe.completeCheckout(checkoutIdOf(checkout.body.redirectUrl));

    expect(await startCheckout(browser, billingAccountId, 1, 'annual')).toEqual({
      status: 409,
      body: { reasonCode: 'subscription_already_exists' }
    });
    expect((await personalAccount(browser)).subscription).toMatchObject({ status: 'active' });
  });

  it('lets exactly one of two concurrent Checkouts start', async () => {
    const { browser } = await signedInBrowser(upstream, 'checkout-race@example.com');
    const { billingAccountId } = await personalAccount(browser);
    // Hold both Checkout creations until both requests have read the account.
    let arrived = 0;
    let release!: () => void;
    const bothArrived = new Promise<void>((resolve) => (release = resolve));
    stripe.beforeCheckoutCreate = async () => {
      if (++arrived === 2) release();
      await bothArrived;
    };

    const results = await Promise.all([startCheckout(browser, billingAccountId), startCheckout(browser, billingAccountId, 1, 'annual')]);
    const [started, rejected] = [...results].sort((a, b) => a.status - b.status);
    expect(started!.status).toBe(200);
    expect(rejected).toEqual({ status: 409, body: { reasonCode: 'checkout_operation_in_progress' } });
    const statuses = [...stripe.checkouts.values()].map((checkout) => checkout.status).sort();
    expect(statuses).toEqual(['expired', 'open']);
  });

  it('allows a new purchase once the subscription was cancelled', async () => {
    const { browser } = await signedInBrowser(upstream, 'repurchase@example.com');
    const { billingAccountId } = await personalAccount(browser);
    const first = await subscribe(browser, billingAccountId, 1, { customer: 'cus_repurchase' });
    stripe.subscriptions.get(first.id)!.status = 'canceled';
    await subscriptionEvent('customer.subscription.deleted', first.id);
    expect((await personalAccount(browser)).canStartCheckout).toBe(true);

    const second = await subscribe(browser, billingAccountId);
    const form = latestCheckoutForm();
    expectManagedPaymentsCheckout(form);
    expect(form.get('customer')).toBe('cus_repurchase');
    expect(form.get('customer_email')).toBeNull();
    expect(await personalAccount(browser)).toMatchObject({ subscription: { status: 'active' }, entitlement: { state: 'commercial' } });
    expect(await countRows('SELECT count(*) AS total FROM subscriptions WHERE stripe_subscription_id = ?', second.id)).toBe(1);

    // A further live subscription for the same account replaces nothing.
    stripe.subscriptions.set('sub_stray', { ...stripe.subscriptions.get(second.id)!, id: 'sub_stray' });
    expect((await subscriptionEvent('customer.subscription.created', 'sub_stray')).status).toBe(200);
    expect(await countRows("SELECT count(*) AS total FROM subscriptions WHERE stripe_subscription_id = 'sub_stray'")).toBe(0);
    expect(await countRows('SELECT count(*) AS total FROM subscriptions WHERE stripe_subscription_id = ?', second.id)).toBe(1);
  });

  it('licenses a delayed-payment Checkout only once its payment succeeds', async () => {
    const { browser } = await signedInBrowser(upstream, 'delayed@example.com');
    const { billingAccountId } = await personalAccount(browser);
    const subscription = await subscribe(browser, billingAccountId, 1, { status: 'incomplete' });

    expect(await personalAccount(browser)).toMatchObject({
      canStartCheckout: false,
      entitlement: { state: 'evaluation' },
      subscription: { status: 'incomplete' }
    });
    expect((await startCheckout(browser, billingAccountId)).body).toEqual({ reasonCode: 'subscription_already_exists' });

    stripe.subscriptions.get(subscription.id)!.status = 'active';
    for (let delivery = 0; delivery < 2; delivery++) {
      const response = await checkoutSessionEvent('checkout.session.async_payment_succeeded', subscription.id);
      expect(await response.json()).toMatchObject({ reasonCode: 'billing_webhook_received' });
    }
    expect(await personalAccount(browser)).toMatchObject({
      entitlement: { state: 'commercial', reasonCode: 'active_subscription' },
      subscription: { status: 'active' }
    });
    expect(await countRows('SELECT count(*) AS total FROM subscriptions WHERE billing_account_id = ?', billingAccountId)).toBe(1);
  });

  it('keeps a failed delayed payment from granting a license', async () => {
    const { browser } = await signedInBrowser(upstream, 'delayed-failed@example.com');
    const { billingAccountId } = await personalAccount(browser);
    const subscription = await subscribe(browser, billingAccountId, 1, { status: 'incomplete' });

    const response = await checkoutSessionEvent('checkout.session.async_payment_failed', subscription.id);
    expect(await response.json()).toMatchObject({ reasonCode: 'billing_webhook_received' });
    expect(await personalAccount(browser)).toMatchObject({ subscription: { status: 'incomplete' } });
    expect((await personalAccount(browser)).entitlement.state).not.toBe('commercial');
  });
});

describe('organization billing', () => {
  it("ends the organization's trial once its subscription is live", async () => {
    const owner = await signedInBrowser(upstream, 'org-trial-owner@example.com');
    const organization = await createOrganization(owner.browser);
    const account = () => billingAccount(owner.browser, organization.billingAccountId);
    expect(await account()).toMatchObject({ entitlement: { state: 'trial' }, trial: { isActive: true, terminatedAt: null } });

    // A Checkout awaiting a delayed payment keeps the trial running.
    const subscription = await subscribe(owner.browser, organization.billingAccountId, 1, { status: 'incomplete' });
    expect(await account()).toMatchObject({ entitlement: { state: 'trial' }, trial: { isActive: true, terminatedAt: null } });

    stripe.subscriptions.get(subscription.id)!.status = 'active';
    await checkoutSessionEvent('checkout.session.async_payment_succeeded', subscription.id);
    expect(await account()).toMatchObject({
      entitlement: { state: 'commercial', reasonCode: 'active_subscription' },
      trial: { isActive: false, terminatedAt: expect.any(String) }
    });
  });

  it('only lets the owner manage billing and enforces the seats in use', async () => {
    const owner = await signedInBrowser(upstream, 'org-billing-owner@example.com');
    const organization = await createOrganization(owner.browser);
    const member = await addMember(upstream, owner.browser, organization, 'org-billing-member@example.com');

    expect((await startCheckout(member.browser, organization.billingAccountId, 2)).body).toEqual({ reasonCode: 'insufficient_permission' });
    expect((await startCheckout(owner.browser, organization.billingAccountId, 1)).body).toEqual({
      reasonCode: 'seat_quantity_too_small',
      requiredSeatQuantity: 2
    });

    const subscription = await subscribe(owner.browser, organization.billingAccountId, 2);
    const seatPath = `/api/billing-accounts/${organization.billingAccountId}/seat-quantity`;
    expect((await owner.browser.json(seatPath, 'PATCH', { seatQuantity: 1 })).body).toEqual({
      reasonCode: 'seat_quantity_too_small',
      requiredSeatQuantity: 2
    });
    expect((await owner.browser.json(seatPath, 'PATCH', { seatQuantity: 2 })).body).toEqual({ reasonCode: 'seat_quantity_unchanged' });
    expect((await owner.browser.json(seatPath, 'PATCH', { seatQuantity: 4 })).body).toEqual({ reasonCode: 'seat_quantity_changed', seatQuantity: 4 });
    expect(stripe.subscriptions.get(subscription.id)!.quantity).toBe(4);
    expect(stripe.subscriptionUpdates.at(-1)!.get('payment_behavior')).toBe('pending_if_incomplete');
    expect(stripe.subscriptionUpdates.at(-1)!.get('proration_behavior')).toBe('always_invoice');

    expect((await owner.browser.json(seatPath, 'PATCH', { seatQuantity: 3 })).body).toEqual({ reasonCode: 'seat_quantity_changed', seatQuantity: 3 });
    expect(stripe.subscriptionUpdates.at(-1)!.get('proration_behavior')).toBe('none');
    expect(stripe.subscriptionUpdates.at(-1)!.get('payment_behavior')).toBeNull();

    expect((await billingAccount(owner.browser, organization.billingAccountId)).subscription!.seatQuantity).toBe(3);
  });

  it('adds seats only once their invoice is paid', async () => {
    const owner = await signedInBrowser(upstream, 'declined-owner@example.com');
    const organization = await createOrganization(owner.browser);
    const subscription = await subscribe(owner.browser, organization.billingAccountId, 1);
    stripe.declinePayments = true;

    const seatPath = `/api/billing-accounts/${organization.billingAccountId}/seat-quantity`;
    const pending = await owner.browser.json(seatPath, 'PATCH', { seatQuantity: 50 });
    expect(pending).toEqual({
      status: 200,
      body: { reasonCode: 'seat_quantity_pending_payment', seatQuantity: 50, paymentUrl: expect.stringMatching(/^https:\/\/invoice\.stripe\.com\//) }
    });
    expect(stripe.subscriptionUpdates.at(-1)!.get('expand[0]')).toBe('latest_invoice');
    expect(stripe.subscriptions.get(subscription.id)!.quantity).toBe(1);
    expect((await billingAccount(owner.browser, organization.billingAccountId)).subscription!.seatQuantity).toBe(1);

    // The owner holds the only paid seat, so there is still no room for another.
    const invitation = await owner.browser.json(`/api/organizations/${organization.organizationId}/invitations`, 'POST', {
      email: 'unpaid-seat@example.com',
      role: 'member'
    });
    expect(invitation.body).toEqual({ reasonCode: 'seat_capacity_reached' });
    expect(await countRows("SELECT count(*) AS total FROM audit_records WHERE action = 'billing_seat_quantity_payment_pending' AND target_id = ?", organization.billingAccountId)).toBe(1);

    // Paying the invoice applies the pending update, and its webhook brings the seats.
    stripe.subscriptions.get(subscription.id)!.quantity = 50;
    await deliver({ id: 'evt_seats_paid', type: 'invoice.paid', data: { object: { id: 'in_seats', parent: { subscription_details: { subscription: subscription.id } } } } });
    expect((await billingAccount(owner.browser, organization.billingAccountId)).subscription!.seatQuantity).toBe(50);
    const admitted = await owner.browser.json(`/api/organizations/${organization.organizationId}/invitations`, 'POST', {
      email: 'unpaid-seat@example.com',
      role: 'member'
    });
    expect(admitted).toMatchObject({ status: 201, body: { reasonCode: 'invitation_created' } });
  });

  it('funds the owner first when seats exceed the subscription quantity', async () => {
    const owner = await signedInBrowser(upstream, 'funding-owner@example.com');
    const organization = await createOrganization(owner.browser);
    const member = await addMember(upstream, owner.browser, organization, 'funding-member@example.com');
    const subscription = await subscribe(owner.browser, organization.billingAccountId, 2);
    stripe.subscriptions.get(subscription.id)!.quantity = 1;
    await subscriptionEvent('customer.subscription.updated', subscription.id);

    const states = async (browser: Browser) =>
      (await browser.json<{ entitlements: { billingAccountId: string; reasonCode: string }[] }>('/api/entitlements')).body.entitlements.find(
        (entitlement) => entitlement.billingAccountId === organization.billingAccountId
      )!.reasonCode;
    expect(await states(owner.browser)).toBe('active_subscription');
    expect(await states(member.browser)).toBe('subscription_seat_capacity_exceeded');
  });
});

describe('billing failures', () => {
  it('reports why a billing change was refused', async () => {
    const { browser } = await signedInBrowser(upstream, 'refusals@example.com');
    const other = await signedInBrowser(upstream, 'refusals-other@example.com');
    const { billingAccountId } = await personalAccount(browser);
    const base = `/api/billing-accounts/${billingAccountId}`;
    const refused = (status: number, reasonCode: string, details = {}) => ({ status, body: { reasonCode, ...details } });

    expect(await browser.json(`${base}/customer-portal-sessions`, 'POST')).toEqual(refused(409, 'subscription_not_found'));
    expect(await browser.json(`${base}/seat-quantity`, 'PATCH', { seatQuantity: 1 })).toEqual(refused(409, 'subscription_not_found'));
    expect(await other.browser.json(`${base}/customer-portal-sessions`, 'POST')).toEqual(refused(404, 'billing_account_not_found'));
    expect(await browser.json('/api/billing-accounts/not-a-uuid/customer-portal-sessions', 'POST')).toEqual(
      refused(404, 'billing_account_not_found')
    );

    upstream.on('GET', `${stripeApi}/prices/*`, stripeError);
    expect(await browser.json('/api/billing-prices')).toEqual(refused(503, 'billing_prices_unavailable'));
    upstream.on('POST', `${stripeApi}/checkout/sessions`, stripeError);
    expect(await startCheckout(browser, billingAccountId)).toEqual(refused(503, 'checkout_provider_unavailable'));
  });

  it('reports Stripe failures and external changes to a subscription', async () => {
    const { browser } = await signedInBrowser(upstream, 'stripe-down@example.com');
    const organization = await createOrganization(browser);
    const subscription = await subscribe(browser, organization.billingAccountId, 2);
    const base = `/api/billing-accounts/${organization.billingAccountId}`;

    upstream.on('POST', `${stripeApi}/billing_portal/sessions`, stripeError);
    expect(await browser.json(`${base}/customer-portal-sessions`, 'POST')).toEqual({
      status: 503,
      body: { reasonCode: 'customer_portal_provider_unavailable' }
    });

    stripe.subscriptions.get(subscription.id)!.quantity = 5;
    expect(await browser.json(`${base}/seat-quantity`, 'PATCH', { seatQuantity: 3 })).toEqual({
      status: 409,
      body: { reasonCode: 'subscription_quantity_changed', seatQuantity: 5 }
    });
    expect(await countRows('SELECT seat_quantity AS total FROM subscriptions WHERE stripe_subscription_id = ?', subscription.id)).toBe(5);

    upstream.on('POST', `${stripeApi}/subscriptions/*`, stripeError);
    expect(await browser.json(`${base}/seat-quantity`, 'PATCH', { seatQuantity: 6 })).toEqual({
      status: 503,
      body: { reasonCode: 'seat_quantity_provider_unavailable' }
    });

    stripe.subscriptions.get(subscription.id)!.status = 'canceled';
    await subscriptionEvent('customer.subscription.deleted', subscription.id);
    expect(await browser.json(`${base}/seat-quantity`, 'PATCH', { seatQuantity: 6 })).toEqual({
      status: 409,
      body: { reasonCode: 'subscription_inactive' }
    });
  });
});

describe('webhooks', () => {
  it('rejects bad signatures and oversized payloads', async () => {
    const { payload } = await signedWebhook({ id: 'evt_bad', type: 'invoice.paid', data: { object: {} } });
    const forged = await exports.default.fetch(`${origin}/api/webhooks/stripe`, {
      method: 'POST',
      headers: { 'Stripe-Signature': 't=1,v1=deadbeef' },
      body: payload
    });
    expect(forged.status).toBe(400);
    expect(await forged.json()).toEqual({ reasonCode: 'billing_webhook_invalid_signature' });

    const huge = await exports.default.fetch(`${origin}/api/webhooks/stripe`, { method: 'POST', body: 'x'.repeat(300 * 1024) });
    expect(huge.status).toBe(413);
  });

  it('ignores unrelated events and subscriptions that are not Styrhous licenses', async () => {
    expect(await (await deliver({ id: 'evt_other', type: 'customer.created', data: { object: { id: 'cus_9' } } })).json()).toMatchObject({
      reasonCode: 'billing_webhook_ignored'
    });
    expect(
      await (await deliver({ id: 'evt_payment', type: 'checkout.session.completed', data: { object: { id: 'cs_x', subscription: null } } })).json()
    ).toMatchObject({ reasonCode: 'billing_webhook_ignored' });

    const { browser } = await signedInBrowser(upstream, 'foreign@example.com');
    const { billingAccountId } = await personalAccount(browser);
    const foreign = { customer: 'cus_9', status: 'active', cancel_at_period_end: false, quantity: 1, periodStart: 0, periodEnd: 1 };
    stripe.subscriptions.set('sub_no_account', { ...foreign, id: 'sub_no_account', metadata: {}, priceId: env.STRIPE_MONTHLY_PRICE_ID });
    stripe.subscriptions.set('sub_unknown_price', { ...foreign, id: 'sub_unknown_price', metadata: { billing_account_id: billingAccountId }, priceId: 'price_other' });
    for (const id of ['sub_no_account', 'sub_unknown_price']) {
      expect((await subscriptionEvent('customer.subscription.updated', id)).status).toBe(200);
      expect(await countRows('SELECT count(*) AS total FROM subscriptions WHERE stripe_subscription_id = ?', id)).toBe(0);
    }
  });

  it('syncs the subscription from every handled event type, idempotently', async () => {
    const { browser } = await signedInBrowser(upstream, 'events@example.com');
    const { billingAccountId } = await personalAccount(browser);
    const checkout = await startCheckout(browser, billingAccountId);
    const subscription = stripe.completeCheckout(checkoutIdOf(checkout.body.redirectUrl));

    const created = await subscriptionEvent('customer.subscription.created', subscription.id);
    expect(await created.json()).toMatchObject({ reasonCode: 'billing_webhook_received' });
    expect((await personalAccount(browser)).subscription).toMatchObject({ status: 'active' });

    stripe.subscriptions.get(subscription.id)!.status = 'past_due';
    const paid = { id: 'evt_paid', type: 'invoice.paid', data: { object: { id: 'in_1', parent: { subscription_details: { subscription: subscription.id } } } } };
    for (const delivery of [paid, paid]) expect((await deliver(delivery)).status).toBe(200);
    expect((await personalAccount(browser)).subscription).toMatchObject({ status: 'past_due' });

    expect(await countRows('SELECT count(*) AS total FROM subscriptions WHERE billing_account_id = ?', billingAccountId)).toBe(1);
  });

  it('applies past-due and cancellation state from Stripe', async () => {
    const { browser } = await signedInBrowser(upstream, 'state@example.com');
    const { billingAccountId } = await personalAccount(browser);
    const subscription = await subscribe(browser, billingAccountId);
    const entitlement = async () => (await browser.json<{ entitlements: { state: string; reasonCode: string }[] }>('/api/entitlements')).body.entitlements[0];

    stripe.subscriptions.get(subscription.id)!.status = 'past_due';
    await deliver({ id: 'evt_failed', type: 'invoice.payment_failed', data: { object: { id: 'in_1', parent: { subscription_details: { subscription: subscription.id } } } } });
    expect(await entitlement()).toMatchObject({ state: 'grace', reasonCode: 'subscription_past_due' });

    stripe.subscriptions.get(subscription.id)!.status = 'canceled';
    await subscriptionEvent('customer.subscription.deleted', subscription.id);
    expect(await entitlement()).toMatchObject({ state: 'evaluation', reasonCode: 'subscription_inactive' });
  });

  it('fails the delivery so Stripe retries when the subscription cannot be read', async () => {
    upstream.on('GET', `${stripeApi}/subscriptions/*`, stripeError);
    const response = await subscriptionEvent('customer.subscription.updated', 'sub_unreadable');
    expect(response.status).toBe(500);
    expect(await response.json()).toEqual({ reasonCode: 'internal_error' });
    expect(await countRows("SELECT count(*) AS total FROM subscriptions WHERE stripe_subscription_id = 'sub_unreadable'")).toBe(0);
  });
});
