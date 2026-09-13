namespace Styrhous.Licensing.Application.Organizations;

public enum OrganizationInvitationResendStatus
{
    Resent,
    OrganizationNotFound,
    InsufficientPermission,
    InvitationNotFound,
    Superseded,
    AlreadyMember,
    InvitationAlreadyPending,
    NoActiveSeatCapacity,
    SeatCapacityReached,
}
