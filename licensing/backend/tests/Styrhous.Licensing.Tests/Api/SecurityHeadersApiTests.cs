using System.Net;
using Styrhous.Licensing.Tests.Persistence;
using static Styrhous.Licensing.Tests.Persistence.LicensingPersistenceScenario;

namespace Styrhous.Licensing.Tests.Api;

[TestFixture]
[Parallelizable(ParallelScope.All)]
public sealed class SecurityHeadersApiTests
{
    [Test]
    public async Task HealthIncludesTheMonolithSecurityHeaders()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        using var factory = new LicensingWebApplicationFactory(database, SignupTime);
        using var client = factory.CreateApiClient();

        using var response = await client.GetAsync("/health");

        Assert.Multiple(() =>
        {
            Assert.That(response.StatusCode, Is.EqualTo(HttpStatusCode.OK));
            Assert.That(
                response.Headers.GetValues("Content-Security-Policy"),
                Is.EquivalentTo(["frame-ancestors 'none'"]));
            Assert.That(
                response.Headers.GetValues("Referrer-Policy"),
                Is.EquivalentTo(["strict-origin-when-cross-origin"]));
            Assert.That(
                response.Headers.GetValues("X-Content-Type-Options"),
                Is.EquivalentTo(["nosniff"]));
            Assert.That(
                response.Headers.GetValues("X-Frame-Options"),
                Is.EquivalentTo(["DENY"]));
        });
    }
}
