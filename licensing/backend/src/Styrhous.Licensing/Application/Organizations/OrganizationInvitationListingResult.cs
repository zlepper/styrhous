namespace Styrhous.Licensing.Application.Organizations;

public abstract class OrganizationInvitationListingResult
{
    private OrganizationInvitationListingResult(OrganizationInvitationListingStatus status)
    {
        Status = status;
    }

    public OrganizationInvitationListingStatus Status { get; }

    public sealed class Success : OrganizationInvitationListingResult
    {
        internal Success(
            Guid organizationId,
            IReadOnlyList<OrganizationInvitationSummary> invitations)
            : base(OrganizationInvitationListingStatus.Listed)
        {
            OrganizationId = organizationId;
            Invitations = invitations;
        }

        public Guid OrganizationId { get; }

        public IReadOnlyList<OrganizationInvitationSummary> Invitations { get; }
    }

    public sealed class Rejection : OrganizationInvitationListingResult
    {
        internal Rejection(OrganizationInvitationListingStatus status)
            : base(status)
        {
            if (status == OrganizationInvitationListingStatus.Listed)
            {
                throw new ArgumentException(
                    "A successful invitation listing cannot be represented as a rejection.",
                    nameof(status));
            }
        }
    }

    internal static Success Listed(
        Guid organizationId,
        IReadOnlyList<OrganizationInvitationSummary> invitations)
    {
        return new(organizationId, invitations);
    }

    internal static Rejection Rejected(OrganizationInvitationListingStatus status)
    {
        return new(status);
    }
}
