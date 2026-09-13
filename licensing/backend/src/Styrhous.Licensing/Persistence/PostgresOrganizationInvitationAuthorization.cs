using Styrhous.Licensing.Domain.Organizations;

namespace Styrhous.Licensing.Persistence;

internal enum OrganizationInvitationAuthorizationStatus
{
    Authorized,
    OrganizationNotFound,
    InsufficientPermission,
}

internal sealed class OrganizationInvitationAuthorization
{
    private OrganizationInvitationAuthorization(
        OrganizationInvitationAuthorizationStatus status,
        Organization? organization)
    {
        Status = status;
        _organization = organization;
    }

    public OrganizationInvitationAuthorizationStatus Status { get; }

    public Organization Organization => _organization
        ?? throw new InvalidOperationException(
            "An authorized organization result must contain its organization.");

    private readonly Organization? _organization;

    public static OrganizationInvitationAuthorization Authorized(Organization organization)
    {
        return new(OrganizationInvitationAuthorizationStatus.Authorized, organization);
    }

    public static OrganizationInvitationAuthorization OrganizationNotFound()
    {
        return new(OrganizationInvitationAuthorizationStatus.OrganizationNotFound, null);
    }

    public static OrganizationInvitationAuthorization InsufficientPermission()
    {
        return new(OrganizationInvitationAuthorizationStatus.InsufficientPermission, null);
    }
}

internal static class PostgresOrganizationInvitationAuthorization
{
    public static async Task<OrganizationInvitationAuthorization> SerializeAndAuthorizeAsync(
        LicensingDbContext dbContext,
        Guid actorUserId,
        Guid organizationId,
        CancellationToken cancellationToken)
    {
        var actor = await PostgresOrganizationActorResolver.SerializeAndResolveAsync(
            dbContext,
            actorUserId,
            organizationId,
            cancellationToken);
        if (actor is null)
        {
            return OrganizationInvitationAuthorization.OrganizationNotFound();
        }

        return actor.Role is OrganizationRole.Owner or OrganizationRole.Admin
            ? OrganizationInvitationAuthorization.Authorized(actor.Organization)
            : OrganizationInvitationAuthorization.InsufficientPermission();
    }
}
