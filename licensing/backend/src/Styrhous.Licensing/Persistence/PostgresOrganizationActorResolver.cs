using Microsoft.EntityFrameworkCore;
using Styrhous.Licensing.Application.Accounts;
using Styrhous.Licensing.Domain.Organizations;

namespace Styrhous.Licensing.Persistence;

internal sealed record OrganizationActorContext(
    Organization Organization,
    OrganizationRole Role);

internal static class PostgresOrganizationActorResolver
{
    public static async Task<OrganizationActorContext?> SerializeAndResolveAsync(
        LicensingDbContext dbContext,
        Guid actorUserId,
        Guid organizationId,
        CancellationToken cancellationToken)
    {
        if (!await EfTransactionSerialization.TryClaimUserAsync(
                dbContext,
                actorUserId,
                cancellationToken))
        {
            throw new UserNotFoundException();
        }

        var organization = await EfTransactionSerialization.FindOrganizationAndClaimAsync(
            dbContext,
            organizationId,
            cancellationToken);
        if (organization is null)
        {
            return null;
        }

        var actorRole = await dbContext.OrganizationMemberships
            .AsNoTracking()
            .Where(membership => membership.OrganizationId == organizationId
                && membership.UserId == actorUserId)
            .Select(membership => (OrganizationRole?)membership.Role)
            .SingleOrDefaultAsync(cancellationToken);

        return actorRole is null
            ? null
            : new OrganizationActorContext(organization, actorRole.Value);
    }
}
