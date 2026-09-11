using Styrhous.Licensing.Domain.Organizations;

namespace Styrhous.Licensing.Application.Organizations;

public sealed record OrganizationMemberSummary(
    Guid MembershipId,
    Guid UserId,
    Guid SeatId,
    string Email,
    OrganizationRole Role,
    bool ProductSeatAssigned,
    int DeviceLimit,
    DateTimeOffset JoinedAt);
