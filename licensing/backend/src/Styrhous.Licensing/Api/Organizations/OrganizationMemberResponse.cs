using Styrhous.Licensing.Application.Organizations;

namespace Styrhous.Licensing.Api.Organizations;

public sealed record OrganizationMemberResponse(
    Guid MembershipId,
    Guid UserId,
    Guid SeatId,
    string Email,
    string Role,
    bool ProductSeatAssigned,
    int DeviceLimit,
    DateTimeOffset JoinedAt)
{
    internal static OrganizationMemberResponse From(OrganizationMemberSummary member)
    {
        return new(
            member.MembershipId,
            member.UserId,
            member.SeatId,
            member.Email,
            OrganizationRoleNames.ToApiValue(member.Role),
            member.ProductSeatAssigned,
            member.DeviceLimit,
            member.JoinedAt);
    }
}
