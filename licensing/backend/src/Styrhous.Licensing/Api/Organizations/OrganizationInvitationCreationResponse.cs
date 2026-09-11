using Styrhous.Licensing.Application.Organizations;

namespace Styrhous.Licensing.Api.Organizations;

public sealed record OrganizationInvitationCreationResponse(
    string ReasonCode,
    Guid InvitationId,
    Guid CorrelationId,
    DateTimeOffset ExpiresAt)
{
    internal static OrganizationInvitationCreationResponse From(
        OrganizationInvitationCreationResult.Success result)
    {
        return new(
            OrganizationInvitationReasonCodes.Created,
            result.InvitationId,
            result.CorrelationId,
            result.ExpiresAt);
    }
}
