using Styrhous.Licensing.Persistence;

namespace Styrhous.Licensing.Application.Organizations;

public sealed class OrganizationInvitationCancellationService(
    PostgresOrganizationInvitationCancellationStore store,
    TimeProvider timeProvider)
{

    public async Task<OrganizationInvitationCancellationResult> CancelAsync(
        Guid actorUserId,
        Guid organizationId,
        Guid invitationId,
        CancellationToken cancellationToken = default)
    {

        var observedAt = timeProvider.GetUtcNow();
        var correlationId = Guid.CreateVersion7();
        var status = await store.CancelAsync(
            actorUserId,
            organizationId,
            invitationId,
            observedAt,
            correlationId,
            cancellationToken);
        return status == OrganizationInvitationCancellationStatus.Cancelled
            ? OrganizationInvitationCancellationResult.Cancelled(
                invitationId,
                correlationId,
                observedAt)
            : OrganizationInvitationCancellationResult.Rejected(status);
    }
}
