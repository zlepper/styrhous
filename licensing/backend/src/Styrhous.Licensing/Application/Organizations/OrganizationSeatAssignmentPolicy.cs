using Styrhous.Licensing.Domain.Organizations;

namespace Styrhous.Licensing.Application.Organizations;

internal enum OrganizationSeatAssignmentDecision
{
    Allowed,
    InsufficientPermission,
    Unchanged,
}

internal static class OrganizationSeatAssignmentPolicy
{
    public static bool CanManage(OrganizationRole actorRole)
    {
        return actorRole is OrganizationRole.Owner or OrganizationRole.Admin;
    }

    public static OrganizationSeatAssignmentDecision Decide(
        OrganizationRole actorRole,
        bool productAccessEnabled,
        bool requestedAssignment)
    {
        if (!CanManage(actorRole))
        {
            return OrganizationSeatAssignmentDecision.InsufficientPermission;
        }

        return productAccessEnabled == requestedAssignment
            ? OrganizationSeatAssignmentDecision.Unchanged
            : OrganizationSeatAssignmentDecision.Allowed;
    }
}
