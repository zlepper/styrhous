using Styrhous.Licensing.Domain.Organizations;

namespace Styrhous.Licensing.Application.Organizations;

internal enum OrganizationMemberRemovalDecision
{
    Allowed,
    InsufficientPermission,
    OwnershipTransferRequired,
}

internal static class OrganizationMemberRemovalPolicy
{
    public static OrganizationMemberRemovalDecision Decide(
        Guid actorUserId,
        OrganizationRole actorRole,
        Guid targetUserId,
        OrganizationRole targetRole)
    {
        if (targetRole == OrganizationRole.Owner)
        {
            return OrganizationMemberRemovalDecision.OwnershipTransferRequired;
        }

        if (actorUserId == targetUserId)
        {
            return OrganizationMemberRemovalDecision.Allowed;
        }

        return (actorRole, targetRole) switch
        {
            (OrganizationRole.Owner, OrganizationRole.Admin or OrganizationRole.Member) =>
                OrganizationMemberRemovalDecision.Allowed,
            (OrganizationRole.Admin, OrganizationRole.Member) =>
                OrganizationMemberRemovalDecision.Allowed,
            _ => OrganizationMemberRemovalDecision.InsufficientPermission,
        };
    }
}
