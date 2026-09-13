using Styrhous.Licensing.Domain.Auditing;

namespace Styrhous.Licensing.Application.Organizations;

internal static class OrganizationRoleManagementAuditRecords
{
    public static AuditRecord RoleChanged(
        Guid actorUserId,
        Guid membershipId,
        Guid correlationId,
        DateTimeOffset observedAt)
    {
        return AuditRecord.Create(
            correlationId,
            actorUserId,
            AuditAction.OrganizationMemberRoleChanged,
            AuditTargetType.OrganizationMembership,
            membershipId,
            observedAt);
    }

    public static AuditRecord[] OwnershipTransferred(
        Guid actorUserId,
        Guid previousOwnerMembershipId,
        Guid ownerMembershipId,
        Guid correlationId,
        DateTimeOffset observedAt)
    {
        return [
            RoleChanged(
                actorUserId,
                previousOwnerMembershipId,
                correlationId,
                observedAt),
            AuditRecord.Create(
                correlationId,
                actorUserId,
                AuditAction.OrganizationOwnerAssigned,
                AuditTargetType.OrganizationMembership,
                ownerMembershipId,
                observedAt),
        ];
    }
}
