using Styrhous.Licensing.Persistence;
using Styrhous.Licensing.Domain.Organizations;

namespace Styrhous.Licensing.Application.Organizations;

public sealed class OrganizationRoleManagementService(
    PostgresOrganizationRoleManagementStore store,
    TimeProvider timeProvider)
{

    public Task<OrganizationRoleManagementResult> ChangeRoleAsync(
        Guid actorUserId,
        Guid organizationId,
        Guid membershipId,
        OrganizationRole requestedRole,
        CancellationToken cancellationToken = default)
    {
        ValidateRole(requestedRole);

        return ChangeRoleAsync(
            actorUserId,
            organizationId,
            (Guid?)membershipId,
            requestedRole,
            cancellationToken);
    }

    public Task<OrganizationRoleManagementResult> ChangeRoleAsync(
        Guid actorUserId,
        Guid organizationId,
        string candidateMembershipId,
        OrganizationRole requestedRole,
        CancellationToken cancellationToken = default)
    {
        ValidateRole(requestedRole);
        return ChangeRoleAsync(
            actorUserId,
            organizationId,
            Guid.TryParse(candidateMembershipId, out var membershipId)
                ? membershipId
                : null,
            requestedRole,
            cancellationToken);
    }

    public Task<OrganizationRoleManagementResult> TransferOwnershipAsync(
        Guid actorUserId,
        Guid organizationId,
        Guid membershipId,
        CancellationToken cancellationToken = default)
    {

        return TransferOwnershipAsync(
            actorUserId,
            organizationId,
            (Guid?)membershipId,
            cancellationToken);
    }

    public Task<OrganizationRoleManagementResult> TransferOwnershipAsync(
        Guid actorUserId,
        Guid organizationId,
        string candidateMembershipId,
        CancellationToken cancellationToken = default)
    {
        return TransferOwnershipAsync(
            actorUserId,
            organizationId,
            Guid.TryParse(candidateMembershipId, out var membershipId)
                ? membershipId
                : null,
            cancellationToken);
    }

    private Task<OrganizationRoleManagementResult> ChangeRoleAsync(
        Guid actorUserId,
        Guid organizationId,
        Guid? membershipId,
        OrganizationRole requestedRole,
        CancellationToken cancellationToken)
    {
        return store.ChangeRoleAsync(
            actorUserId,
            organizationId,
            membershipId,
            requestedRole,
            timeProvider.GetUtcNow().ToUniversalTime(),
            Guid.CreateVersion7(),
            cancellationToken);
    }

    private Task<OrganizationRoleManagementResult> TransferOwnershipAsync(
        Guid actorUserId,
        Guid organizationId,
        Guid? membershipId,
        CancellationToken cancellationToken)
    {
        return store.TransferOwnershipAsync(
            actorUserId,
            organizationId,
            membershipId,
            timeProvider.GetUtcNow().ToUniversalTime(),
            Guid.CreateVersion7(),
            cancellationToken);
    }


    private static void ValidateRole(OrganizationRole role)
    {
        if (role is not OrganizationRole.Admin and not OrganizationRole.Member)
        {
            throw new ArgumentException(
                "The requested role must be Admin or Member.",
                nameof(role));
        }
    }
}
