namespace Styrhous.Licensing.Application.Organizations;

public abstract class OrganizationMemberRemovalResult
{
    private OrganizationMemberRemovalResult(OrganizationMemberRemovalStatus status)
    {
        Status = status;
    }

    public OrganizationMemberRemovalStatus Status { get; }

    public sealed class Success : OrganizationMemberRemovalResult
    {
        internal Success(
            Guid organizationId,
            Guid membershipId,
            Guid userId,
            Guid seatId,
            Guid correlationId,
            DateTimeOffset removedAt)
            : base(OrganizationMemberRemovalStatus.Removed)
        {
            OrganizationId = organizationId;
            MembershipId = membershipId;
            UserId = userId;
            SeatId = seatId;
            CorrelationId = correlationId;
            RemovedAt = removedAt;
        }

        public Guid OrganizationId { get; }

        public Guid MembershipId { get; }

        public Guid UserId { get; }

        public Guid SeatId { get; }

        public Guid CorrelationId { get; }

        public DateTimeOffset RemovedAt { get; }
    }

    public sealed class Rejection : OrganizationMemberRemovalResult
    {
        internal Rejection(OrganizationMemberRemovalStatus status)
            : base(status)
        {
            if (status == OrganizationMemberRemovalStatus.Removed)
            {
                throw new ArgumentException(
                    "A successful member removal cannot be represented as a rejection.",
                    nameof(status));
            }
        }
    }

    internal static Success Removed(
        Guid organizationId,
        Guid membershipId,
        Guid userId,
        Guid seatId,
        Guid correlationId,
        DateTimeOffset removedAt)
    {
        return new(
            organizationId,
            membershipId,
            userId,
            seatId,
            correlationId,
            removedAt);
    }

    internal static Rejection Rejected(OrganizationMemberRemovalStatus status)
    {
        return new(status);
    }
}
