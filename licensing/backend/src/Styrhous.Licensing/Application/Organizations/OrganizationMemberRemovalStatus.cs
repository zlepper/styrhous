namespace Styrhous.Licensing.Application.Organizations;

public enum OrganizationMemberRemovalStatus
{
    Removed,
    OrganizationNotFound,
    MemberNotFound,
    InsufficientPermission,
    OwnershipTransferRequired,
    ConcurrentModification,
}
