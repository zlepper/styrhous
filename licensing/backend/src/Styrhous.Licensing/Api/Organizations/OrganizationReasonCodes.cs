namespace Styrhous.Licensing.Api.Organizations;

public static class OrganizationReasonCodes
{
    public const string OrganizationsListed = "organizations_listed";

    public const string OrganizationNotFound = "organization_not_found";

    public const string MembersListed = "organization_members_listed";

    public const string MemberRemoved = "organization_member_removed";

    public const string MemberRoleChanged = "organization_member_role_changed";

    public const string MemberRoleUnchanged = "organization_member_role_unchanged";

    public const string MemberSeatAssigned = "organization_member_seat_assigned";

    public const string MemberSeatUnassigned = "organization_member_seat_unassigned";

    public const string MemberSeatUnchanged = "organization_member_seat_unchanged";

    public const string MemberNotFound = "organization_member_not_found";

    public const string InsufficientPermission = "insufficient_permission";

    public const string OwnershipTransferRequired = "ownership_transfer_required";

    public const string OwnershipTransferred = "organization_ownership_transferred";

    public const string InvalidOwnershipTarget = "ownership_transfer_target_invalid";

    public const string MembersChanged = "organization_members_changed";
}
