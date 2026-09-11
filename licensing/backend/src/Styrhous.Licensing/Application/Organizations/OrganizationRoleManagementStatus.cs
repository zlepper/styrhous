namespace Styrhous.Licensing.Application.Organizations;

public enum OrganizationRoleManagementStatus
{
    RoleChanged,
    OwnershipTransferred,
    OrganizationNotFound,
    MemberNotFound,
    InsufficientPermission,
    OwnershipTransferRequired,
    RoleUnchanged,
    InvalidOwnershipTarget,
    ConcurrentModification,
}
