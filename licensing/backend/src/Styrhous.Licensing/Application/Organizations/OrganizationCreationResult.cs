using Styrhous.Licensing.Domain.Organizations;

namespace Styrhous.Licensing.Application.Organizations;

public sealed record OrganizationCreationResult(
    Guid UserId,
    Guid OrganizationId,
    Guid BillingAccountId,
    Guid OwnerMembershipId,
    Guid SeatId,
    Guid CorrelationId,
    bool TrialWasTransferred)
{
    public static OrganizationCreationResult Created(
        OrganizationRegistration registration,
        Guid correlationId,
        bool trialWasTransferred)
    {
        return new(
            registration.OwnerMembership.UserId,
            registration.Organization.Id,
            registration.BillingAccount.Id,
            registration.OwnerMembership.Id,
            registration.Seat.Id,
            correlationId,
            trialWasTransferred);
    }
}
