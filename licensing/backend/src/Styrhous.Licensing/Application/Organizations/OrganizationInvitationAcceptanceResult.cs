using Styrhous.Licensing.Domain.Organizations;

namespace Styrhous.Licensing.Application.Organizations;

public abstract class OrganizationInvitationAcceptanceResult
{
    private OrganizationInvitationAcceptanceResult(
        OrganizationInvitationAcceptanceStatus status)
    {
        Status = status;
    }

    public OrganizationInvitationAcceptanceStatus Status { get; }

    public sealed class Success : OrganizationInvitationAcceptanceResult
    {
        internal Success(
            OrganizationInvitationAcceptance acceptance,
            Guid correlationId)
            : base(OrganizationInvitationAcceptanceStatus.Accepted)
        {
            InvitationId = acceptance.Invitation.Id;
            OrganizationId = acceptance.Invitation.OrganizationId;
            MembershipId = acceptance.Membership.Id;
            SeatId = acceptance.Seat.Id;
            ProductSeatAssigned = acceptance.Seat.ProductAccessEnabled;
            CorrelationId = correlationId;
            Role = acceptance.Membership.Role;
            AcceptedAt = acceptance.ObservedAt;
        }

        public Guid InvitationId { get; }

        public Guid OrganizationId { get; }

        public Guid MembershipId { get; }

        public Guid SeatId { get; }

        public bool ProductSeatAssigned { get; }

        public Guid CorrelationId { get; }

        public OrganizationRole Role { get; }

        public DateTimeOffset AcceptedAt { get; }
    }

    public sealed class Rejection : OrganizationInvitationAcceptanceResult
    {
        internal Rejection(OrganizationInvitationAcceptanceStatus status)
            : base(status)
        {
            if (status == OrganizationInvitationAcceptanceStatus.Accepted)
            {
                throw new ArgumentException(
                    "A successful acceptance cannot be represented as a rejection.",
                    nameof(status));
            }
        }
    }

    internal static Success Accepted(
        OrganizationInvitationAcceptance acceptance,
        Guid correlationId)
    {
        return new(acceptance, correlationId);
    }

    internal static Rejection Rejected(OrganizationInvitationAcceptanceStatus status)
    {
        return new(status);
    }
}
