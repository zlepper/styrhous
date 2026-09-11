using Styrhous.Licensing.Application.Organizations;

namespace Styrhous.Licensing.Api.Organizations;

public sealed record OrganizationSeatAssignmentResponse(
    string ReasonCode,
    Guid OrganizationId,
    Guid MembershipId,
    Guid UserId,
    Guid SeatId,
    bool Assigned,
    int DeviceLimit,
    Guid CorrelationId,
    DateTimeOffset ChangedAt)
{
    internal static OrganizationSeatAssignmentResponse From(
        OrganizationSeatAssignmentResult.Success result)
    {
        return new(
            result.Assigned
                ? OrganizationReasonCodes.MemberSeatAssigned
                : OrganizationReasonCodes.MemberSeatUnassigned,
            result.OrganizationId,
            result.MembershipId,
            result.UserId,
            result.SeatId,
            result.Assigned,
            result.DeviceLimit,
            result.CorrelationId,
            result.ChangedAt);
    }
}
