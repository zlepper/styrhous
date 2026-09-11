using Styrhous.Licensing.Application.Messaging;
using Styrhous.Licensing.Domain.Organizations;

namespace Styrhous.Licensing.Application.Organizations;

public abstract class OrganizationInvitationCreationResult
{
    private OrganizationInvitationCreationResult(
        OrganizationInvitationCreationStatus status)
    {
        Status = status;
    }

    public OrganizationInvitationCreationStatus Status { get; }

    public sealed class Success : OrganizationInvitationCreationResult
    {
        internal Success(
            OrganizationInvitation invitation,
            Guid correlationId,
            OrganizationInvitationSecret secret,
            BackgroundWorkReference backgroundWork)
            : base(OrganizationInvitationCreationStatus.Created)
        {
            InvitationId = invitation.Id;
            CorrelationId = correlationId;
            Secret = secret;
            ExpiresAt = invitation.ExpiresAt;
            BackgroundWork = backgroundWork;
        }

        public Guid InvitationId { get; }

        public Guid CorrelationId { get; }

        public OrganizationInvitationSecret Secret { get; }

        public DateTimeOffset ExpiresAt { get; }

        internal BackgroundWorkReference BackgroundWork { get; }
    }

    public sealed class Rejection : OrganizationInvitationCreationResult
    {
        internal Rejection(OrganizationInvitationCreationStatus status)
            : base(status)
        {
            if (status == OrganizationInvitationCreationStatus.Created)
            {
                throw new ArgumentException(
                    "A successful invitation cannot be represented as a rejection.",
                    nameof(status));
            }
        }
    }

    internal static Success Created(
        OrganizationInvitation invitation,
        Guid correlationId,
        OrganizationInvitationSecret secret,
        BackgroundWorkReference backgroundWork)
    {
        return new(invitation, correlationId, secret, backgroundWork);
    }

    internal static Rejection Rejected(OrganizationInvitationCreationStatus status)
    {
        return new(status);
    }
}
