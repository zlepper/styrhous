using System.Net;
using System.Text.Json;
using Microsoft.AspNetCore.Authorization;
using Microsoft.AspNetCore.Authorization.Policy;
using Microsoft.AspNetCore.Http;
using Styrhous.Licensing.Api.Desktop;
using Styrhous.Licensing.Tests.Persistence;

namespace Styrhous.Licensing.Tests.Api;

[TestFixture]
[Parallelizable(ParallelScope.All)]
public sealed class DesktopAuthorizationResultHandlerTests
{
    [Test]
    public async Task MissingDesktopScopeReturnsTheStableForbiddenBody()
    {
        var context = new DefaultHttpContext();
        context.SetEndpoint(new Endpoint(
            _ => Task.CompletedTask,
            new EndpointMetadataCollection(DesktopAccessTokenMetadata.Instance),
            "protected desktop endpoint"));
        context.Response.Body = new MemoryStream();
        var nextCalled = false;
        var handler = new DesktopAuthorizationResultHandler();

        await handler.HandleAsync(
            _ =>
            {
                nextCalled = true;
                return Task.CompletedTask;
            },
            context,
            new AuthorizationPolicyBuilder().RequireAuthenticatedUser().Build(),
            PolicyAuthorizationResult.Forbid());

        context.Response.Body.Position = 0;
        using var body = await JsonDocument.ParseAsync(context.Response.Body);
        Assert.Multiple(() =>
        {
            Assert.That(nextCalled, Is.False);
            Assert.That(context.Response.StatusCode, Is.EqualTo((int)HttpStatusCode.Forbidden));
            Assert.That(context.Response.ContentType, Does.StartWith("application/json"));
            Assert.That(
                body.RootElement.GetProperty("reasonCode").GetString(),
                Is.EqualTo("insufficient_scope"));
        });
    }

    [TestCase("POST", "/desktop/v1/entitlement")]
    [TestCase("GET", "/desktop/v1/devices")]
    [TestCase("DELETE", "/desktop/v1/devices/01999999-0000-7000-8000-000000000001")]
    public async Task ProtectedDesktopRoutesRejectAnAuthenticatedPrincipalWithoutScope(
        string method,
        string path)
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        using var factory = new LicensingWebApplicationFactory(
            database,
            DateTimeOffset.UtcNow,
            useScopeLessDesktopAuthentication: true);
        using var client = factory.CreateApiClient();
        using var request = new HttpRequestMessage(new HttpMethod(method), path);

        using var response = await client.SendAsync(request);
        using var body = JsonDocument.Parse(await response.Content.ReadAsStringAsync());

        Assert.Multiple(() =>
        {
            Assert.That(response.StatusCode, Is.EqualTo(HttpStatusCode.Forbidden));
            Assert.That(
                body.RootElement.GetProperty("reasonCode").GetString(),
                Is.EqualTo("insufficient_scope"));
        });
    }
}
