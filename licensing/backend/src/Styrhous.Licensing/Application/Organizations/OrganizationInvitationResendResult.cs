using Styrhous.Licensing.Application.Messaging;

namespace Styrhous.Licensing.Application.Organizations;

public abstract class OrganizationInvitationResendResult
{
    private OrganizationInvitationResendResult(OrganizationInvitationResendStatus status)
    {
        Status = status;
    }

    public OrganizationInvitationResendStatus Status { get; }

    public sealed class Success : OrganizationInvitationResendResult
    {
        internal Success(
            Guid invitationId,
            Guid correlationId,
            OrganizationInvitationSecret secret,
            DateTimeOffset expiresAt,
            BackgroundWorkReference backgroundWork)
            : base(OrganizationInvitationResendStatus.Resent)
        {
            InvitationId = invitationId;
            CorrelationId = correlationId;
            Secret = secret;
            ExpiresAt = expiresAt;
            BackgroundWork = backgroundWork;
        }

        public Guid InvitationId { get; }

        public Guid CorrelationId { get; }

        public OrganizationInvitationSecret Secret { get; }

        public DateTimeOffset ExpiresAt { get; }

        internal BackgroundWorkReference BackgroundWork { get; }
    }

    public sealed class Rejection : OrganizationInvitationResendResult
    {
        internal Rejection(OrganizationInvitationResendStatus status)
            : base(status)
        {
            if (status == OrganizationInvitationResendStatus.Resent)
            {
                throw new ArgumentException(
                    "A successful resend cannot be represented as a rejection.",
                    nameof(status));
            }
        }
    }

    internal static Success Resent(
        Guid invitationId,
        Guid correlationId,
        OrganizationInvitationSecret secret,
        DateTimeOffset expiresAt,
        BackgroundWorkReference backgroundWork)
    {
        return new(invitationId, correlationId, secret, expiresAt, backgroundWork);
    }

    internal static Rejection Rejected(OrganizationInvitationResendStatus status)
    {
        return new(status);
    }
}
