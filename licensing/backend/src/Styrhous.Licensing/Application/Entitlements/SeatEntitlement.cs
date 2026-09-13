using Styrhous.Licensing.Domain.Identifiers;
using Styrhous.Licensing.Domain.Billing;

namespace Styrhous.Licensing.Application.Entitlements;

public sealed record SeatEntitlement(
    Guid SeatId,
    Guid BillingAccountId,
    EntitlementState State,
    string ReasonCode,
    DateTimeOffset? ValidFrom,
    DateTimeOffset? ValidUntil)
{
    public bool IsEligible =>
        State is EntitlementState.Trial
            or EntitlementState.Commercial
            or EntitlementState.Grace;

    public static SeatEntitlement Resolve(
        SeatEntitlementSource source,
        DateTimeOffset observedAt)
    {
        ArgumentNullException.ThrowIfNull(source);

        if (!source.ProductAccessEnabled)
        {
            return new SeatEntitlement(
                source.SeatId,
                source.BillingAccountId,
                EntitlementState.Evaluation,
                EntitlementReasonCodes.ProductSeatNotAssigned,
                ValidFrom: null,
                ValidUntil: null);
        }

        var subscriptionEntitlement = ResolveSubscription(source, observedAt);
        var trialEntitlement = ResolveTrial(source, observedAt);
        if (subscriptionEntitlement?.IsEligible == true)
        {
            return source.Commercial!.IsSeatFunded
                ? subscriptionEntitlement
                : subscriptionEntitlement with
                {
                    State = EntitlementState.Evaluation,
                    ReasonCode = EntitlementReasonCodes.SubscriptionSeatCapacityExceeded,
                };
        }

        if (trialEntitlement.IsEligible)
        {
            return trialEntitlement;
        }

        if (subscriptionEntitlement?.ReasonCode == EntitlementReasonCodes.SubscriptionRenewalPending
            && !source.Commercial!.IsSeatFunded)
        {
            return subscriptionEntitlement with { ReasonCode = EntitlementReasonCodes.SubscriptionSeatCapacityExceeded };
        }

        return subscriptionEntitlement ?? trialEntitlement;
    }

    private static SeatEntitlement ResolveTrial(
        SeatEntitlementSource source,
        DateTimeOffset observedAt)
    {
        if (source.TrialStartedAt is null && source.TrialEndsAt is null)
        {
            return new SeatEntitlement(
                source.SeatId,
                source.BillingAccountId,
                EntitlementState.Evaluation,
                EntitlementReasonCodes.NoValidEntitlement,
                ValidFrom: null,
                ValidUntil: null);
        }

        if (source.TrialStartedAt is null || source.TrialEndsAt is null)
        {
            throw new InvalidOperationException("A trial must have both validity boundaries.");
        }

        var validFrom = source.TrialStartedAt.Value.ToUniversalTime();
        var validUntil = source.TrialEndsAt.Value.ToUniversalTime();
        if (validUntil <= validFrom)
        {
            throw new InvalidOperationException("A trial must end after it starts.");
        }

        var utcObservedAt = observedAt.ToUniversalTime();
        var (state, reasonCode) = utcObservedAt switch
        {
            _ when utcObservedAt < validFrom =>
                (EntitlementState.Evaluation, EntitlementReasonCodes.TrialNotStarted),
            _ when utcObservedAt < validUntil =>
                (EntitlementState.Trial, EntitlementReasonCodes.ActiveTrial),
            _ => (EntitlementState.Evaluation, EntitlementReasonCodes.TrialExpired),
        };

        return new SeatEntitlement(
            source.SeatId,
            source.BillingAccountId,
            state,
            reasonCode,
            validFrom,
            validUntil);
    }

    private static SeatEntitlement? ResolveSubscription(
        SeatEntitlementSource source,
        DateTimeOffset observedAt)
    {
        if (source.Commercial is null)
        {
            return null;
        }

        var commercial = CommercialEntitlement.Resolve(
            source.Commercial.Status,
            source.Commercial.CancelsAtPeriodEnd,
            source.Commercial.PeriodStartedAt,
            source.Commercial.PeriodEndsAt,
            observedAt);
        return new SeatEntitlement(
            source.SeatId,
            source.BillingAccountId,
            commercial.State,
            commercial.ReasonCode,
            commercial.ValidFrom,
            commercial.ValidUntil);
    }
}
