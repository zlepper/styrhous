using Styrhous.Licensing.Application.Organizations;

namespace Styrhous.Licensing.Api.Organizations;

public sealed record OrganizationInvitationSummaryResponse(
    Guid InvitationId,
    Guid CreatedByUserId,
    string Email,
    string Role,
    bool AssignProductSeat,
    DateTimeOffset CreatedAt,
    DateTimeOffset LastSentAt,
    DateTimeOffset ExpiresAt)
{
    internal static OrganizationInvitationSummaryResponse From(
        OrganizationInvitationSummary invitation)
    {
        return new(
            invitation.InvitationId,
            invitation.CreatedByUserId,
            invitation.Email,
            OrganizationRoleNames.ToApiValue(invitation.Role),
            invitation.AssignProductSeat,
            invitation.CreatedAt,
            invitation.LastSentAt,
            invitation.ExpiresAt);
    }
}
