using Microsoft.EntityFrameworkCore;
using Styrhous.Licensing.Application.Organizations;

namespace Styrhous.Licensing.Persistence;

public sealed class PostgresOrganizationMemberListingStore(LicensingDbContext dbContext)

{

    public async Task<OrganizationMemberListingResult> ListAsync(
        Guid actorUserId,
        Guid organizationId,
        CancellationToken cancellationToken)
    {
        await PostgresReadChecks.EnsureUserExistsAsync(
            dbContext,
            actorUserId,
            cancellationToken);

        var rows = await (
                from membership in dbContext.OrganizationMemberships.AsNoTracking()
                join user in dbContext.UserAccounts.AsNoTracking()
                    on membership.UserId equals user.Id
                join organization in dbContext.Organizations.AsNoTracking()
                    on membership.OrganizationId equals organization.Id
                join candidateSeat in dbContext.Seats.AsNoTracking()
                    on new
                    {
                        organization.BillingAccountId,
                        membership.UserId,
                    }
                    equals new
                    {
                        candidateSeat.BillingAccountId,
                        UserId = candidateSeat.AssignedUserId,
                    }
                    into assignedSeats
                from seat in assignedSeats.DefaultIfEmpty()
                where membership.OrganizationId == organizationId
                    && dbContext.OrganizationMemberships.Any(
                        actorMembership =>
                            actorMembership.OrganizationId == organizationId
                            && actorMembership.UserId == actorUserId)
                orderby membership.CreatedAt, membership.Id
                select new
                {
                    MembershipId = membership.Id,
                    membership.UserId,
                    SeatId = (Guid?)seat.Id,
                    user.VerifiedEmail,
                    membership.Role,
                    ProductSeatAssigned = (bool?)seat.ProductAccessEnabled,
                    DeviceLimit = (int?)seat.DeviceLimit,
                    JoinedAt = membership.CreatedAt,
                })
            .ToArrayAsync(cancellationToken);
        if (rows.Length == 0)
        {
            return OrganizationMemberListingResult.Rejected(
                OrganizationMemberListingStatus.OrganizationNotFound);
        }

        if (rows.Any(row => row.SeatId is null
                || row.ProductSeatAssigned is null
                || row.DeviceLimit is null))
        {
            throw new InvalidOperationException(
                "An organization membership is missing its assigned seat.");
        }

        var members = rows
            .Select(row => new OrganizationMemberSummary(
                row.MembershipId,
                row.UserId,
                row.SeatId!.Value,
                row.VerifiedEmail,
                row.Role,
                row.ProductSeatAssigned!.Value,
                row.DeviceLimit!.Value,
                row.JoinedAt))
            .ToArray();
        return OrganizationMemberListingResult.Listed(organizationId, members);
    }
}
