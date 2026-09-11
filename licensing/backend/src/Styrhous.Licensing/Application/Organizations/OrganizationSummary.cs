using Styrhous.Licensing.Domain.Organizations;

namespace Styrhous.Licensing.Application.Organizations;

public sealed record OrganizationSummary(
    Guid OrganizationId,
    Guid BillingAccountId,
    Guid MembershipId,
    Guid SeatId,
    string Name,
    OrganizationRole Role,
    bool ProductSeatAssigned,
    int DeviceLimit,
    DateTimeOffset JoinedAt);
