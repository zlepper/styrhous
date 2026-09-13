using Styrhous.Licensing.Domain.Billing;

namespace Styrhous.Licensing.Application.Entitlements;

public sealed record CommercialEntitlement(
    EntitlementState State,
    string ReasonCode,
    DateTimeOffset ValidFrom,
    DateTimeOffset ValidUntil)
{
    public bool IsEligible =>
        State is EntitlementState.Commercial or EntitlementState.Grace;

    public static CommercialEntitlement Resolve(
        CommercialSubscriptionStatus status,
        bool cancelsAtPeriodEnd,
        DateTimeOffset periodStartedAt,
        DateTimeOffset periodEndsAt,
        DateTimeOffset observedAt)
    {
        if (!Enum.IsDefined(status))
        {
            throw new InvalidOperationException("The subscription status is invalid.");
        }

        var validFrom = periodStartedAt.ToUniversalTime();
        var validUntil = periodEndsAt.ToUniversalTime();
        if (validUntil <= validFrom)
        {
            throw new InvalidOperationException(
                "A subscription period must end after it starts.");
        }

        var utcObservedAt = observedAt.ToUniversalTime();
        var (state, reasonCode) = utcObservedAt switch
        {
            _ when utcObservedAt < validFrom =>
                (EntitlementState.Evaluation, EntitlementReasonCodes.SubscriptionNotStarted),
            _ when utcObservedAt >= validUntil =>
                (EntitlementState.Evaluation,
                    !cancelsAtPeriodEnd && status is CommercialSubscriptionStatus.Active or CommercialSubscriptionStatus.PastDue
                        ? EntitlementReasonCodes.SubscriptionRenewalPending : EntitlementReasonCodes.SubscriptionExpired),
            _ => ResolveStatus(status, cancelsAtPeriodEnd),
        };
        return new CommercialEntitlement(state, reasonCode, validFrom, validUntil);
    }

    private static (EntitlementState State, string ReasonCode) ResolveStatus(
        CommercialSubscriptionStatus status,
        bool cancelsAtPeriodEnd)
    {
        return status switch
        {
            CommercialSubscriptionStatus.Active =>
                cancelsAtPeriodEnd
                    ? (EntitlementState.Commercial,
                        EntitlementReasonCodes.SubscriptionCancelsAtPeriodEnd)
                    : (EntitlementState.Commercial,
                        EntitlementReasonCodes.ActiveSubscription),
            CommercialSubscriptionStatus.PastDue =>
                (EntitlementState.Grace, EntitlementReasonCodes.SubscriptionPastDue),
            _ => (EntitlementState.Evaluation, EntitlementReasonCodes.SubscriptionInactive),
        };
    }
}
