using Styrhous.Licensing.Application.Organizations;

namespace Styrhous.Licensing.Api.Organizations;

public sealed record OrganizationCreationResponse(
    Guid UserId,
    Guid OrganizationId,
    Guid BillingAccountId,
    Guid OwnerMembershipId,
    Guid SeatId,
    Guid CorrelationId,
    bool TrialWasTransferred)
{
    internal static OrganizationCreationResponse From(OrganizationCreationResult result)
    {
        return new(
            result.UserId,
            result.OrganizationId,
            result.BillingAccountId,
            result.OwnerMembershipId,
            result.SeatId,
            result.CorrelationId,
            result.TrialWasTransferred);
    }
}
