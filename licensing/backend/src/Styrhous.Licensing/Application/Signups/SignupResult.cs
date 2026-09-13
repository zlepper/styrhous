using Styrhous.Licensing.Domain.Signups;

namespace Styrhous.Licensing.Application.Signups;

public sealed record SignupResult(
    Guid UserId,
    Guid PersonalBillingAccountId,
    Guid SeatId,
    Guid TrialId,
    DateTimeOffset TrialStartedAt,
    DateTimeOffset TrialEndsAt,
    bool WasCreated)
{
    public static SignupResult Created(SignupRegistration registration)
    {
        return new(
            registration.User.Id,
            registration.BillingAccount.Id,
            registration.Seat.Id,
            registration.Trial.Id,
            registration.Trial.StartedAt,
            registration.Trial.EndsAt,
            true);
    }
}
