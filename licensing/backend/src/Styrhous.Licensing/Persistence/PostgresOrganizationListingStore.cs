using Microsoft.EntityFrameworkCore;
using Styrhous.Licensing.Application.Organizations;

namespace Styrhous.Licensing.Persistence;

public sealed class PostgresOrganizationListingStore(LicensingDbContext dbContext)

{

    public async Task<IReadOnlyList<OrganizationSummary>> ListForUserAsync(
        Guid userId,
        CancellationToken cancellationToken)
    {
        await PostgresReadChecks.EnsureUserExistsAsync(
            dbContext,
            userId,
            cancellationToken);

        return await (
                from membership in dbContext.OrganizationMemberships.AsNoTracking()
                join organization in dbContext.Organizations.AsNoTracking()
                    on membership.OrganizationId equals organization.Id
                join seat in dbContext.Seats.AsNoTracking()
                    on new
                    {
                        organization.BillingAccountId,
                        membership.UserId,
                    }
                    equals new
                    {
                        seat.BillingAccountId,
                        UserId = seat.AssignedUserId,
                    }
                where membership.UserId == userId
                orderby membership.CreatedAt, membership.Id
                select new OrganizationSummary(
                    organization.Id,
                    organization.BillingAccountId,
                    membership.Id,
                    seat.Id,
                    organization.Name,
                    membership.Role,
                    seat.ProductAccessEnabled,
                    seat.DeviceLimit,
                    membership.CreatedAt))
            .ToArrayAsync(cancellationToken);
    }
}
