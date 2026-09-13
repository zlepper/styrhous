namespace Styrhous.Licensing.Application.Organizations;

public enum OrganizationInvitationCreationStatus
{
    Created,
    OrganizationNotFound,
    InsufficientPermission,
    AlreadyMember,
    InvitationAlreadyPending,
    NoActiveSeatCapacity,
    SeatCapacityReached,
}
