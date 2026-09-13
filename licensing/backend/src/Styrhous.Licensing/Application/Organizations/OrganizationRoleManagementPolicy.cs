using Styrhous.Licensing.Domain.Organizations;

namespace Styrhous.Licensing.Application.Organizations;

internal enum OrganizationRoleManagementDecision
{
    Allowed,
    InsufficientPermission,
    InvalidOwnershipTarget,
    InvalidRole,
    OwnershipTransferRequired,
    RoleUnchanged,
}

internal static class OrganizationRoleManagementPolicy
{
    public static OrganizationRoleManagementDecision DecideRoleChange(
        OrganizationRole actorRole,
        OrganizationRole targetRole,
        OrganizationRole requestedRole)
    {
        if (requestedRole is not OrganizationRole.Admin and not OrganizationRole.Member)
        {
            return OrganizationRoleManagementDecision.InvalidRole;
        }

        if (actorRole != OrganizationRole.Owner)
        {
            return OrganizationRoleManagementDecision.InsufficientPermission;
        }

        if (targetRole == OrganizationRole.Owner)
        {
            return OrganizationRoleManagementDecision.OwnershipTransferRequired;
        }

        return targetRole == requestedRole
            ? OrganizationRoleManagementDecision.RoleUnchanged
            : OrganizationRoleManagementDecision.Allowed;
    }

    public static OrganizationRoleManagementDecision DecideOwnershipTransfer(
        Guid actorUserId,
        OrganizationRole actorRole,
        Guid targetUserId,
        OrganizationRole targetRole)
    {
        if (actorRole != OrganizationRole.Owner)
        {
            return OrganizationRoleManagementDecision.InsufficientPermission;
        }

        return actorUserId == targetUserId
            || targetRole is not OrganizationRole.Admin and not OrganizationRole.Member
            ? OrganizationRoleManagementDecision.InvalidOwnershipTarget
            : OrganizationRoleManagementDecision.Allowed;
    }
}
