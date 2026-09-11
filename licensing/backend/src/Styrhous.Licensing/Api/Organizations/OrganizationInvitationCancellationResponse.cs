using Styrhous.Licensing.Application.Organizations;

namespace Styrhous.Licensing.Api.Organizations;

public sealed record OrganizationInvitationCancellationResponse(
    string ReasonCode,
    Guid InvitationId,
    Guid CorrelationId,
    DateTimeOffset CancelledAt)
{
    internal static OrganizationInvitationCancellationResponse From(
        OrganizationInvitationCancellationResult.Success result)
    {
        return new(
            OrganizationInvitationReasonCodes.Cancelled,
            result.InvitationId,
            result.CorrelationId,
            result.CancelledAt);
    }
}
