using Styrhous.Licensing.Application.Messaging;
using Styrhous.Licensing.Domain.Organizations;

namespace Styrhous.Licensing.Application.Organizations;



public abstract class OrganizationInvitationCreationStoreResult
{
    private OrganizationInvitationCreationStoreResult(
        OrganizationInvitationCreationStatus status)
    {
        Status = status;
    }

    public OrganizationInvitationCreationStatus Status { get; }

    public sealed class Success(BackgroundWorkReference backgroundWork)
        : OrganizationInvitationCreationStoreResult(
            OrganizationInvitationCreationStatus.Created)
    {
        public BackgroundWorkReference BackgroundWork { get; } =
            backgroundWork ?? throw new ArgumentNullException(nameof(backgroundWork));
    }

    public sealed class Rejection : OrganizationInvitationCreationStoreResult
    {
        private Rejection(OrganizationInvitationCreationStatus status) : base(status)
        {
            if (status == OrganizationInvitationCreationStatus.Created)
            {
                throw new ArgumentException(
                    "A created invitation cannot be represented as a rejection.",
                    nameof(status));
            }
        }

        internal static Rejection From(OrganizationInvitationCreationStatus status)
        {
            return new(status);
        }
    }
}
