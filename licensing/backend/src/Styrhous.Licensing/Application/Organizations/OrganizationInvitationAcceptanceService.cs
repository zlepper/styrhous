using Styrhous.Licensing.Persistence;
using Styrhous.Licensing.Domain.Identifiers;

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
            Uuid7.Create(),
            cancellationToken);
    }
}
