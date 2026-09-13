using Styrhous.Licensing.Persistence;
using Styrhous.Licensing.Domain.Identifiers;

namespace Styrhous.Licensing.Application.Organizations;

public sealed class OrganizationMemberRemovalService(
    PostgresOrganizationMemberRemovalStore store,
    TimeProvider timeProvider)
{

    public Task<OrganizationMemberRemovalResult> RemoveAsync(
        Guid actorUserId,
        Guid organizationId,
        Guid membershipId,
        CancellationToken cancellationToken = default)
    {

        return store.RemoveAsync(
            actorUserId,
            organizationId,
            membershipId,
            timeProvider.GetUtcNow().ToUniversalTime(),
            Uuid7.Create(),
            cancellationToken);
    }

    public Task<OrganizationMemberRemovalResult> RemoveAsync(
        Guid actorUserId,
        Guid organizationId,
        string candidateMembershipId,
        CancellationToken cancellationToken = default)
    {
        var parsedMembershipId = Guid.TryParse(
            candidateMembershipId,
            out var membershipId)
            ? membershipId
            : (Guid?)null;
        return store.RemoveAsync(
            actorUserId,
            organizationId,
            parsedMembershipId,
            timeProvider.GetUtcNow().ToUniversalTime(),
            Uuid7.Create(),
            cancellationToken);
    }

}
