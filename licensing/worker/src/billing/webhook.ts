import { Hono } from 'hono';
import Stripe from 'stripe';
import type { AppEnv } from '../env';
import { fail } from '../http/errors';
import { stripeClient, stripeId, syncSubscription } from './stripe';

const maxPayloadBytes = 256 * 1024;

export const stripeWebhookRoutes = new Hono<AppEnv>();

/**
 * Stripe retries failed deliveries, so processing happens inline: verify the signature,
 * re-read the affected subscription, and return 500 on any failure.
 */
stripeWebhookRoutes.post('/', async (c) => {
  const declaredLength = Number(c.req.header('Content-Length') ?? 0);
  if (declaredLength > maxPayloadBytes) fail(413, 'billing_webhook_payload_too_large');
  const payload = await c.req.text();
  if (new TextEncoder().encode(payload).byteLength > maxPayloadBytes) {
    fail(413, 'billing_webhook_payload_too_large');
  }

  const stripe = stripeClient(c.env);
  let event: Stripe.Event;
  try {
    event = await stripe.webhooks.constructEventAsync(
      payload,
      c.req.header('Stripe-Signature') ?? '',
      c.env.STRIPE_WEBHOOK_SECRET,
      undefined,
      Stripe.createSubtleCryptoProvider()
    );
  } catch {
    fail(400, 'billing_webhook_invalid_signature');
  }

  const subscriptionId = affectedSubscription(event);
  if (!subscriptionId) return c.json({ reasonCode: 'billing_webhook_ignored', eventId: event.id });
  await syncSubscription(c.env, c.var.db, stripe, subscriptionId);
  return c.json({ reasonCode: 'billing_webhook_received', eventId: event.id });
});

function affectedSubscription(event: Stripe.Event): string | null {
  switch (event.type) {
    case 'checkout.session.completed':
    case 'checkout.session.async_payment_succeeded':
    case 'checkout.session.async_payment_failed':
      return stripeId(event.data.object.subscription);
    case 'customer.subscription.created':
    case 'customer.subscription.updated':
    case 'customer.subscription.deleted':
      return event.data.object.id;
    case 'invoice.paid':
    case 'invoice.payment_failed':
      return stripeId(event.data.object.parent?.subscription_details?.subscription);
    default:
      return null;
  }
}
