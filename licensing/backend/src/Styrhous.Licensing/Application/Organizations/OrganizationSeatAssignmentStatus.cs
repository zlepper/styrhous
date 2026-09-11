namespace Styrhous.Licensing.Application.Organizations;

public enum OrganizationSeatAssignmentStatus
{
    Assigned,
    Unassigned,
    OrganizationNotFound,
    MemberNotFound,
    InsufficientPermission,
    Unchanged,
    NoActiveSeatCapacity,
    SeatCapacityReached,
    ConcurrentModification,
}
