using System.Net;
using Styrhous.Licensing.Tests.Persistence;
using static Styrhous.Licensing.Tests.Persistence.LicensingPersistenceScenario;

namespace Styrhous.Licensing.Tests.Api;

[TestFixture]
[Parallelizable(ParallelScope.All)]
public sealed class PortalHostingApiTests
{
    [Test]
    public async Task MonolithServesPortalRoutesButDoesNotRewriteBackendMisses()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var webRoot = Directory.CreateTempSubdirectory();
        try
        {
            const string portal = "<!doctype html><title>Styrhous licensing</title>";
            await File.WriteAllTextAsync(Path.Combine(webRoot.FullName, "index.html"), portal);
            using var factory = new LicensingWebApplicationFactory(
                database,
                SignupTime,
                webRootPath: webRoot.FullName);
            using var client = factory.CreateApiClient();

            using var portalResponse = await client.GetAsync("/billing");
            using var backendResponse = await client.GetAsync("/api/does-not-exist");
            var portalContent = await portalResponse.Content.ReadAsStringAsync();
            var backendContent = await backendResponse.Content.ReadAsStringAsync();

            Assert.Multiple(() =>
            {
                Assert.That(portalResponse.StatusCode, Is.EqualTo(HttpStatusCode.OK));
                Assert.That(portalContent, Is.EqualTo(portal));
                Assert.That(
                    portalResponse.Headers.GetValues("Content-Security-Policy"),
                    Is.EquivalentTo(["frame-ancestors 'none'"]));
                Assert.That(backendResponse.StatusCode, Is.EqualTo(HttpStatusCode.NotFound));
                Assert.That(
                    backendResponse.Headers.GetValues("Cache-Control"),
                    Is.EquivalentTo(["no-store"]));
                Assert.That(
                    backendContent,
                    Does.Not.Contain("Styrhous licensing"));
            });
        }
        finally
        {
            webRoot.Delete(recursive: true);
        }
    }
}
