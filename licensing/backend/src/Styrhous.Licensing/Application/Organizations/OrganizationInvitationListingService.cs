using Styrhous.Licensing.Persistence;
using Styrhous.Licensing.Domain.Identifiers;

namespace Styrhous.Licensing.Application.Organizations;

public sealed class OrganizationInvitationListingService(
    PostgresOrganizationInvitationListingStore store,
    TimeProvider timeProvider)
{

    public Task<OrganizationInvitationListingResult> ListAsync(
        Guid actorUserId,
        Guid organizationId,
        CancellationToken cancellationToken = default)
    {

        return store.ListAsync(
            actorUserId,
            organizationId,
            timeProvider.GetUtcNow(),
            cancellationToken);
    }
}
