namespace Styrhous.Licensing.Application.Entitlements;

internal sealed record CommercialSeatCandidate(
    Guid SeatId,
    Guid BillingAccountId,
    DateTimeOffset CreatedAt,
    bool IsOrganizationOwner);

internal static class CommercialSeatFundingPolicy
{
    public static IReadOnlySet<Guid> Resolve(
        IReadOnlyCollection<CommercialSeatCandidate> candidates,
        IReadOnlyDictionary<Guid, int> seatQuantities)
    {
        var fundedSeatIds = new HashSet<Guid>();
        foreach (var billingAccountSeats in candidates.GroupBy(
                     candidate => candidate.BillingAccountId))
        {
            if (!seatQuantities.TryGetValue(
                    billingAccountSeats.Key,
                    out var seatQuantity))
            {
                continue;
            }

            if (seatQuantity <= 0)
            {
                throw new ArgumentOutOfRangeException(
                    nameof(seatQuantities),
                    "A commercial subscription must fund at least one seat.");
            }

            foreach (var candidate in billingAccountSeats
                         .OrderByDescending(candidate => candidate.IsOrganizationOwner)
                         .ThenBy(candidate => candidate.CreatedAt)
                         .ThenBy(candidate => candidate.SeatId)
                         .Take(seatQuantity))
            {
                fundedSeatIds.Add(candidate.SeatId);
            }
        }

        return fundedSeatIds;
    }
}
