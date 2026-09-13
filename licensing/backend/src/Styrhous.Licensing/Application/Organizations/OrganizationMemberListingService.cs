using Styrhous.Licensing.Persistence;

namespace Styrhous.Licensing.Application.Organizations;

public sealed class OrganizationMemberListingService(PostgresOrganizationMemberListingStore store)
{

    public Task<OrganizationMemberListingResult> ListAsync(
        Guid actorUserId,
        Guid organizationId,
        CancellationToken cancellationToken = default)
    {

        return store.ListAsync(
            actorUserId,
            organizationId,
            cancellationToken);
    }
}
