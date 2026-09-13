using Styrhous.Licensing.Application.Messaging;

namespace Styrhous.Licensing.Application.Organizations;



public abstract class OrganizationInvitationResendStoreResult
{
    private OrganizationInvitationResendStoreResult(
        OrganizationInvitationResendStatus status)
    {
        Status = status;
    }

    public OrganizationInvitationResendStatus Status { get; }

    public sealed class Success(BackgroundWorkReference backgroundWork)
        : OrganizationInvitationResendStoreResult(
            OrganizationInvitationResendStatus.Resent)
    {
        public BackgroundWorkReference BackgroundWork { get; } =
            backgroundWork ?? throw new ArgumentNullException(nameof(backgroundWork));
    }

    public sealed class Rejection : OrganizationInvitationResendStoreResult
    {
        private Rejection(OrganizationInvitationResendStatus status) : base(status)
        {
            if (status == OrganizationInvitationResendStatus.Resent)
            {
                throw new ArgumentException(
                    "A resent invitation cannot be represented as a rejection.",
                    nameof(status));
            }
        }

        internal static Rejection From(OrganizationInvitationResendStatus status)
        {
            return new(status);
        }
    }
}
