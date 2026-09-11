namespace Styrhous.Licensing.Application.Organizations;

public abstract class OrganizationSeatAssignmentResult
{
    private OrganizationSeatAssignmentResult(OrganizationSeatAssignmentStatus status)
    {
        Status = status;
    }

    public OrganizationSeatAssignmentStatus Status { get; }

    public sealed class Success : OrganizationSeatAssignmentResult
    {
        internal Success(
            OrganizationSeatAssignmentStatus status,
            Guid organizationId,
            Guid membershipId,
            Guid userId,
            Guid seatId,
            bool assigned,
            int deviceLimit,
            Guid correlationId,
            DateTimeOffset changedAt)
            : base(status)
        {
            if (status is not OrganizationSeatAssignmentStatus.Assigned
                and not OrganizationSeatAssignmentStatus.Unassigned)
            {
                throw new ArgumentOutOfRangeException(nameof(status));
            }

            OrganizationId = organizationId;
            MembershipId = membershipId;
            UserId = userId;
            SeatId = seatId;
            Assigned = assigned;
            DeviceLimit = deviceLimit;
            CorrelationId = correlationId;
            ChangedAt = changedAt;
        }

        public Guid OrganizationId { get; }

        public Guid MembershipId { get; }

        public Guid UserId { get; }

        public Guid SeatId { get; }

        public bool Assigned { get; }

        public int DeviceLimit { get; }

        public Guid CorrelationId { get; }

        public DateTimeOffset ChangedAt { get; }
    }

    public sealed class Rejection(OrganizationSeatAssignmentStatus status)
        : OrganizationSeatAssignmentResult(status)
    {
    }

    internal static Success Changed(
        Guid organizationId,
        Guid membershipId,
        Guid userId,
        Guid seatId,
        bool assigned,
        int deviceLimit,
        Guid correlationId,
        DateTimeOffset changedAt)
    {
        return new(
            assigned
                ? OrganizationSeatAssignmentStatus.Assigned
                : OrganizationSeatAssignmentStatus.Unassigned,
            organizationId,
            membershipId,
            userId,
            seatId,
            assigned,
            deviceLimit,
            correlationId,
            changedAt);
    }

    internal static Rejection Rejected(OrganizationSeatAssignmentStatus status)
    {
        return new(status);
    }
}
