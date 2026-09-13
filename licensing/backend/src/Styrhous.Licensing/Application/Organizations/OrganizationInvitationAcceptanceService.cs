using Styrhous.Licensing.Persistence;

namespace Styrhous.Licensing.Application.Organizations;

public sealed class OrganizationInvitationAcceptanceService(
    PostgresOrganizationInvitationAcceptanceStore store)
{

    public Task<OrganizationInvitationAcceptanceResult> AcceptAsync(
        Guid actorUserId,
        string secret,
        CancellationToken cancellationToken = default)
    {

        var invitationSecret = new OrganizationInvitationSecret(secret);
        return store.AcceptAsync(
            actorUserId,
            invitationSecret.Hash,
            Guid.CreateVersion7(),
            cancellationToken);
    }
}
