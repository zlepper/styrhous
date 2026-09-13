using Styrhous.Licensing.Application.Organizations;

namespace Styrhous.Licensing.Api.Organizations;

public sealed record OrganizationInvitationListResponse(
    string ReasonCode,
    Guid OrganizationId,
    IReadOnlyList<OrganizationInvitationSummaryResponse> Invitations)
{
    internal static OrganizationInvitationListResponse From(
        OrganizationInvitationListingResult.Success success)
    {
        return new(
            OrganizationInvitationReasonCodes.Listed,
            success.OrganizationId,
            success.Invitations
                .Select(OrganizationInvitationSummaryResponse.From)
                .ToArray());
    }
}
