using Styrhous.Licensing.Application.Signups;
using Styrhous.Licensing.Domain.Signups;

namespace Styrhous.Licensing.Tests.Persistence;

internal static class LicensingPersistenceScenario
{
    public static readonly DateTimeOffset SignupTime =
        new(2026, 8, 30, 10, 15, 0, TimeSpan.Zero);

    public static async Task<SignupResult> SignUpAsync(
        PostgresTestDatabase database,
        string subject = "licensing-user",
        string email = "person@example.com")
    {
        await using var test = ServiceTestBase<UserSignupService>.ForDatabase(database, SignupTime);
        return await test.Service
            .SignUpAsync(VerifiedExternalIdentity.Create("github", subject, email));
    }
}
