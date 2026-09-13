using Styrhous.Licensing.Application.Organizations;

namespace Styrhous.Licensing.Api.Organizations;

public sealed record OrganizationListResponse(
    string ReasonCode,
    IReadOnlyList<OrganizationSummaryResponse> Organizations)
{
    internal static OrganizationListResponse From(
        IReadOnlyList<OrganizationSummary> organizations)
    {
        return new(
            OrganizationReasonCodes.OrganizationsListed,
            organizations.Select(OrganizationSummaryResponse.From).ToArray());
    }
}
