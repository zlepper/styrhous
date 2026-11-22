import { liveStatuses, type SubscriptionStatus } from '../db/schema';
import { dayMs, isoAfter } from '../lib/time';

export type EntitlementState = 'evaluation' | 'trial' | 'commercial' | 'grace';

export type Entitlement = Readonly<{
  state: EntitlementState;
  reasonCode: string;
  validFrom: string | null;
  validUntil: string | null;
}>;

export type SeatEntitlement = Entitlement &
  Readonly<{ seatId: string; billingAccountId: string; isEligible: boolean }>;

export type SubscriptionTerms = Readonly<{
  status: SubscriptionStatus;
  seatQuantity: number;
  cancelAtPeriodEnd: boolean;
  currentPeriodStart: string;
  currentPeriodEnd: string;
}>;

export type TrialTerms = Readonly<{
  startedAt: string;
  endsAt: string;
  terminatedAt: string | null;
}>;

/** How long the trial a user's first organization starts with lasts. */
export const trialDurationMs = 30 * dayMs;

/** Seat capacity of an organization during its trial. */
const trialSeatCapacity = 5;

/**
 * How long an active subscription whose period has ended keeps commercial entitlement while
 * Stripe's renewal (and its webhook) is still outstanding.
 */
const renewalGraceMs = 3 * dayMs;

/** A user without a product seat on the account. */
export const unassignedSeat: Entitlement = {
  state: 'evaluation',
  reasonCode: 'product_seat_not_assigned',
  validFrom: null,
  validUntil: null
};

export function isEligible(entitlement: Entitlement): boolean {
  return entitlement.state !== 'evaluation';
}

export function subscriptionEntitlement(terms: SubscriptionTerms, now: string): Entitlement {
  const validFrom = terms.currentPeriodStart;
  const periodEnd = terms.currentPeriodEnd;
  const evaluation = (reasonCode: string, validUntil = periodEnd): Entitlement => ({
    state: 'evaluation',
    reasonCode,
    validFrom,
    validUntil
  });

  if (now < validFrom) return evaluation('subscription_not_started');
  if (now >= periodEnd) {
    const renewing = !terms.cancelAtPeriodEnd && liveStatuses.includes(terms.status);
    const graceEnd = isoAfter(periodEnd, renewalGraceMs);
    return renewing && now < graceEnd
      ? { state: 'grace', reasonCode: 'subscription_renewal_pending', validFrom, validUntil: graceEnd }
      : evaluation('subscription_expired');
  }
  switch (terms.status) {
    case 'active':
      return {
        state: 'commercial',
        reasonCode: terms.cancelAtPeriodEnd ? 'subscription_cancels_at_period_end' : 'active_subscription',
        validFrom,
        validUntil: periodEnd
      };
    case 'past_due':
      return { state: 'grace', reasonCode: 'subscription_past_due', validFrom, validUntil: periodEnd };
    default:
      return evaluation('subscription_inactive');
  }
}

export function trialEntitlement(trial: TrialTerms | null, now: string): Entitlement {
  if (!trial) {
    return { state: 'evaluation', reasonCode: 'no_valid_entitlement', validFrom: null, validUntil: null };
  }
  const validFrom = trial.startedAt;
  const validUntil = trial.terminatedAt ?? trial.endsAt;
  if (now < validFrom) return { state: 'evaluation', reasonCode: 'trial_not_started', validFrom, validUntil };
  if (now < validUntil) return { state: 'trial', reasonCode: 'active_trial', validFrom, validUntil };
  return { state: 'evaluation', reasonCode: 'trial_expired', validFrom, validUntil };
}

export type SeatContext = Readonly<{
  productAccess: boolean;
  subscription: SubscriptionTerms | null;
  trial: TrialTerms | null;
  /** Whether the subscription's quantity covers this seat. */
  funded: boolean;
}>;

export function seatEntitlement(seat: SeatContext, now: string): Entitlement {
  if (!seat.productAccess) return unassignedSeat;
  const commercial = seat.subscription ? subscriptionEntitlement(seat.subscription, now) : null;
  if (commercial && isEligible(commercial)) {
    return seat.funded
      ? commercial
      : { ...commercial, state: 'evaluation', reasonCode: 'subscription_seat_capacity_exceeded' };
  }
  const trial = trialEntitlement(seat.trial, now);
  return isEligible(trial) ? trial : (commercial ?? trial);
}

export type FundingCandidate = Readonly<{ seatId: string; createdAt: string; isOwner: boolean }>;

/** The first `quantity` product seats are funded: the owner's first, then oldest first. */
export function fundedSeatIds(candidates: readonly FundingCandidate[], quantity: number): Set<string> {
  const ordered = [...candidates].sort(
    (a, b) =>
      Number(b.isOwner) - Number(a.isOwner) ||
      a.createdAt.localeCompare(b.createdAt) ||
      a.seatId.localeCompare(b.seatId)
  );
  return new Set(ordered.slice(0, quantity).map((candidate) => candidate.seatId));
}

/** Product seats an organization may assign right now, or null without active capacity. */
export function organizationSeatCapacity(
  subscription: SubscriptionTerms | null,
  trial: TrialTerms | null,
  now: string
): number | null {
  if (subscription && isEligible(subscriptionEntitlement(subscription, now))) {
    return subscription.seatQuantity;
  }
  if (trial && isEligible(trialEntitlement(trial, now))) {
    return trialSeatCapacity;
  }
  return null;
}
