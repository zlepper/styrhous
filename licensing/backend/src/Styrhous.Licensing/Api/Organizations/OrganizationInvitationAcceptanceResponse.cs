using Styrhous.Licensing.Application.Organizations;

namespace Styrhous.Licensing.Api.Organizations;

public sealed record OrganizationInvitationAcceptanceResponse(
    string ReasonCode,
    Guid InvitationId,
    Guid OrganizationId,
    Guid MembershipId,
    Guid SeatId,
    bool ProductSeatAssigned,
    Guid CorrelationId,
    string Role,
    DateTimeOffset AcceptedAt)
{
    internal static OrganizationInvitationAcceptanceResponse From(
        OrganizationInvitationAcceptanceResult.Success success)
    {
        return new(
            OrganizationInvitationReasonCodes.Accepted,
            success.InvitationId,
            success.OrganizationId,
            success.MembershipId,
            success.SeatId,
            success.ProductSeatAssigned,
            success.CorrelationId,
            OrganizationRoleNames.ToApiValue(success.Role),
            success.AcceptedAt);
    }
}
