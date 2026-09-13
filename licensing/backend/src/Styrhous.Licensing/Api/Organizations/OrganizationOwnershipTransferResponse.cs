using Styrhous.Licensing.Application.Organizations;

namespace Styrhous.Licensing.Api.Organizations;

public sealed record OrganizationOwnershipTransferResponse(
    string ReasonCode,
    Guid OrganizationId,
    Guid PreviousOwnerMembershipId,
    Guid PreviousOwnerUserId,
    Guid OwnerMembershipId,
    Guid OwnerUserId,
    Guid CorrelationId,
    DateTimeOffset TransferredAt)
{
    internal static OrganizationOwnershipTransferResponse From(
        OrganizationRoleManagementResult.OwnershipTransferred result)
    {
        return new(
            OrganizationReasonCodes.OwnershipTransferred,
            result.OrganizationId,
            result.PreviousOwnerMembershipId,
            result.PreviousOwnerUserId,
            result.OwnerMembershipId,
            result.OwnerUserId,
            result.CorrelationId,
            result.TransferredAt);
    }
}
