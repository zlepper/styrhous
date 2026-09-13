using Styrhous.Licensing.Application.Organizations;

namespace Styrhous.Licensing.Api.Organizations;

public sealed record OrganizationMemberRoleChangeResponse(
    string ReasonCode,
    Guid OrganizationId,
    Guid MembershipId,
    Guid UserId,
    string PreviousRole,
    string Role,
    Guid CorrelationId,
    DateTimeOffset ChangedAt)
{
    internal static OrganizationMemberRoleChangeResponse From(
        OrganizationRoleManagementResult.RoleChanged result)
    {
        return new(
            OrganizationReasonCodes.MemberRoleChanged,
            result.OrganizationId,
            result.MembershipId,
            result.UserId,
            OrganizationRoleNames.ToApiValue(result.PreviousRole),
            OrganizationRoleNames.ToApiValue(result.Role),
            result.CorrelationId,
            result.ChangedAt);
    }
}
