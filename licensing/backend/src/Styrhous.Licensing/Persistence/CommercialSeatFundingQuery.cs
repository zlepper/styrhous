using Microsoft.EntityFrameworkCore;
using Styrhous.Licensing.Application.Entitlements;
using Styrhous.Licensing.Domain.Organizations;

namespace Styrhous.Licensing.Persistence;

internal sealed record CommercialSeatFundingCandidate(
    Guid SeatId,
    Guid BillingAccountId,
    Guid AssignedUserId,
    DateTimeOffset CreatedAt);

internal sealed record CommercialSeatFundingOwner(
    Guid BillingAccountId,
    Guid UserId);

internal static class CommercialSeatFundingQuery
{
    public static async Task<IReadOnlySet<Guid>> ResolveFundedSeatIdsAsync(
        LicensingDbContext dbContext,
        IReadOnlyCollection<SeatEntitlementRow> rows,
        CancellationToken cancellationToken)
    {
        var quantities = rows
            .Where(row => row.SubscriptionSeatQuantity is not null)
            .GroupBy(row => row.BillingAccountId)
            .ToDictionary(
                group => group.Key,
                group => group.Select(row => row.SubscriptionSeatQuantity!.Value).Single());
        if (quantities.Count == 0)
        {
            return new HashSet<Guid>();
        }

        var billingAccountIds = quantities.Keys.ToArray();
        var owners = await (
                from organization in dbContext.Organizations.AsNoTracking()
                join membership in dbContext.OrganizationMemberships.AsNoTracking()
                    on organization.Id equals membership.OrganizationId
                where billingAccountIds.Contains(organization.BillingAccountId)
                    && membership.Role == OrganizationRole.Owner
                select new CommercialSeatFundingOwner(
                    organization.BillingAccountId,
                    membership.UserId))
            .ToDictionaryAsync(
                owner => owner.BillingAccountId,
                owner => owner.UserId,
                cancellationToken);
        var candidates = await dbContext.Seats
            .AsNoTracking()
            .Where(seat => billingAccountIds.Contains(seat.BillingAccountId)
                && seat.ProductAccessEnabled)
            .OrderBy(seat => seat.BillingAccountId)
            .ThenBy(seat => seat.CreatedAt)
            .ThenBy(seat => seat.Id)
            .Select(seat => new CommercialSeatFundingCandidate(
                seat.Id,
                seat.BillingAccountId,
                seat.AssignedUserId,
                seat.CreatedAt))
            .ToArrayAsync(cancellationToken);

        return CommercialSeatFundingPolicy.Resolve(
            candidates
                .Select(candidate => new CommercialSeatCandidate(
                    candidate.SeatId,
                    candidate.BillingAccountId,
                    candidate.CreatedAt,
                    candidate.AssignedUserId
                        == owners.GetValueOrDefault(candidate.BillingAccountId)))
                .ToArray(),
            quantities);
    }
}
