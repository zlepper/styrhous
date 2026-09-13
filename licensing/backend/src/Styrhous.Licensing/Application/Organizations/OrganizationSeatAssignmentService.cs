using Styrhous.Licensing.Persistence;

namespace Styrhous.Licensing.Application.Organizations;

public sealed class OrganizationSeatAssignmentService(
    PostgresOrganizationSeatAssignmentStore store,
    TimeProvider timeProvider)
{

    public Task<OrganizationSeatAssignmentResult> SetAssignedAsync(
        Guid actorUserId,
        Guid organizationId,
        string candidateMembershipId,
        bool assigned,
        CancellationToken cancellationToken = default)
    {

        return store.SetAssignedAsync(
            actorUserId,
            organizationId,
            Guid.TryParse(candidateMembershipId, out var membershipId)
                ? membershipId
                : null,
            assigned,
            timeProvider.GetUtcNow().ToUniversalTime(),
            Guid.CreateVersion7(),
            cancellationToken);
    }
}
