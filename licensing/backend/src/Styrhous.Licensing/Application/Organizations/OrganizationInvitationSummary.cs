using Styrhous.Licensing.Domain.Organizations;

namespace Styrhous.Licensing.Application.Organizations;

public sealed record OrganizationInvitationSummary(
    Guid InvitationId,
    Guid CreatedByUserId,
    string Email,
    OrganizationRole Role,
    bool AssignProductSeat,
    DateTimeOffset CreatedAt,
    DateTimeOffset LastSentAt,
    DateTimeOffset ExpiresAt);
