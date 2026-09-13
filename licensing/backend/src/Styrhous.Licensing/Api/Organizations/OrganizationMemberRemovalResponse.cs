using Styrhous.Licensing.Application.Organizations;

namespace Styrhous.Licensing.Api.Organizations;

public sealed record OrganizationMemberRemovalResponse(
    string ReasonCode,
    Guid OrganizationId,
    Guid MembershipId,
    Guid UserId,
    Guid SeatId,
    Guid CorrelationId,
    DateTimeOffset RemovedAt)
{
    internal static OrganizationMemberRemovalResponse From(
        OrganizationMemberRemovalResult.Success result)
    {
        return new(
            OrganizationReasonCodes.MemberRemoved,
            result.OrganizationId,
            result.MembershipId,
            result.UserId,
            result.SeatId,
            result.CorrelationId,
            result.RemovedAt);
    }
}
