using Styrhous.Licensing.Application.Entitlements;

namespace Styrhous.Licensing.Api.Entitlements;

public sealed record SeatEntitlementResponse(
    Guid SeatId,
    Guid BillingAccountId,
    string State,
    string ReasonCode,
    bool IsEligible,
    DateTimeOffset? ValidFrom,
    DateTimeOffset? ValidUntil)
{
    internal static SeatEntitlementResponse From(SeatEntitlement entitlement)
    {
        return new(
            entitlement.SeatId,
            entitlement.BillingAccountId,
            ToApiValue(entitlement.State),
            entitlement.ReasonCode,
            entitlement.IsEligible,
            entitlement.ValidFrom,
            entitlement.ValidUntil);
    }

    private static string ToApiValue(EntitlementState state)
    {
        return state switch
        {
            EntitlementState.Evaluation => "evaluation",
            EntitlementState.Trial => "trial",
            EntitlementState.Commercial => "commercial",
            EntitlementState.Grace => "grace",
            _ => throw new ArgumentOutOfRangeException(nameof(state), state, null),
        };
    }
}
