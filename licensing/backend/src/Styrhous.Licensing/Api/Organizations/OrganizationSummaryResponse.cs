using Styrhous.Licensing.Application.Organizations;
using Styrhous.Licensing.Domain.Organizations;

namespace Styrhous.Licensing.Api.Organizations;

public sealed record OrganizationSummaryResponse(
    Guid OrganizationId,
    Guid BillingAccountId,
    Guid MembershipId,
    Guid SeatId,
    string Name,
    string Role,
    bool ProductSeatAssigned,
    int DeviceLimit,
    DateTimeOffset JoinedAt)
{
    internal static OrganizationSummaryResponse From(OrganizationSummary organization)
    {
        return new(
            organization.OrganizationId,
            organization.BillingAccountId,
            organization.MembershipId,
            organization.SeatId,
            organization.Name,
            OrganizationRoleNames.ToApiValue(organization.Role),
            organization.ProductSeatAssigned,
            organization.DeviceLimit,
            organization.JoinedAt);
    }
}
