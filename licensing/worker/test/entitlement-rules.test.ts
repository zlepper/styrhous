import { describe, expect, it } from 'vitest';
import {
  fundedSeatIds,
  organizationSeatCapacity,
  seatEntitlement,
  subscriptionEntitlement,
  type SubscriptionTerms,
  type TrialTerms
} from '../src/entitlements/rules';

const now = '2030-06-15T00:00:00.000Z';
const subscription = (overrides: Partial<SubscriptionTerms> = {}): SubscriptionTerms => ({
  status: 'active',
  seatQuantity: 3,
  cancelAtPeriodEnd: false,
  currentPeriodStart: '2030-06-01T00:00:00.000Z',
  currentPeriodEnd: '2030-07-01T00:00:00.000Z',
  ...overrides
});
const trial = (overrides: Partial<TrialTerms> = {}): TrialTerms => ({
  startedAt: '2030-06-01T00:00:00.000Z',
  endsAt: '2030-07-01T00:00:00.000Z',
  terminatedAt: null,
  ...overrides
});

describe('subscription entitlement', () => {
  it.each([
    [{}, 'commercial', 'active_subscription'],
    [{ cancelAtPeriodEnd: true }, 'commercial', 'subscription_cancels_at_period_end'],
    [{ status: 'past_due' as const }, 'grace', 'subscription_past_due'],
    [{ status: 'unpaid' as const }, 'evaluation', 'subscription_inactive'],
    [{ currentPeriodStart: '2030-06-20T00:00:00.000Z' }, 'evaluation', 'subscription_not_started'],
    [{ currentPeriodEnd: '2030-06-14T00:00:00.000Z' }, 'grace', 'subscription_renewal_pending'],
    [{ currentPeriodEnd: '2030-06-10T00:00:00.000Z' }, 'evaluation', 'subscription_expired'],
    [{ currentPeriodEnd: '2030-06-14T00:00:00.000Z', cancelAtPeriodEnd: true }, 'evaluation', 'subscription_expired']
  ])('%o resolves to %s / %s', (overrides, state, reasonCode) => {
    expect(subscriptionEntitlement(subscription(overrides), now)).toMatchObject({ state, reasonCode });
  });

  it('limits renewal grace to three days past the period end', () => {
    expect(subscriptionEntitlement(subscription({ currentPeriodEnd: '2030-06-14T00:00:00.000Z' }), now).validUntil).toBe(
      '2030-06-17T00:00:00.000Z'
    );
  });
});

describe('seat entitlement', () => {
  it('requires an assigned product seat', () => {
    expect(seatEntitlement({ productAccess: false, subscription: subscription(), trial: null, funded: true }, now)).toMatchObject({
      reasonCode: 'product_seat_not_assigned'
    });
  });

  it('prefers a funded subscription, then the trial, then reports the subscription state', () => {
    expect(seatEntitlement({ productAccess: true, subscription: subscription(), trial: trial(), funded: true }, now).state).toBe('commercial');
    expect(seatEntitlement({ productAccess: true, subscription: subscription(), trial: null, funded: false }, now).reasonCode).toBe(
      'subscription_seat_capacity_exceeded'
    );
    expect(
      seatEntitlement({ productAccess: true, subscription: subscription({ status: 'canceled' }), trial: trial(), funded: true }, now).state
    ).toBe('trial');
    expect(
      seatEntitlement({ productAccess: true, subscription: subscription({ status: 'canceled' }), trial: null, funded: true }, now).reasonCode
    ).toBe('subscription_inactive');
    expect(seatEntitlement({ productAccess: true, subscription: null, trial: null, funded: false }, now).reasonCode).toBe('no_valid_entitlement');
  });

  it('ends a trial early when it is terminated', () => {
    expect(
      seatEntitlement({ productAccess: true, subscription: null, trial: trial({ terminatedAt: '2030-06-10T00:00:00.000Z' }), funded: false }, now)
        .reasonCode
    ).toBe('trial_expired');
  });
});

describe('seat funding and capacity', () => {
  it('funds the owner first, then the oldest seats', () => {
    const funded = fundedSeatIds(
      [
        { seatId: 'a', createdAt: '2030-01-01', isOwner: false },
        { seatId: 'b', createdAt: '2030-01-02', isOwner: false },
        { seatId: 'c', createdAt: '2030-01-03', isOwner: true }
      ],
      2
    );
    expect([...funded].sort()).toEqual(['a', 'c']);
  });

  it('derives organization capacity from the subscription or a running trial', () => {
    expect(organizationSeatCapacity(subscription(), null, now)).toBe(3);
    expect(organizationSeatCapacity(null, trial(), now)).toBe(5);
    expect(organizationSeatCapacity(null, trial({ terminatedAt: '2030-06-10T00:00:00.000Z' }), now)).toBeNull();
    expect(organizationSeatCapacity(subscription({ status: 'canceled' }), null, now)).toBeNull();
  });
});
