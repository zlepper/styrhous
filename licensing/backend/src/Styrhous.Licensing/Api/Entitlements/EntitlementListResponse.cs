using Styrhous.Licensing.Application.Entitlements;

namespace Styrhous.Licensing.Api.Entitlements;

public sealed record EntitlementListResponse(
    IReadOnlyList<SeatEntitlementResponse> Entitlements)
{
    internal static EntitlementListResponse From(
        IReadOnlyList<SeatEntitlement> entitlements)
    {
        return new(entitlements.Select(SeatEntitlementResponse.From).ToArray());
    }
}
