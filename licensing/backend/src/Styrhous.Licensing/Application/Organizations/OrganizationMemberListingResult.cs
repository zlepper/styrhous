namespace Styrhous.Licensing.Application.Organizations;

public abstract class OrganizationMemberListingResult
{
    private OrganizationMemberListingResult(OrganizationMemberListingStatus status)
    {
        Status = status;
    }

    public OrganizationMemberListingStatus Status { get; }

    public sealed class Success : OrganizationMemberListingResult
    {
        internal Success(
            Guid organizationId,
            IReadOnlyList<OrganizationMemberSummary> members)
            : base(OrganizationMemberListingStatus.Listed)
        {
            OrganizationId = organizationId;
            Members = members;
        }

        public Guid OrganizationId { get; }

        public IReadOnlyList<OrganizationMemberSummary> Members { get; }
    }

    public sealed class Rejection : OrganizationMemberListingResult
    {
        internal Rejection(OrganizationMemberListingStatus status)
            : base(status)
        {
            if (status == OrganizationMemberListingStatus.Listed)
            {
                throw new ArgumentException(
                    "A successful member listing cannot be represented as a rejection.",
                    nameof(status));
            }
        }
    }

    internal static Success Listed(
        Guid organizationId,
        IReadOnlyList<OrganizationMemberSummary> members)
    {
        return new(organizationId, members);
    }

    internal static Rejection Rejected(OrganizationMemberListingStatus status)
    {
        return new(status);
    }
}
