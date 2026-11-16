import Stripe from 'stripe';
import { testBindings } from '../test-config.ts';
import { FakeUpstream, json } from './upstream.ts';

type FakeSubscription = {
  id: string;
  customer: string;
  status: string;
  cancel_at_period_end: boolean;
  metadata: Record<string, string>;
  priceId: string;
  quantity: number;
  periodStart: number;
  periodEnd: number;
};

type FakeCheckout = {
  id: string;
  url: string;
  status: string;
  priceId: string;
  quantity: number;
  expiresAt: number;
  subscription: string | null;
  form: URLSearchParams;
};

const api = 'https://api.stripe.com/v1';
const day = 86_400;
// Stripe identifiers are globally unique, also across tests sharing a database.
let counter = 0;

/** An in-memory stand-in for the parts of the Stripe API the Worker uses. */
export class FakeStripe {
  readonly subscriptions = new Map<string, FakeSubscription>();
  readonly checkouts = new Map<string, FakeCheckout>();
  /** Awaited before each Checkout is created, to line up concurrent requests. */
  beforeCheckoutCreate: () => Promise<void> = async () => undefined;
  /** Whether invoices for added seats stay unpaid, leaving the seat increase pending. */
  declinePayments = false;
  subscriptionUpdates: URLSearchParams[] = [];

  constructor(upstream: FakeUpstream) {
    upstream
      .on('GET', `${api}/prices/*`, (_request, url) => {
        const id = url.pathname.split('/').at(-1)!;
        return json({ id, unit_amount: id === testBindings.STRIPE_MONTHLY_PRICE_ID ? 900 : 9000, currency: 'eur', tax_behavior: 'exclusive', transform_quantity: null });
      })
      .on('POST', `${api}/checkout/sessions`, async (request) => {
        const form = await formFields(request);
        await this.beforeCheckoutCreate();
        const id = `cs_${++counter}`;
        const checkout = {
          id,
          url: `https://checkout.stripe.com/c/pay/${id}`,
          status: 'open',
          priceId: form.get('line_items[0][price]')!,
          quantity: Number(form.get('line_items[0][quantity]')),
          expiresAt: Number(form.get('expires_at')),
          subscription: null,
          form
        };
        this.checkouts.set(id, checkout);
        return json(this.checkoutJson(checkout));
      })
      .on('GET', `${api}/checkout/sessions/*`, (_request, url) =>
        json(this.checkoutJson(this.checkouts.get(url.pathname.split('/').at(-1)!)!))
      )
      .on('POST', `${api}/checkout/sessions/*`, (_request, url) => {
        const checkout = this.checkouts.get(url.pathname.split('/').at(-2)!)!;
        checkout.status = 'expired';
        return json(this.checkoutJson(checkout));
      })
      .on('POST', `${api}/billing_portal/sessions`, () => json({ id: 'bps_1', url: 'https://billing.stripe.com/p/session/1' }))
      .on('GET', `${api}/subscriptions/*`, (_request, url) =>
        json(this.subscriptionJson(this.subscriptions.get(url.pathname.split('/').at(-1)!)!))
      )
      .on('POST', `${api}/subscriptions/*`, async (request, url) => {
        const subscription = this.subscriptions.get(url.pathname.split('/').at(-1)!)!;
        const form = await formFields(request);
        this.subscriptionUpdates.push(form);
        const quantity = Number(form.get('items[0][quantity]'));
        if (this.declinePayments && form.get('payment_behavior') === 'pending_if_incomplete') {
          return json({
            ...this.subscriptionJson(subscription),
            pending_update: { subscription_items: [{ quantity }] },
            latest_invoice: { id: `in_${++counter}`, object: 'invoice', hosted_invoice_url: `https://invoice.stripe.com/i/in_${counter}` }
          });
        }
        subscription.quantity = quantity;
        return json(this.subscriptionJson(subscription));
      });
  }

  /** Completes a Checkout as the customer would, creating its subscription. */
  completeCheckout(checkoutId: string, overrides: Partial<FakeSubscription> = {}): FakeSubscription {
    const checkout = this.checkouts.get(checkoutId)!;
    const now = Math.floor(Date.now() / 1000);
    const subscription = {
      id: `sub_${++counter}`,
      customer: 'cus_1',
      status: 'active',
      cancel_at_period_end: false,
      metadata: { billing_account_id: checkout.form.get('subscription_data[metadata][billing_account_id]')! },
      priceId: checkout.priceId,
      quantity: checkout.quantity,
      periodStart: now - day,
      periodEnd: now + 29 * day,
      ...overrides
    };
    this.subscriptions.set(subscription.id, subscription);
    checkout.status = 'complete';
    checkout.subscription = subscription.id;
    return subscription;
  }

  private checkoutJson(checkout: FakeCheckout) {
    return {
      id: checkout.id,
      object: 'checkout.session',
      url: checkout.status === 'open' ? checkout.url : null,
      status: checkout.status,
      expires_at: checkout.expiresAt,
      subscription: checkout.subscription,
      line_items: { object: 'list', data: [{ price: { id: checkout.priceId }, quantity: checkout.quantity }] }
    };
  }

  private subscriptionJson(subscription: FakeSubscription) {
    return {
      id: subscription.id,
      object: 'subscription',
      customer: subscription.customer,
      status: subscription.status,
      cancel_at_period_end: subscription.cancel_at_period_end,
      metadata: subscription.metadata,
      pending_update: null,
      items: {
        object: 'list',
        data: [
          {
            id: `si_${subscription.id}`,
            price: { id: subscription.priceId },
            quantity: subscription.quantity,
            current_period_start: subscription.periodStart,
            current_period_end: subscription.periodEnd
          }
        ]
      }
    };
  }
}

/** A Stripe API error that the SDK does not retry. */
export function stripeError(): Response {
  return json({ error: { type: 'invalid_request_error', message: 'Simulated Stripe failure' } }, 400);
}

/** A webhook delivery signed with the test endpoint secret. */
export async function signedWebhook(event: Record<string, unknown>, secret = testBindings.STRIPE_WEBHOOK_SECRET) {
  const payload = JSON.stringify({ object: 'event', created: Math.floor(Date.now() / 1000), ...event });
  const signature = await new Stripe('sk_test_signer').webhooks.generateTestHeaderStringAsync({
    payload,
    secret,
    cryptoProvider: Stripe.createSubtleCryptoProvider()
  });
  return { payload, signature };
}

async function formFields(request: Request): Promise<URLSearchParams> {
  const fields = new URLSearchParams();
  for (const [key, value] of await request.formData()) fields.append(key, String(value));
  return fields;
}
