namespace Styrhous.Licensing.Application.Organizations;

public abstract class OrganizationInvitationCancellationResult
{
    private OrganizationInvitationCancellationResult(
        OrganizationInvitationCancellationStatus status)
    {
        Status = status;
    }

    public OrganizationInvitationCancellationStatus Status { get; }

    public sealed class Success : OrganizationInvitationCancellationResult
    {
        internal Success(Guid invitationId, Guid correlationId, DateTimeOffset cancelledAt)
            : base(OrganizationInvitationCancellationStatus.Cancelled)
        {
            InvitationId = invitationId;
            CorrelationId = correlationId;
            CancelledAt = cancelledAt;
        }

        public Guid InvitationId { get; }

        public Guid CorrelationId { get; }

        public DateTimeOffset CancelledAt { get; }
    }

    public sealed class Rejection : OrganizationInvitationCancellationResult
    {
        internal Rejection(OrganizationInvitationCancellationStatus status)
            : base(status)
        {
            if (status == OrganizationInvitationCancellationStatus.Cancelled)
            {
                throw new ArgumentException(
                    "A successful cancellation cannot be represented as a rejection.",
                    nameof(status));
            }
        }
    }

    internal static Success Cancelled(
        Guid invitationId,
        Guid correlationId,
        DateTimeOffset cancelledAt)
    {
        return new(invitationId, correlationId, cancelledAt);
    }

    internal static Rejection Rejected(OrganizationInvitationCancellationStatus status)
    {
        return new(status);
    }
}
