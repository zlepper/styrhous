using Styrhous.Licensing.Application.Organizations;

namespace Styrhous.Licensing.Api.Organizations;

public sealed record OrganizationMemberListResponse(
    string ReasonCode,
    Guid OrganizationId,
    IReadOnlyList<OrganizationMemberResponse> Members)
{
    internal static OrganizationMemberListResponse From(
        OrganizationMemberListingResult.Success success)
    {
        return new(
            OrganizationReasonCodes.MembersListed,
            success.OrganizationId,
            success.Members.Select(OrganizationMemberResponse.From).ToArray());
    }
}
