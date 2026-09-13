using Styrhous.Licensing.Application.Organizations;

namespace Styrhous.Licensing.Api.Organizations;

public sealed record OrganizationInvitationResendResponse(
    string ReasonCode,
    Guid InvitationId,
    Guid CorrelationId,
    DateTimeOffset ExpiresAt)
{
    internal static OrganizationInvitationResendResponse From(
        OrganizationInvitationResendResult.Success result)
    {
        return new(
            OrganizationInvitationReasonCodes.Resent,
            result.InvitationId,
            result.CorrelationId,
            result.ExpiresAt);
    }
}
