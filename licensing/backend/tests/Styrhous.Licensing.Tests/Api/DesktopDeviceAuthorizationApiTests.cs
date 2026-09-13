using System.Globalization;
using System.Net;
using System.Net.Http.Json;
using System.Security.Cryptography;
using System.Security.Cryptography.X509Certificates;
using System.Security.Claims;
using System.Text;
using System.Text.Json;
using Microsoft.EntityFrameworkCore;
using Microsoft.EntityFrameworkCore.Diagnostics;
using Microsoft.IdentityModel.JsonWebTokens;
using Microsoft.IdentityModel.Tokens;
using OpenIddict.Abstractions;
using OpenIddict.EntityFrameworkCore.Models;
using Styrhous.Licensing.Api.Desktop;
using Styrhous.Licensing.Api.Devices;
using Styrhous.Licensing.Application.Desktop;
using Styrhous.Licensing.Application.Billing;
using Styrhous.Licensing.Application.Entitlements;
using Styrhous.Licensing.Domain.Billing;
using Styrhous.Licensing.Domain.Devices;
using Styrhous.Licensing.Domain.Organizations;
using Styrhous.Licensing.Persistence;
using Styrhous.Licensing.Tests.Infrastructure;
using Styrhous.Licensing.Tests.Persistence;
using static Styrhous.Licensing.Tests.Persistence.DevicePersistenceScenario;
using static Styrhous.Licensing.Tests.Persistence.LicensingPersistenceScenario;

namespace Styrhous.Licensing.Tests.Api;

[TestFixture]
[Parallelizable(ParallelScope.All)]
public sealed class DesktopDeviceAuthorizationApiTests
{
    private const string ClientId = "styrhous-desktop";

    private const string ApprovedOpenApiSha256 =
        "8C912E128FC4F6E4B08448AE377707784F5598FAE344AB822D9C0DC80AE5592B";

    [Test]
    public async Task PublishedOpenApiContainsOnlyTheApprovedDesktopV1Contract()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        using var factory = new LicensingWebApplicationFactory(database, DateTimeOffset.UtcNow);
        using var client = factory.CreateApiClient();

        using var response = await client.GetAsync("/desktop/v1/openapi.json");
        var responseBody = await response.Content.ReadAsStringAsync();
        using var document = JsonDocument.Parse(responseBody);
        var paths = document.RootElement.GetProperty("paths")
            .EnumerateObject()
            .Select(path => path.Name)
            .ToArray();

        Assert.Multiple(() =>
        {
            Assert.That(response.StatusCode, Is.EqualTo(HttpStatusCode.OK));
            Assert.That(response.Headers.CacheControl?.NoStore, Is.True);
            Assert.That(response.Content.Headers.ContentType?.MediaType,
                Is.EqualTo("application/json"));
            Assert.That(document.RootElement.GetProperty("openapi").GetString(),
                Is.EqualTo("3.1.0"));
            Assert.That(paths, Is.Not.Empty);
            Assert.That(paths, Has.All.StartsWith("/desktop/v1/"));
            Assert.That(
                Convert.ToHexString(SHA256.HashData(Encoding.UTF8.GetBytes(responseBody))),
                Is.EqualTo(ApprovedOpenApiSha256),
                "A desktop v1 contract change requires explicit compatibility review.");
        });
    }

    [TestCase("POST", DesktopProtocolConstants.EntitlementPath)]
    [TestCase("GET", DesktopProtocolConstants.DevicesPath)]
    [TestCase("DELETE", DesktopProtocolConstants.DevicesPath + "/01999999-0000-7000-8000-000000000001")]
    public async Task ProtectedDesktopRoutesRejectNonDesktopCredentials(
        string method,
        string path)
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(
            database,
            $"protected-route-{method.ToLowerInvariant()}-{Guid.NewGuid():N}",
            $"protected-route-{Guid.NewGuid():N}@example.com");
        using var factory = new LicensingWebApplicationFactory(database, SignupTime);
        using var anonymousClient = factory.CreateApiClient();
        using var invalidTokenClient = factory.CreateApiClient();
        invalidTokenClient.DefaultRequestHeaders.Authorization =
            new System.Net.Http.Headers.AuthenticationHeaderValue(
                "Bearer",
                "not-a-desktop-access-token");
        using var browserSessionClient = factory.CreateApiClient(signup.UserId);

        using var missing = await anonymousClient.SendAsync(
            new HttpRequestMessage(new HttpMethod(method), path));
        using var invalid = await invalidTokenClient.SendAsync(
            new HttpRequestMessage(new HttpMethod(method), path));
        using var browserSession = await browserSessionClient.SendAsync(
            new HttpRequestMessage(new HttpMethod(method), path));
        var responses = new[] { missing, invalid, browserSession };
        var errors = new List<DesktopProtocolErrorResponse?>();
        foreach (var response in responses)
        {
            errors.Add(await response.Content.ReadFromJsonAsync<DesktopProtocolErrorResponse>());
        }

        Assert.Multiple(() =>
        {
            Assert.That(responses, Has.All.Property("StatusCode")
                .EqualTo(HttpStatusCode.Unauthorized));
            Assert.That(
                responses.Select(response => response.Content.Headers.ContentType?.MediaType),
                Has.All.EqualTo("application/json"));
            Assert.That(
                errors.Select(error => error?.ReasonCode),
                Has.All.EqualTo("invalid_access_token"));
            Assert.That(
                responses.Select(response => response.Headers.WwwAuthenticate.Count),
                Has.All.EqualTo(1));
        });
    }

    [TestCase(null, "/devices/authorize")]
    [TestCase("AB CD+/", "/devices/authorize?user_code=AB%20CD%2B%2F")]
    public async Task VerificationLinkRedirectsOnlyToTheStaticApprovalRoute(
        string? userCode,
        string expectedLocation)
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        using var factory = new LicensingWebApplicationFactory(database, DateTimeOffset.UtcNow);
        using var client = factory.CreateApiClient();
        var path = userCode is null
            ? "/desktop/v1/device/verify"
            : $"/desktop/v1/device/verify?user_code={Uri.EscapeDataString(userCode)}";

        using var response = await client.GetAsync(path);

        Assert.Multiple(() =>
        {
            Assert.That(response.StatusCode, Is.EqualTo(HttpStatusCode.Found));
            Assert.That(response.Headers.Location?.OriginalString, Is.EqualTo(expectedLocation));
            Assert.That(response.Headers.CacheControl?.NoStore, Is.True);
        });
    }

    [Test]
    public async Task RequestLogsNeverContainTheDesktopUserCodeQueryValue()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(
            database,
            "desktop-log-redaction",
            "desktop-log-redaction@example.com");
        var logs = new TestLogCollector();
        using var factory = new LicensingWebApplicationFactory(
            database,
            SignupTime.AddDays(1),
            logCollector: logs);
        using var client = factory.CreateApiClient(signup.UserId);
        var authorization = await StartAuthorizationAsync(client, Guid.CreateVersion7());
        logs.Clear();

        using var response = await client.GetAsync(
            $"/desktop/v1/device/approval?user_code="
                + Uri.EscapeDataString(authorization.UserCode));

        Assert.Multiple(() =>
        {
            Assert.That(response.StatusCode, Is.EqualTo(HttpStatusCode.OK));
            Assert.That(
                logs.Messages,
                Has.Some.Contains("/desktop/v1/device/approval"));
            Assert.That(
                logs.Messages,
                Has.None.Contains(authorization.UserCode));
        });
    }

    [TestCase(DesktopProtocolConstants.TokenPath)]
    [TestCase(DesktopProtocolConstants.DeviceAuthorizationPath)]
    public async Task OversizedDesktopProtocolFormIsRejectedBeforeProcessing(string path)
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        using var factory = new LicensingWebApplicationFactory(
            database,
            SignupTime.AddDays(1));
        using var client = factory.CreateApiClient();

        using var response = await client.PostAsync(
            path,
            new FormUrlEncodedContent(
            [
                new("client_id", ClientId),
                new("grant_type", "refresh_token"),
                new("refresh_token", new string('x', 17 * 1024)),
            ]));

        Assert.That(
            response.StatusCode,
            Is.EqualTo(HttpStatusCode.RequestEntityTooLarge));
    }

    [TestCase(DesktopProtocolConstants.TokenPath)]
    [TestCase(DesktopProtocolConstants.DeviceAuthorizationPath)]
    public async Task OversizedUnknownLengthDesktopProtocolFormIsRejectedBeforeProcessing(
        string path)
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        using var factory = new LicensingWebApplicationFactory(
            database,
            SignupTime.AddDays(1));
        using var client = factory.CreateApiClient();
        using var content = new UnknownLengthFormContent(
            "client_id=" + ClientId
            + "&grant_type=refresh_token&refresh_token="
            + new string('x', 17 * 1024));

        using var response = await client.PostAsync(path, content);

        Assert.That(
            response.StatusCode,
            Is.EqualTo(HttpStatusCode.RequestEntityTooLarge));
    }

    [Test]
    public async Task FormWithTooManyValuesReturnsInvalidRequest()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        using var factory = new LicensingWebApplicationFactory(
            database,
            SignupTime.AddDays(1));
        using var client = factory.CreateApiClient();
        using var content = new FormUrlEncodedContent(
            Enumerable.Range(0, 1025)
                .Select(index => new KeyValuePair<string, string>($"k{index}", "x")));

        using var response = await client.PostAsync("/desktop/v1/token", content);
        using var body = JsonDocument.Parse(await response.Content.ReadAsStringAsync());

        Assert.Multiple(() =>
        {
            Assert.That(response.StatusCode, Is.EqualTo(HttpStatusCode.BadRequest));
            Assert.That(
                body.RootElement.GetProperty("error").GetString(),
                Is.EqualTo("invalid_request"));
        });
    }

    [Test]
    public async Task ValidInstallationReceivesVersionedTenMinuteDeviceAuthorization()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        using var factory = new LicensingWebApplicationFactory(database, DateTimeOffset.UtcNow);
        using var client = factory.CreateApiClient();

        using var response = await client.PostAsync(
            "/desktop/v1/device/authorize",
            DeviceAuthorizationForm(Guid.CreateVersion7()));
        var responseBody = await response.Content.ReadAsStringAsync();
        using var body = JsonDocument.Parse(responseBody);

        Assert.Multiple(() =>
        {
            Assert.That(response.StatusCode, Is.EqualTo(HttpStatusCode.OK));
            Assert.That(response.Headers.CacheControl?.NoStore, Is.True);
            Assert.That(body.RootElement.GetProperty("device_code").GetString(), Is.Not.Empty);
            Assert.That(body.RootElement.GetProperty("user_code").GetString(), Is.Not.Empty);
            Assert.That(
                body.RootElement.GetProperty("verification_uri").GetString(),
                Is.EqualTo("https://localhost/desktop/v1/device/verify"));
            Assert.That(
                body.RootElement.GetProperty("verification_uri_complete").GetString(),
                Does.StartWith("https://localhost/desktop/v1/device/verify?user_code="));
            Assert.That(body.RootElement.GetProperty("expires_in").GetInt32(), Is.EqualTo(600));
            Assert.That(body.RootElement.GetProperty("interval").GetInt32(), Is.EqualTo(5));
            Assert.That(
                PropertyNames(body.RootElement),
                Is.EquivalentTo(
                [
                    "device_code",
                    "expires_in",
                    "interval",
                    "user_code",
                    "verification_uri",
                    "verification_uri_complete",
                ]));
        });
    }

    [Test]
    public async Task CanonicalIssuerControlsVerificationLinksAcrossAnInternalTransportHost()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        using var factory = new LicensingWebApplicationFactory(
            database,
            DateTimeOffset.UtcNow,
            desktopIssuer: "https://licenses.example.com/");
        using var client = factory.CreateApiClient();
        client.DefaultRequestHeaders.Host = "internal-api.local";

        using var response = await client.PostAsync(
            "/desktop/v1/device/authorize",
            DeviceAuthorizationForm(Guid.CreateVersion7()));
        using var body = JsonDocument.Parse(await response.Content.ReadAsStringAsync());

        Assert.Multiple(() =>
        {
            Assert.That(response.StatusCode, Is.EqualTo(HttpStatusCode.OK));
            Assert.That(
                body.RootElement.GetProperty("verification_uri").GetString(),
                Is.EqualTo("https://licenses.example.com/desktop/v1/device/verify"));
            Assert.That(
                body.RootElement.GetProperty("verification_uri_complete").GetString(),
                Does.StartWith(
                    "https://licenses.example.com/desktop/v1/device/verify?user_code="));
        });
    }

    [TestCase("4f06ed53-3d60-4f91-91d9-735e5b75c8d2")]
    [TestCase(TestIdentifiers.Version7WithNonRfcVariantText)]
    public async Task ExistingInstallationIdentifierFormatsCanStartAuthorization(string installationId)
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        using var factory = new LicensingWebApplicationFactory(database, DateTimeOffset.UtcNow);
        using var client = factory.CreateApiClient();
        using var response = await client.PostAsync(
            "/desktop/v1/device/authorize", DeviceAuthorizationForm(installationId));
        using var body = JsonDocument.Parse(await response.Content.ReadAsStringAsync());
        Assert.Multiple(() =>
        {
            Assert.That(response.StatusCode, Is.EqualTo(HttpStatusCode.OK));
            Assert.That(body.RootElement.GetProperty("device_code").GetString(), Is.Not.Null.And.Not.Empty);
        });
        await using var verification = database.CreateContext();
        Assert.That(await verification.DeviceActivations.CountAsync(), Is.Zero);
    }

    [TestCase("not-a-uuid")]
    [TestCase("")]
    public async Task InvalidInstallationIdentifierCreatesNoAuthorizationState(
        string installationId)
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        using var factory = new LicensingWebApplicationFactory(database, DateTimeOffset.UtcNow);
        using var client = factory.CreateApiClient();

        using var response = await client.PostAsync(
            "/desktop/v1/device/authorize",
            DeviceAuthorizationForm(installationId));

        Assert.That(response.StatusCode, Is.EqualTo(HttpStatusCode.BadRequest));

        await using var verificationContext = database.CreateContext();
        var activationCount = await verificationContext.DeviceActivations.CountAsync();
        var authorizationCount = await verificationContext
            .Set<OpenIddictEntityFrameworkCoreAuthorization<Guid>>()
            .CountAsync();
        var tokenCount = await verificationContext
            .Set<OpenIddictEntityFrameworkCoreToken<Guid>>()
            .CountAsync();
        Assert.Multiple(() =>
        {
            Assert.That(activationCount, Is.Zero);
            Assert.That(authorizationCount, Is.Zero);
            Assert.That(tokenCount, Is.Zero);
        });
    }

    [TestCase(
        "unknown-desktop",
        "styrhous.desktop offline_access",
        "invalid_client",
        HttpStatusCode.Unauthorized)]
    [TestCase(ClientId, "styrhous.desktop", "invalid_scope", HttpStatusCode.BadRequest)]
    [TestCase(ClientId, "offline_access", "invalid_scope", HttpStatusCode.BadRequest)]
    [TestCase(
        ClientId,
        "styrhous.desktop offline_access unexpected",
        "invalid_scope",
        HttpStatusCode.BadRequest)]
    public async Task UnknownClientOrUnsupportedScopesCreateNoAuthorizationState(
        string clientId,
        string scope,
        string expectedError,
        HttpStatusCode expectedStatus)
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        using var factory = new LicensingWebApplicationFactory(database, DateTimeOffset.UtcNow);
        using var client = factory.CreateApiClient();

        using var response = await client.PostAsync(
            "/desktop/v1/device/authorize",
            DeviceAuthorizationForm(Guid.CreateVersion7(), clientId, scope));
        using var body = JsonDocument.Parse(await response.Content.ReadAsStringAsync());

        Assert.Multiple(() =>
        {
            Assert.That(response.StatusCode, Is.EqualTo(expectedStatus));
            Assert.That(
                body.RootElement.GetProperty("error").GetString(),
                Is.EqualTo(expectedError));
        });
        await using var verificationContext = database.CreateContext();
        var authorizationCount = await verificationContext
            .Set<OpenIddictEntityFrameworkCoreAuthorization<Guid>>()
            .CountAsync();
        var tokenCount = await verificationContext
            .Set<OpenIddictEntityFrameworkCoreToken<Guid>>()
            .CountAsync();
        Assert.Multiple(() =>
        {
            Assert.That(authorizationCount, Is.Zero);
            Assert.That(tokenCount, Is.Zero);
        });
    }

    [Test]
    public async Task ApprovalIssuesRotatingTokensAndRefreshReuseRevokesTheChain()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(
            database,
            "desktop-token-user",
            "desktop-token@example.com");
        using var factory = new LicensingWebApplicationFactory(database, SignupTime.AddDays(1));
        using var client = factory.CreateApiClient(signup.UserId);
        var installationId = Guid.CreateVersion7();
        var authorization = await StartAuthorizationAsync(client, installationId);

        using var pendingResponse = await PollDeviceTokenAsync(
            client,
            authorization.DeviceCode);
        using var pendingBody = JsonDocument.Parse(
            await pendingResponse.Content.ReadAsStringAsync());
        Assert.Multiple(() =>
        {
            Assert.That(pendingResponse.StatusCode, Is.EqualTo(HttpStatusCode.BadRequest));
            Assert.That(
                pendingBody.RootElement.GetProperty("error").GetString(),
                Is.EqualTo("authorization_pending"));
        });

        using var approvalResponse = await client.GetAsync(
            $"/desktop/v1/device/approval?user_code="
                + Uri.EscapeDataString(authorization.UserCode));
        using var approvalBody = JsonDocument.Parse(
            await approvalResponse.Content.ReadAsStringAsync());
        var approvalRoot = approvalBody.RootElement;
        var approvalInstallation = approvalRoot.GetProperty("installation");
        var approvalSeat = approvalRoot.GetProperty("eligibleSeats")[0];
        Assert.Multiple(() =>
        {
            Assert.That(approvalResponse.StatusCode, Is.EqualTo(HttpStatusCode.OK));
            Assert.That(
                PropertyNames(approvalRoot),
                Is.EquivalentTo(
                [
                    "reasonCode",
                    "installation",
                    "eligibleSeats",
                    "selectedSeatId",
                ]));
            Assert.That(
                PropertyNames(approvalInstallation),
                Is.EquivalentTo(
                [
                    "installationId",
                    "displayName",
                    "platform",
                    "architecture",
                    "styrhousVersion",
                ]));
            Assert.That(
                PropertyNames(approvalSeat),
                Is.EquivalentTo(
                [
                    "seatId",
                    "billingAccountId",
                    "name",
                    "entitlementState",
                    "entitlementReasonCode",
                    "deviceLimit",
                    "canActivate",
                    "activeDevices",
                ]));
            Assert.That(
                approvalInstallation.GetProperty("installationId").GetGuid(),
                Is.EqualTo(installationId));
            Assert.That(
                approvalRoot.GetProperty("selectedSeatId").GetGuid(),
                Is.EqualTo(signup.SeatId));
            Assert.That(
                approvalRoot.GetProperty("eligibleSeats").GetArrayLength(),
                Is.EqualTo(1));
            Assert.That(approvalSeat.GetProperty("canActivate").GetBoolean(), Is.True);
        });

        using var approvedResponse = await ApproveAsync(
            client,
            authorization.UserCode,
            signup.SeatId);
        Assert.That(approvedResponse.IsSuccessStatusCode, Is.True);

        using var tokenResponse = await PollDeviceTokenAsync(
            client,
            authorization.DeviceCode);
        var firstTokens = await ReadTokenResponseAsync(tokenResponse);
        Assert.Multiple(() =>
        {
            Assert.That(tokenResponse.StatusCode, Is.EqualTo(HttpStatusCode.OK));
            Assert.That(firstTokens.AccessToken, Is.Not.Empty);
            Assert.That(firstTokens.RefreshToken, Is.Not.Empty);
            Assert.That(firstTokens.TokenType, Is.EqualTo("Bearer").IgnoreCase);
            Assert.That(firstTokens.ExpiresIn, Is.EqualTo(900));
            Assert.That(firstTokens.Scope, Is.EqualTo("offline_access styrhous.desktop").Or
                .EqualTo("styrhous.desktop offline_access"));
        });

        using var refreshResponse = await RefreshAsync(client, firstTokens.RefreshToken);
        var rotatedTokens = await ReadTokenResponseAsync(refreshResponse);
        Assert.Multiple(() =>
        {
            Assert.That(refreshResponse.StatusCode, Is.EqualTo(HttpStatusCode.OK));
            Assert.That(rotatedTokens.AccessToken, Is.Not.Empty);
            Assert.That(rotatedTokens.RefreshToken, Is.Not.Empty);
            Assert.That(rotatedTokens.RefreshToken, Is.Not.EqualTo(firstTokens.RefreshToken));
        });

        using var replayResponse = await RefreshAsync(client, firstTokens.RefreshToken);
        using var replayBody = JsonDocument.Parse(
            await replayResponse.Content.ReadAsStringAsync());
        using var revokedChainResponse = await RefreshAsync(
            client,
            rotatedTokens.RefreshToken);
        using var revokedChainBody = JsonDocument.Parse(
            await revokedChainResponse.Content.ReadAsStringAsync());
        Assert.Multiple(() =>
        {
            Assert.That(replayResponse.StatusCode, Is.EqualTo(HttpStatusCode.BadRequest));
            Assert.That(
                replayBody.RootElement.GetProperty("error").GetString(),
                Is.EqualTo("invalid_grant"));
            Assert.That(revokedChainResponse.StatusCode, Is.EqualTo(HttpStatusCode.BadRequest));
            Assert.That(
                revokedChainBody.RootElement.GetProperty("error").GetString(),
                Is.EqualTo("invalid_grant"));
        });

        await using var verificationContext = database.CreateContext();
        var activation = await verificationContext.DeviceActivations.SingleAsync();
        var applicationIds = await verificationContext
            .Set<OpenIddictEntityFrameworkCoreApplication<Guid>>()
            .Select(value => value.Id)
            .ToArrayAsync();
        var authorizationIds = await verificationContext
            .Set<OpenIddictEntityFrameworkCoreAuthorization<Guid>>()
            .Select(value => value.Id)
            .ToArrayAsync();
        var tokenEntries = await verificationContext
            .Set<OpenIddictEntityFrameworkCoreToken<Guid>>()
            .Select(value => new
            {
                value.Id,
                value.Type,
                value.Status,
                value.CreationDate,
                value.ExpirationDate,
            })
            .ToArrayAsync();
        var accessTokenEntries = tokenEntries.Where(entry =>
            entry.Type == OpenIddictConstants.TokenTypeIdentifiers.AccessToken).ToArray();
        var refreshTokenEntries = tokenEntries.Where(entry =>
            entry.Type == OpenIddictConstants.TokenTypeIdentifiers.RefreshToken).ToArray();
        Assert.Multiple(() =>
        {
            Assert.That(activation.InstallationId, Is.EqualTo(installationId));
            Assert.That(activation.SeatId, Is.EqualTo(signup.SeatId));
            Assert.That(applicationIds, Has.All.Property("Version").EqualTo(7));
            Assert.That(authorizationIds, Is.Not.Empty);
            Assert.That(authorizationIds, Has.All.Property("Version").EqualTo(7));
            Assert.That(tokenEntries, Is.Not.Empty);
            Assert.That(
                tokenEntries.Select(entry => entry.Id),
                Has.All.Property("Version").EqualTo(7));
            Assert.That(
                accessTokenEntries
                    .Select(entry => entry.ExpirationDate - entry.CreationDate),
                Has.All.EqualTo(TimeSpan.FromMinutes(15)));
            Assert.That(accessTokenEntries, Has.Length.EqualTo(2));
            Assert.That(
                refreshTokenEntries
                    .Select(entry => entry.ExpirationDate - entry.CreationDate),
                Has.All.EqualTo(TimeSpan.FromDays(90)));
            Assert.That(refreshTokenEntries, Has.Length.EqualTo(2));
            Assert.That(
                refreshTokenEntries
                    .Select(entry => entry.Status),
                Has.All.EqualTo(OpenIddictConstants.Statuses.Revoked));
        });
    }

    [Test]
    public async Task DenialProducesAccessDeniedWithoutActivatingTheInstallation()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(
            database,
            "desktop-denial-user",
            "desktop-denial@example.com");
        using var factory = new LicensingWebApplicationFactory(database, SignupTime.AddDays(1));
        using var client = factory.CreateApiClient(signup.UserId);
        var authorization = await StartAuthorizationAsync(
            client,
            Guid.CreateVersion7());

        using var deniedResponse = await DenyAsync(client, authorization.UserCode);
        using var tokenResponse = await PollDeviceTokenAsync(
            client,
            authorization.DeviceCode);
        using var tokenBody = JsonDocument.Parse(
            await tokenResponse.Content.ReadAsStringAsync());

        Assert.Multiple(() =>
        {
            Assert.That(deniedResponse.IsSuccessStatusCode, Is.True);
            Assert.That(tokenResponse.StatusCode, Is.EqualTo(HttpStatusCode.BadRequest));
            Assert.That(
                tokenBody.RootElement.GetProperty("error").GetString(),
                Is.EqualTo("access_denied"));
        });
        await using var verificationContext = database.CreateContext();
        Assert.That(await verificationContext.DeviceActivations.CountAsync(), Is.Zero);
    }

    [Test]
    public async Task BrowserApprovalRequiresAuthenticationAndAntiforgeryBeforeStateChanges()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(
            database,
            "desktop-security-user",
            "desktop-security@example.com");
        using var factory = new LicensingWebApplicationFactory(database, SignupTime.AddDays(1));
        using var anonymousClient = factory.CreateApiClient();
        using var authenticatedClient = factory.CreateApiClient(signup.UserId);
        var authorization = await StartAuthorizationAsync(
            authenticatedClient,
            Guid.CreateVersion7());

        using var anonymousGet = await anonymousClient.GetAsync(
            $"/desktop/v1/device/approval?user_code="
                + Uri.EscapeDataString(authorization.UserCode));
        using var missingAntiforgery = await authenticatedClient.PostAsync(
            "/desktop/v1/device/approval",
            ApprovalForm(authorization.UserCode, signup.SeatId));
        using var pendingResponse = await PollDeviceTokenAsync(
            authenticatedClient,
            authorization.DeviceCode);
        using var pendingBody = JsonDocument.Parse(
            await pendingResponse.Content.ReadAsStringAsync());

        Assert.Multiple(() =>
        {
            Assert.That(anonymousGet.StatusCode, Is.EqualTo(HttpStatusCode.Unauthorized));
            Assert.That(missingAntiforgery.StatusCode, Is.EqualTo(HttpStatusCode.BadRequest));
            Assert.That(pendingResponse.StatusCode, Is.EqualTo(HttpStatusCode.BadRequest));
            Assert.That(
                pendingBody.RootElement.GetProperty("error").GetString(),
                Is.EqualTo("authorization_pending"));
        });
        await using var verificationContext = database.CreateContext();
        Assert.That(await verificationContext.DeviceActivations.CountAsync(), Is.Zero);
    }

    [TestCase("user_code")]
    [TestCase("decision")]
    [TestCase("seat_id")]
    public async Task DuplicateApprovalFieldReturnsInvalidRequestWithoutChangingState(
        string duplicatedField)
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(
            database,
            $"desktop-duplicate-{duplicatedField}",
            $"desktop-duplicate-{duplicatedField}@example.com");
        using var factory = new LicensingWebApplicationFactory(
            database,
            SignupTime.AddDays(1));
        using var client = factory.CreateApiClient(signup.UserId);
        var authorization = await StartAuthorizationAsync(
            client,
            Guid.CreateVersion7());
        var fields = new List<KeyValuePair<string, string>>
        {
            new("user_code", authorization.UserCode),
            new("decision", "approve"),
            new("seat_id", signup.SeatId.ToString()),
        };
        fields.Add(duplicatedField switch
        {
            "user_code" => new("user_code", authorization.UserCode),
            "decision" => new("decision", "approve"),
            "seat_id" => new("seat_id", signup.SeatId.ToString()),
            _ => throw new ArgumentOutOfRangeException(nameof(duplicatedField)),
        });
        var antiforgeryToken = await AntiforgeryTestClient.GetTokenAsync(client);
        using var request = new HttpRequestMessage(
            HttpMethod.Post,
            DesktopProtocolConstants.ApprovalPath)
        {
            Content = new FormUrlEncodedContent(fields),
        };
        AntiforgeryTestClient.AddToken(request, antiforgeryToken);

        using var response = await client.SendAsync(request);
        using var body = JsonDocument.Parse(await response.Content.ReadAsStringAsync());
        using var pendingResponse = await PollDeviceTokenAsync(
            client,
            authorization.DeviceCode);
        using var pendingBody = JsonDocument.Parse(
            await pendingResponse.Content.ReadAsStringAsync());

        Assert.Multiple(() =>
        {
            Assert.That(response.StatusCode, Is.EqualTo(HttpStatusCode.BadRequest));
            Assert.That(PropertyNames(body.RootElement), Is.EqualTo(["reasonCode"]));
            Assert.That(
                body.RootElement.GetProperty("reasonCode").GetString(),
                Is.EqualTo(DesktopDeviceAuthorizationReasonCodes.InvalidRequest));
            Assert.That(pendingResponse.StatusCode, Is.EqualTo(HttpStatusCode.BadRequest));
            Assert.That(
                pendingBody.RootElement.GetProperty("error").GetString(),
                Is.EqualTo("authorization_pending"));
        });
        await using var verificationContext = database.CreateContext();
        Assert.Multiple(async () =>
        {
            Assert.That(await verificationContext.DeviceActivations.CountAsync(), Is.Zero);
            Assert.That(await verificationContext.DesktopDeviceSessions.CountAsync(), Is.Zero);
        });
    }

    [Test]
    public async Task CapacityConflictCanBeResolvedWithoutRestartingAuthorization()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(
            database,
            "desktop-capacity-user",
            "desktop-capacity@example.com");
        var first = await ActivateAsync(database, signup, 1, SignupTime);
        await ActivateAsync(database, signup, 2, SignupTime);
        await ActivateAsync(database, signup, 3, SignupTime);
        using var factory = new LicensingWebApplicationFactory(database, SignupTime.AddDays(1));
        using var client = factory.CreateApiClient(signup.UserId);
        var authorization = await StartAuthorizationAsync(
            client,
            Guid.CreateVersion7());
        using var approvalResponse = await client.GetAsync(
            $"/desktop/v1/device/approval?user_code="
                + Uri.EscapeDataString(authorization.UserCode));
        using var approvalBody = JsonDocument.Parse(
            await approvalResponse.Content.ReadAsStringAsync());
        var seat = approvalBody.RootElement.GetProperty("eligibleSeats")[0];

        using var conflictResponse = await ApproveAsync(
            client,
            authorization.UserCode,
            signup.SeatId);
        using var conflictBody = JsonDocument.Parse(
            await conflictResponse.Content.ReadAsStringAsync());
        Assert.Multiple(() =>
        {
            Assert.That(approvalResponse.StatusCode, Is.EqualTo(HttpStatusCode.OK));
            Assert.That(seat.GetProperty("canActivate").GetBoolean(), Is.False);
            Assert.That(conflictResponse.StatusCode, Is.EqualTo(HttpStatusCode.Conflict));
            Assert.That(
                PropertyNames(conflictBody.RootElement),
                Is.EquivalentTo(["reasonCode", "activeDevices"]));
            Assert.That(
                conflictBody.RootElement.GetProperty("reasonCode").GetString(),
                Is.EqualTo("device_limit_reached"));
            Assert.That(
                conflictBody.RootElement.GetProperty("activeDevices").GetArrayLength(),
                Is.EqualTo(3));
            Assert.That(
                PropertyNames(conflictBody.RootElement.GetProperty("activeDevices")[0]),
                Is.EquivalentTo(
                [
                    "activationId",
                    "installationId",
                    "displayName",
                    "platform",
                    "architecture",
                    "styrhousVersion",
                    "activatedAt",
                    "lastSeenAt",
                ]));
        });

        using var revokeResponse = await RevokeAsync(
            client,
            first.ActivationId!.Value);
        using var approvedResponse = await ApproveAsync(
            client,
            authorization.UserCode,
            signup.SeatId);
        using var tokenResponse = await PollDeviceTokenAsync(
            client,
            authorization.DeviceCode);
        var tokens = await ReadTokenResponseAsync(tokenResponse);
        Assert.Multiple(() =>
        {
            Assert.That(revokeResponse.StatusCode, Is.EqualTo(HttpStatusCode.OK));
            Assert.That(approvedResponse.StatusCode, Is.EqualTo(HttpStatusCode.NoContent));
            Assert.That(tokenResponse.StatusCode, Is.EqualTo(HttpStatusCode.OK));
            Assert.That(tokens.AccessToken, Is.Not.Empty);
            Assert.That(tokens.RefreshToken, Is.Not.Empty);
        });
    }

    [Test]
    public async Task ManualDeviceRevocationImmediatelyRevokesTheRefreshAuthorization()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(
            database,
            "desktop-revocation-user",
            "desktop-revocation@example.com");
        using var factory = new LicensingWebApplicationFactory(database, SignupTime.AddDays(1));
        using var client = factory.CreateApiClient(signup.UserId);
        var authorization = await StartAuthorizationAsync(
            client,
            Guid.CreateVersion7());
        using var approvedResponse = await ApproveAsync(
            client,
            authorization.UserCode,
            signup.SeatId);
        using var tokenResponse = await PollDeviceTokenAsync(
            client,
            authorization.DeviceCode);
        var tokens = await ReadTokenResponseAsync(tokenResponse);

        Guid activationId;
        await using (var activationContext = database.CreateContext())
        {
            activationId = await activationContext.DeviceActivations
                .Select(activation => activation.Id)
                .SingleAsync();
        }

        using var revokeResponse = await RevokeAsync(client, activationId);

        await using var verificationContext = database.CreateContext();
        var refreshTokenStatuses = await verificationContext
            .Set<OpenIddictEntityFrameworkCoreToken<Guid>>()
            .Where(token => token.Type
                == OpenIddictConstants.TokenTypeIdentifiers.RefreshToken)
            .Select(token => token.Status)
            .ToArrayAsync();
        Assert.Multiple(() =>
        {
            Assert.That(approvedResponse.StatusCode, Is.EqualTo(HttpStatusCode.NoContent));
            Assert.That(tokenResponse.StatusCode, Is.EqualTo(HttpStatusCode.OK));
            Assert.That(revokeResponse.StatusCode, Is.EqualTo(HttpStatusCode.OK));
            Assert.That(refreshTokenStatuses, Is.Not.Empty);
            Assert.That(
                refreshTokenStatuses,
                Has.All.EqualTo(OpenIddictConstants.Statuses.Revoked));
        });

        using var rejectedRefresh = await RefreshAsync(client, tokens.RefreshToken);
        using var rejectedBody = JsonDocument.Parse(
            await rejectedRefresh.Content.ReadAsStringAsync());
        Assert.Multiple(() =>
        {
            Assert.That(rejectedRefresh.StatusCode, Is.EqualTo(HttpStatusCode.BadRequest));
            Assert.That(
                rejectedBody.RootElement.GetProperty("error").GetString(),
                Is.EqualTo("invalid_grant"));
        });
    }

    [Test]
    public async Task ConcurrentManualRevocationCannotLeaveANewRefreshTokenUsable()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(
            database,
            "desktop-refresh-revocation-race",
            "desktop-refresh-revocation-race@example.com");
        DesktopTokenValues tokens;
        Guid activationId;
        using (var issuingFactory = new LicensingWebApplicationFactory(
            database,
            SignupTime.AddDays(1)))
        using (var issuingClient = issuingFactory.CreateApiClient(signup.UserId))
        {
            tokens = await AuthorizeAndIssueTokensAsync(
                issuingClient,
                signup.SeatId,
                Guid.CreateVersion7());
            await using var context = database.CreateContext();
            activationId = await context.DeviceActivations
                .Select(activation => activation.Id)
                .SingleAsync();
        }

        var gate = new DatabaseCommandGate();
        var interceptor = new DatabaseCommandGateInterceptor(
            gate,
            "UPDATE user_accounts");
        using var refreshingFactory = new LicensingWebApplicationFactory(
            database,
            SignupTime.AddDays(1).AddMinutes(1),
            interceptors: [interceptor]);
        using var refreshingClient = refreshingFactory.CreateApiClient(signup.UserId);
        using var revokingFactory = new LicensingWebApplicationFactory(
            database,
            SignupTime.AddDays(1).AddMinutes(1));
        using var revokingClient = revokingFactory.CreateApiClient(signup.UserId);

        var refreshTask = RefreshAsync(refreshingClient, tokens.RefreshToken);
        HttpResponseMessage? revocationResponse = null;
        try
        {
            await gate.WaitUntilReachedAsync();
            revocationResponse = await RevokeAsync(revokingClient, activationId);
        }
        finally
        {
            gate.Release();
        }

        HttpResponseMessage? refreshResponse = null;
        Exception? refreshException = null;
        try
        {
            refreshResponse = await refreshTask;
        }
        catch (Exception exception)
        {
            refreshException = exception;
        }
        JsonDocument? refreshError = refreshResponse is null
            ? null
            : JsonDocument.Parse(await refreshResponse.Content.ReadAsStringAsync());

        using (revocationResponse)
        using (refreshResponse)
        using (refreshError)
        {
            Assert.Multiple(() =>
            {
                Assert.That(
                    revocationResponse?.StatusCode,
                    Is.EqualTo(HttpStatusCode.OK));
                Assert.That(
                    refreshException,
                    Is.Null,
                    "The losing refresh should receive a stable OAuth error.");
                Assert.That(
                    refreshResponse?.StatusCode,
                    Is.EqualTo(HttpStatusCode.BadRequest));
                Assert.That(
                    refreshError?.RootElement.GetProperty("error").GetString(),
                    Is.EqualTo("invalid_grant"));
            });
        }

        await using var verificationContext = database.CreateContext();
        var authorizationStatuses = await verificationContext
            .Set<OpenIddictEntityFrameworkCoreAuthorization<Guid>>()
            .Select(authorization => authorization.Status)
            .ToArrayAsync();
        var refreshTokenStatuses = await verificationContext
            .Set<OpenIddictEntityFrameworkCoreToken<Guid>>()
            .Where(token => token.Type
                == OpenIddictConstants.TokenTypeIdentifiers.RefreshToken)
            .Select(token => token.Status)
            .ToArrayAsync();
        var session = await verificationContext.DesktopDeviceSessions.SingleAsync();
        Assert.Multiple(() =>
        {
            Assert.That(
                authorizationStatuses,
                Has.All.EqualTo(OpenIddictConstants.Statuses.Revoked));
            Assert.That(refreshTokenStatuses, Is.Not.Empty);
            Assert.That(
                refreshTokenStatuses,
                Has.All.EqualTo(OpenIddictConstants.Statuses.Revoked));
            Assert.That(session.RevokedAt, Is.Not.Null);
        });
    }

    [Test]
    public async Task ConcurrentRefreshReuseRecoveryRetriesAndRevokesAnAdvancedWinningChain()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(
            database,
            "desktop-concurrent-refresh-reuse",
            "desktop-concurrent-refresh-reuse@example.com");
        DesktopTokenValues tokens;
        using (var issuingFactory = new LicensingWebApplicationFactory(
            database,
            SignupTime.AddDays(1)))
        using (var issuingClient = issuingFactory.CreateApiClient(signup.UserId))
        {
            tokens = await AuthorizeAndIssueTokensAsync(
                issuingClient,
                signup.SeatId,
                Guid.CreateVersion7());
        }

        var initialGate = new DatabaseCommandGate();
        var initialGateInterceptor = new DatabaseCommandGateInterceptor(
            initialGate,
            "UPDATE user_accounts");
        var recoveryGate = new DatabaseCommandGate();
        var recoveryGateInterceptor = new DatabaseCommandGateInterceptor(
            recoveryGate,
            "UPDATE user_accounts",
            matchingOccurrence: 2);
        var recoveryInterceptor = new OneTimeCommandConcurrencyFailureInterceptor(
            "UPDATE \"OpenIddictAuthorizations\"");
        using var losingFactory = new LicensingWebApplicationFactory(
            database,
            SignupTime.AddDays(1).AddMinutes(1),
            interceptors:
            [
                initialGateInterceptor,
                recoveryGateInterceptor,
                recoveryInterceptor,
            ]);
        using var losingClient = losingFactory.CreateApiClient(signup.UserId);
        using var winningFactory = new LicensingWebApplicationFactory(
            database,
            SignupTime.AddDays(1).AddMinutes(1));
        using var winningClient = winningFactory.CreateApiClient(signup.UserId);

        var losingTask = RefreshAsync(losingClient, tokens.RefreshToken);
        HttpResponseMessage? winningResponse = null;
        HttpResponseMessage? advancedWinnerResponse = null;
        try
        {
            await initialGate.WaitUntilReachedAsync();
            winningResponse = await RefreshAsync(winningClient, tokens.RefreshToken);
            var winningTokens = await ReadTokenResponseAsync(winningResponse);
            initialGate.Release();
            await recoveryGate.WaitUntilReachedAsync();
            advancedWinnerResponse = await RefreshAsync(
                winningClient,
                winningTokens.RefreshToken);
        }
        finally
        {
            initialGate.Release();
            recoveryGate.Release();
        }

        using (winningResponse)
        using (advancedWinnerResponse)
        {
            Assert.Multiple(() =>
            {
                Assert.That(winningResponse?.StatusCode, Is.EqualTo(HttpStatusCode.OK));
                Assert.That(advancedWinnerResponse?.StatusCode, Is.EqualTo(HttpStatusCode.OK));
            });
            var advancedWinnerTokens = await ReadTokenResponseAsync(advancedWinnerResponse!);
            using var losingResponse = await losingTask;
            using var losingBody = JsonDocument.Parse(
                await losingResponse.Content.ReadAsStringAsync());
            using var rejectedWinner = await RefreshAsync(
                winningClient,
                advancedWinnerTokens.RefreshToken);
            using var rejectedWinnerBody = JsonDocument.Parse(
                await rejectedWinner.Content.ReadAsStringAsync());

            Assert.Multiple(() =>
            {
                Assert.That(losingResponse.StatusCode, Is.EqualTo(HttpStatusCode.BadRequest));
                Assert.That(
                    losingBody.RootElement.GetProperty("error").GetString(),
                    Is.EqualTo("invalid_grant"));
                Assert.That(
                    rejectedWinner.StatusCode,
                    Is.EqualTo(HttpStatusCode.BadRequest));
                Assert.That(
                    rejectedWinnerBody.RootElement.GetProperty("error").GetString(),
                    Is.EqualTo("invalid_grant"));
                Assert.That(recoveryInterceptor.MatchingCommandCount, Is.EqualTo(2));
            });
        }

        await using var verificationContext = database.CreateContext();
        var authorizationStatuses = await verificationContext
            .Set<OpenIddictEntityFrameworkCoreAuthorization<Guid>>()
            .Select(authorization => authorization.Status)
            .ToArrayAsync();
        var tokenStatuses = await verificationContext
            .Set<OpenIddictEntityFrameworkCoreToken<Guid>>()
            .Select(token => token.Status)
            .ToArrayAsync();
        var session = await verificationContext.DesktopDeviceSessions.SingleAsync();
        Assert.Multiple(() =>
        {
            Assert.That(
                authorizationStatuses,
                Has.All.EqualTo(OpenIddictConstants.Statuses.Revoked));
            Assert.That(tokenStatuses, Is.Not.Empty);
            Assert.That(
                tokenStatuses,
                Has.All.EqualTo(OpenIddictConstants.Statuses.Revoked));
            Assert.That(session.RevokedAt, Is.Not.Null);
        });
    }

    [Test]
    public async Task ConcurrentDeviceCodePollRejectsOnlyTheLosingPoll()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(
            database,
            "desktop-concurrent-device-code",
            "desktop-concurrent-device-code@example.com");
        DeviceAuthorizationValues authorization;
        using (var setupFactory = new LicensingWebApplicationFactory(
            database,
            SignupTime.AddDays(1)))
        using (var setupClient = setupFactory.CreateApiClient(signup.UserId))
        {
            authorization = await StartAuthorizationAsync(
                setupClient,
                Guid.CreateVersion7());
            using var approval = await ApproveAsync(
                setupClient,
                authorization.UserCode,
                signup.SeatId);
            Assert.That(approval.StatusCode, Is.EqualTo(HttpStatusCode.NoContent));
        }

        var gate = new DatabaseCommandGate();
        var interceptor = new DatabaseCommandGateInterceptor(
            gate,
            "UPDATE user_accounts");
        using var losingFactory = new LicensingWebApplicationFactory(
            database,
            SignupTime.AddDays(1).AddMinutes(1),
            interceptors: [interceptor]);
        using var losingClient = losingFactory.CreateApiClient(signup.UserId);
        using var winningFactory = new LicensingWebApplicationFactory(
            database,
            SignupTime.AddDays(1).AddMinutes(1));
        using var winningClient = winningFactory.CreateApiClient(signup.UserId);

        var losingTask = PollDeviceTokenAsync(losingClient, authorization.DeviceCode);
        HttpResponseMessage? winningResponse = null;
        try
        {
            await gate.WaitUntilReachedAsync();
            winningResponse = await PollDeviceTokenAsync(
                winningClient,
                authorization.DeviceCode);
        }
        finally
        {
            gate.Release();
        }

        using (winningResponse)
        {
            Assert.That(winningResponse?.StatusCode, Is.EqualTo(HttpStatusCode.OK));
            var winningTokens = await ReadTokenResponseAsync(winningResponse!);
            using var losingResponse = await losingTask;
            using var losingBody = JsonDocument.Parse(
                await losingResponse.Content.ReadAsStringAsync());
            using var continuedSession = await RefreshAsync(
                winningClient,
                winningTokens.RefreshToken);

            Assert.Multiple(() =>
            {
                Assert.That(losingResponse.StatusCode, Is.EqualTo(HttpStatusCode.BadRequest));
                Assert.That(
                    losingBody.RootElement.GetProperty("error").GetString(),
                    Is.EqualTo("invalid_grant"));
                Assert.That(continuedSession.StatusCode, Is.EqualTo(HttpStatusCode.OK));
            });
        }
    }

    [Test]
    public async Task DeviceCodeCommitFailureReleasesOnlyAnErrorAndLeavesTheCodeReusable()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(
            database,
            "desktop-commit-failure",
            "desktop-commit-failure@example.com");
        var interceptor = new OneTimeCommitFailureInterceptor();
        using var factory = new LicensingWebApplicationFactory(
            database,
            SignupTime.AddDays(1),
            interceptors: [interceptor]);
        using var client = factory.CreateApiClient(signup.UserId);
        var authorization = await StartAuthorizationAsync(
            client,
            Guid.CreateVersion7());
        using var approval = await ApproveAsync(
            client,
            authorization.UserCode,
            signup.SeatId);
        interceptor.FailNextCommit();

        using var response = await PollDeviceTokenAsync(client, authorization.DeviceCode);
        using var body = JsonDocument.Parse(await response.Content.ReadAsStringAsync());
        await using (var failedCommitContext = database.CreateContext())
        {
            var issuedTokenCount = await failedCommitContext
                .Set<OpenIddictEntityFrameworkCoreToken<Guid>>()
                .CountAsync(token => token.Type
                    == OpenIddictConstants.TokenTypeIdentifiers.AccessToken
                    || token.Type
                    == OpenIddictConstants.TokenTypeIdentifiers.RefreshToken);
            var session = await failedCommitContext.DesktopDeviceSessions.SingleAsync();
            Assert.Multiple(() =>
            {
                Assert.That(issuedTokenCount, Is.Zero);
                Assert.That(session.RevokedAt, Is.Null);
            });
        }

        using var successfulRetry = await PollDeviceTokenAsync(
            client,
            authorization.DeviceCode);

        Assert.Multiple(() =>
        {
            Assert.That(approval.StatusCode, Is.EqualTo(HttpStatusCode.NoContent));
            Assert.That(response.StatusCode, Is.EqualTo(HttpStatusCode.BadRequest));
            Assert.That(
                PropertyNames(body.RootElement),
                Is.EquivalentTo(["error", "error_description"]));
            Assert.That(
                body.RootElement.GetProperty("error").GetString(),
                Is.EqualTo("invalid_grant"));
            Assert.That(successfulRetry.StatusCode, Is.EqualTo(HttpStatusCode.OK));
        });
        await ReadTokenResponseAsync(successfulRetry);
    }

    [Test]
    public async Task RefreshCommitFailureReleasesOnlyAnErrorAndRetainsTheUnredeemedCredential()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(
            database,
            "desktop-refresh-commit-failure",
            "desktop-refresh-commit-failure@example.com");
        var interceptor = new OneTimeCommitFailureInterceptor();
        using var factory = new LicensingWebApplicationFactory(
            database,
            SignupTime.AddDays(1),
            interceptors: [interceptor]);
        using var client = factory.CreateApiClient(signup.UserId);
        var tokens = await AuthorizeAndIssueTokensAsync(
            client,
            signup.SeatId,
            Guid.CreateVersion7());
        var before = await ReadDesktopSessionStateAsync(database);
        interceptor.FailNextCommit();

        using var response = await RefreshAsync(client, tokens.RefreshToken);
        using var body = JsonDocument.Parse(await response.Content.ReadAsStringAsync());
        var rolledBack = await ReadDesktopSessionStateAsync(database);

        Assert.Multiple(() =>
        {
            Assert.That(response.StatusCode, Is.EqualTo(HttpStatusCode.ServiceUnavailable));
            Assert.That(
                PropertyNames(body.RootElement),
                Is.EquivalentTo(["error", "error_description"]));
            Assert.That(
                body.RootElement.GetProperty("error").GetString(),
                Is.EqualTo("temporarily_unavailable"));
            Assert.That(rolledBack.AuthorizationStatuses, Is.EquivalentTo(before.AuthorizationStatuses));
            Assert.That(rolledBack.TokenStatuses, Is.EquivalentTo(before.TokenStatuses));
            Assert.That(rolledBack.SessionRevokedAt, Is.Null);
        });
        using var successfulRetry = await RefreshAsync(client, tokens.RefreshToken);
        Assert.That(successfulRetry.StatusCode, Is.EqualTo(HttpStatusCode.OK));
        await ReadTokenResponseAsync(successfulRetry);
    }

    [Test]
    public async Task ProtocolRevocationIncludesARefreshCommittedBeforeItsUserLock()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(
            database,
            "desktop-reverse-protocol-revocation-race",
            "desktop-reverse-protocol-revocation-race@example.com");
        DesktopTokenValues tokens;
        using (var issuingFactory = new LicensingWebApplicationFactory(
            database,
            SignupTime.AddDays(1)))
        using (var issuingClient = issuingFactory.CreateApiClient(signup.UserId))
        {
            tokens = await AuthorizeAndIssueTokensAsync(
                issuingClient,
                signup.SeatId,
                Guid.CreateVersion7());
        }

        var gate = new DatabaseCommandGate();
        var interceptor = new DatabaseCommandGateInterceptor(
            gate,
            "UPDATE user_accounts");
        using var revokingFactory = new LicensingWebApplicationFactory(
            database,
            SignupTime.AddDays(1).AddMinutes(1),
            interceptors: [interceptor]);
        using var revokingClient = revokingFactory.CreateApiClient(signup.UserId);
        using var refreshingFactory = new LicensingWebApplicationFactory(
            database,
            SignupTime.AddDays(1).AddMinutes(1));
        using var refreshingClient = refreshingFactory.CreateApiClient(signup.UserId);

        var revocationTask = RevokeTokenAsync(
            revokingClient,
            tokens.RefreshToken,
            tokenTypeHint: null);
        HttpResponseMessage? refreshResponse = null;
        try
        {
            await gate.WaitUntilReachedAsync();
            refreshResponse = await RefreshAsync(
                refreshingClient,
                tokens.RefreshToken);
        }
        finally
        {
            gate.Release();
        }

        using (refreshResponse)
        {
            Assert.That(refreshResponse?.StatusCode, Is.EqualTo(HttpStatusCode.OK));
            var refreshed = await ReadTokenResponseAsync(refreshResponse!);
            using var revocationResponse = await revocationTask;
            using var rejected = await RefreshAsync(
                refreshingClient,
                refreshed.RefreshToken);
            using var rejectedBody = JsonDocument.Parse(
                await rejected.Content.ReadAsStringAsync());

            Assert.Multiple(() =>
            {
                Assert.That(revocationResponse.StatusCode, Is.EqualTo(HttpStatusCode.OK));
                Assert.That(rejected.StatusCode, Is.EqualTo(HttpStatusCode.BadRequest));
                Assert.That(
                    rejectedBody.RootElement.GetProperty("error").GetString(),
                    Is.EqualTo("invalid_grant"));
            });
        }
    }

    [Test]
    public async Task ManualRevocationIncludesARefreshCommittedBeforeItsUserLock()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(
            database,
            "desktop-reverse-refresh-revocation-race",
            "desktop-reverse-refresh-revocation-race@example.com");
        DesktopTokenValues tokens;
        Guid activationId;
        using (var issuingFactory = new LicensingWebApplicationFactory(
            database,
            SignupTime.AddDays(1)))
        using (var issuingClient = issuingFactory.CreateApiClient(signup.UserId))
        {
            tokens = await AuthorizeAndIssueTokensAsync(
                issuingClient,
                signup.SeatId,
                Guid.CreateVersion7());
            await using var context = database.CreateContext();
            activationId = await context.DeviceActivations
                .Select(activation => activation.Id)
                .SingleAsync();
        }

        var gate = new DatabaseCommandGate();
        var interceptor = new DatabaseCommandGateInterceptor(
            gate,
            "UPDATE user_accounts");
        using var revokingFactory = new LicensingWebApplicationFactory(
            database,
            SignupTime.AddDays(1).AddMinutes(1),
            interceptors: [interceptor]);
        using var revokingClient = revokingFactory.CreateApiClient(signup.UserId);
        using var refreshingFactory = new LicensingWebApplicationFactory(
            database,
            SignupTime.AddDays(1).AddMinutes(1));
        using var refreshingClient = refreshingFactory.CreateApiClient(signup.UserId);

        var revocationTask = RevokeAsync(revokingClient, activationId);
        HttpResponseMessage? refreshResponse = null;
        try
        {
            await gate.WaitUntilReachedAsync();
            refreshResponse = await RefreshAsync(refreshingClient, tokens.RefreshToken);
        }
        finally
        {
            gate.Release();
        }

        using (refreshResponse)
        {
            Assert.That(refreshResponse?.StatusCode, Is.EqualTo(HttpStatusCode.OK));
            var refreshed = await ReadTokenResponseAsync(refreshResponse!);
            using var revocationResponse = await revocationTask;
            using var rejected = await RefreshAsync(
                refreshingClient,
                refreshed.RefreshToken);
            using var rejectedBody = JsonDocument.Parse(
                await rejected.Content.ReadAsStringAsync());
            Assert.Multiple(() =>
            {
                Assert.That(
                    revocationResponse.StatusCode,
                    Is.EqualTo(HttpStatusCode.OK));
                Assert.That(rejected.StatusCode, Is.EqualTo(HttpStatusCode.BadRequest));
                Assert.That(
                    rejectedBody.RootElement.GetProperty("error").GetString(),
                    Is.EqualTo("invalid_grant"));
            });
        }

        await using var verificationContext = database.CreateContext();
        var authorizationStatuses = await verificationContext
            .Set<OpenIddictEntityFrameworkCoreAuthorization<Guid>>()
            .Select(authorization => authorization.Status)
            .ToArrayAsync();
        var session = await verificationContext.DesktopDeviceSessions.SingleAsync();
        Assert.Multiple(() =>
        {
            Assert.That(
                authorizationStatuses,
                Has.All.EqualTo(OpenIddictConstants.Statuses.Revoked));
            Assert.That(session.RevokedAt, Is.Not.Null);
        });
    }

    [Test]
    public async Task ApprovalFailureRollsBackTheActivationAndLeavesTheCodePending()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(
            database,
            "desktop-atomic-user",
            "desktop-atomic@example.com");
        using var factory = new LicensingWebApplicationFactory(
            database,
            SignupTime.AddDays(1),
            interceptors: [new FailAfterDesktopApprovalInterceptor()]);
        using var client = factory.CreateApiClient(signup.UserId);
        var authorization = await StartAuthorizationAsync(
            client,
            Guid.CreateVersion7());

        Assert.That(
            async () => await ApproveAsync(
                client,
                authorization.UserCode,
                signup.SeatId),
            Throws.Exception);

        await using (var verificationContext = database.CreateContext())
        {
            var activationCount = await verificationContext.DeviceActivations.CountAsync();
            var sessionCount = await verificationContext.DesktopDeviceSessions.CountAsync();
            Assert.Multiple(() =>
            {
                Assert.That(activationCount, Is.Zero);
                Assert.That(sessionCount, Is.Zero);
            });
        }

        using var pendingResponse = await PollDeviceTokenAsync(
            client,
            authorization.DeviceCode);
        using var pendingBody = JsonDocument.Parse(
            await pendingResponse.Content.ReadAsStringAsync());
        Assert.Multiple(() =>
        {
            Assert.That(pendingResponse.StatusCode, Is.EqualTo(HttpStatusCode.BadRequest));
            Assert.That(
                pendingBody.RootElement.GetProperty("error").GetString(),
                Is.EqualTo("authorization_pending"));
        });
    }

    [Test]
    public async Task RevocationEndpointInvalidatesTheWholeRotatedSessionIdempotently()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(
            database,
            "desktop-logout-user",
            "desktop-logout@example.com");
        using var factory = new LicensingWebApplicationFactory(database, SignupTime.AddDays(1));
        using var client = factory.CreateApiClient(signup.UserId);
        var issued = await AuthorizeAndIssueTokensAsync(
            client,
            signup.SeatId,
            Guid.CreateVersion7());
        using var refreshResponse = await RefreshAsync(client, issued.RefreshToken);
        var rotated = await ReadTokenResponseAsync(refreshResponse);

        using var revokeResponse = await RevokeTokenAsync(
            client,
            rotated.RefreshToken,
            tokenTypeHint: null);
        using var repeatedResponse = await RevokeTokenAsync(client, rotated.RefreshToken);
        using var unknownResponse = await RevokeTokenAsync(client, "unknown-refresh-token");
        using var rejectedRefresh = await RefreshAsync(client, rotated.RefreshToken);
        using var rejectedBody = JsonDocument.Parse(
            await rejectedRefresh.Content.ReadAsStringAsync());

        await using var verificationContext = database.CreateContext();
        var statuses = await verificationContext
            .Set<OpenIddictEntityFrameworkCoreToken<Guid>>()
            .Select(token => token.Status)
            .ToArrayAsync();
        var session = await verificationContext.DesktopDeviceSessions.SingleAsync();
        Assert.Multiple(() =>
        {
            Assert.That(refreshResponse.StatusCode, Is.EqualTo(HttpStatusCode.OK));
            Assert.That(revokeResponse.StatusCode, Is.EqualTo(HttpStatusCode.OK));
            Assert.That(repeatedResponse.StatusCode, Is.EqualTo(HttpStatusCode.OK));
            Assert.That(unknownResponse.StatusCode, Is.EqualTo(HttpStatusCode.OK));
            Assert.That(rejectedRefresh.StatusCode, Is.EqualTo(HttpStatusCode.BadRequest));
            Assert.That(
                rejectedBody.RootElement.GetProperty("error").GetString(),
                Is.EqualTo("invalid_grant"));
            Assert.That(statuses, Is.Not.Empty);
            Assert.That(statuses, Has.All.EqualTo(OpenIddictConstants.Statuses.Revoked));
            Assert.That(session.RevokedAt, Is.EqualTo(SignupTime.AddDays(1)));
        });
    }

    [Test]
    public async Task RevokingAnAccessTokenWithoutAHintInvalidatesTheWholeDesktopSession()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(
            database,
            "desktop-access-token-revocation",
            "desktop-access-token-revocation@example.com");
        using var factory = new LicensingWebApplicationFactory(
            database,
            SignupTime.AddDays(1));
        using var client = factory.CreateApiClient(signup.UserId);
        var issued = await AuthorizeAndIssueTokensAsync(
            client,
            signup.SeatId,
            Guid.CreateVersion7());

        using var revokeResponse = await RevokeTokenAsync(
            client,
            issued.AccessToken,
            tokenTypeHint: null);
        using var rejectedRefresh = await RefreshAsync(client, issued.RefreshToken);
        using var rejectedBody = JsonDocument.Parse(
            await rejectedRefresh.Content.ReadAsStringAsync());

        await using var verificationContext = database.CreateContext();
        var authorizationStatuses = await verificationContext
            .Set<OpenIddictEntityFrameworkCoreAuthorization<Guid>>()
            .Select(authorization => authorization.Status)
            .ToArrayAsync();
        var tokenStatuses = await verificationContext
            .Set<OpenIddictEntityFrameworkCoreToken<Guid>>()
            .Select(token => token.Status)
            .ToArrayAsync();
        var session = await verificationContext.DesktopDeviceSessions.SingleAsync();
        Assert.Multiple(() =>
        {
            Assert.That(revokeResponse.StatusCode, Is.EqualTo(HttpStatusCode.OK));
            Assert.That(rejectedRefresh.StatusCode, Is.EqualTo(HttpStatusCode.BadRequest));
            Assert.That(
                rejectedBody.RootElement.GetProperty("error").GetString(),
                Is.EqualTo("invalid_grant"));
            Assert.That(
                authorizationStatuses,
                Has.All.EqualTo(OpenIddictConstants.Statuses.Revoked));
            Assert.That(tokenStatuses, Is.Not.Empty);
            Assert.That(
                tokenStatuses,
                Has.All.EqualTo(OpenIddictConstants.Statuses.Revoked));
            Assert.That(session.RevokedAt, Is.Not.Null);
        });
    }

    [TestCase(null)]
    [TestCase("unknown-desktop")]
    public async Task RevocationRejectsAMissingOrUnknownClientWithoutRevoking(
        string? clientId)
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(
            database,
            $"desktop-revocation-client-{clientId ?? "missing"}",
            $"desktop-revocation-client-{clientId ?? "missing"}@example.com");
        using var factory = new LicensingWebApplicationFactory(
            database,
            SignupTime.AddDays(1));
        using var client = factory.CreateApiClient(signup.UserId);
        var issued = await AuthorizeAndIssueTokensAsync(
            client,
            signup.SeatId,
            Guid.CreateVersion7());

        using var rejected = await RevokeTokenAsync(
            client,
            issued.RefreshToken,
            clientId);
        using var rejectedBody = JsonDocument.Parse(
            await rejected.Content.ReadAsStringAsync());
        using var refresh = await RefreshAsync(client, issued.RefreshToken);

        Assert.Multiple(() =>
        {
            Assert.That(rejected.StatusCode, Is.EqualTo(HttpStatusCode.Unauthorized));
            Assert.That(
                rejectedBody.RootElement.GetProperty("error").GetString(),
                Is.EqualTo("invalid_client"));
            Assert.That(refresh.StatusCode, Is.EqualTo(HttpStatusCode.OK));
        });
    }

    [Test]
    public async Task SeparateDesktopCertificateRingKeepsOutstandingStateUsableDuringRotation()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(
            database,
            "desktop-certificate-user",
            "desktop-certificate@example.com");
        var oldCertificate =
            TestDataProtectionCertificate.CreateDesktopProtocolRsaCertificate(
                DateTimeOffset.UtcNow.AddYears(2));
        var newCertificate =
            TestDataProtectionCertificate.CreateDesktopProtocolRsaCertificate(
                DateTimeOffset.UtcNow.AddYears(3));
        DesktopTokenValues retainedTokens;
        DesktopTokenValues unretainedTokens;
        DeviceAuthorizationValues pendingAuthorization;
        using (var issuingFactory = new LicensingWebApplicationFactory(
            database,
            SignupTime.AddDays(1),
            desktopCertificateRing: new TestCertificateRing(oldCertificate, [])))
        using (var issuingClient = issuingFactory.CreateApiClient(signup.UserId))
        {
            retainedTokens = await AuthorizeAndIssueTokensAsync(
                issuingClient,
                signup.SeatId,
                Guid.CreateVersion7());
            unretainedTokens = await AuthorizeAndIssueTokensAsync(
                issuingClient,
                signup.SeatId,
                Guid.CreateVersion7());
            pendingAuthorization = await StartAuthorizationAsync(
                issuingClient,
                Guid.CreateVersion7());
        }

        using var rotatingFactory = new LicensingWebApplicationFactory(
            database,
            SignupTime.AddDays(1).AddMinutes(5),
            desktopCertificateRing: new TestCertificateRing(
                newCertificate,
                [oldCertificate]));
        using var rotatingClient = rotatingFactory.CreateApiClient(signup.UserId);
        using var retainedResponse = await RefreshAsync(
            rotatingClient,
            retainedTokens.RefreshToken);
        var transitionTokens = await ReadTokenResponseAsync(retainedResponse);
        using var pendingApproval = await ApproveAsync(
            rotatingClient,
            pendingAuthorization.UserCode,
            signup.SeatId);
        using var pendingTokenResponse = await PollDeviceTokenAsync(
            rotatingClient,
            pendingAuthorization.DeviceCode);

        using var withoutPreviousFactory = new LicensingWebApplicationFactory(
            database,
            SignupTime.AddDays(1).AddMinutes(6),
            desktopCertificateRing: new TestCertificateRing(newCertificate, []));
        using var withoutPreviousClient = withoutPreviousFactory.CreateApiClient(signup.UserId);
        using var unretainedResponse = await RefreshAsync(
            withoutPreviousClient,
            unretainedTokens.RefreshToken);
        using var transitionResponse = await RefreshAsync(
            withoutPreviousClient,
            transitionTokens.RefreshToken);
        using var unretainedBody = JsonDocument.Parse(
            await unretainedResponse.Content.ReadAsStringAsync());

        Assert.Multiple(() =>
        {
            Assert.That(retainedResponse.StatusCode, Is.EqualTo(HttpStatusCode.OK));
            Assert.That(pendingApproval.StatusCode, Is.EqualTo(HttpStatusCode.NoContent));
            Assert.That(pendingTokenResponse.StatusCode, Is.EqualTo(HttpStatusCode.OK));
            Assert.That(unretainedResponse.StatusCode, Is.EqualTo(HttpStatusCode.BadRequest));
            Assert.That(transitionResponse.StatusCode, Is.EqualTo(HttpStatusCode.OK));
            Assert.That(
                unretainedBody.RootElement.GetProperty("error").GetString(),
                Is.EqualTo("invalid_grant"));
        });
    }

    [Test]
    public async Task DesktopCertificateRingRejectsAPreviousCertificateThatExpiresLater()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var currentCertificate =
            TestDataProtectionCertificate.CreateDesktopProtocolRsaCertificate(
                DateTimeOffset.UtcNow.AddYears(2));
        var previousCertificate =
            TestDataProtectionCertificate.CreateDesktopProtocolRsaCertificate(
                DateTimeOffset.UtcNow.AddYears(3));
        using var factory = new LicensingWebApplicationFactory(
            database,
            SignupTime.AddDays(1),
            desktopCertificateRing: new TestCertificateRing(
                currentCertificate,
                [previousCertificate]));

        var exception = Assert.Throws<InvalidOperationException>(
            () => factory.CreateApiClient());

        Assert.That(
            exception!.Message,
            Does.Contain("must expire after every retained previous"));
    }

    [Test]
    public async Task DesktopCertificateRingRejectsMatchingCurrentAndPreviousExpirations()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var expiration = DateTimeOffset.UtcNow.AddYears(2);
        var currentCertificate =
            TestDataProtectionCertificate.CreateDesktopProtocolRsaCertificate(expiration);
        var previousCertificate =
            TestDataProtectionCertificate.CreateDesktopProtocolRsaCertificate(expiration);
        using var factory = new LicensingWebApplicationFactory(
            database,
            SignupTime.AddDays(1),
            desktopCertificateRing: new TestCertificateRing(
                currentCertificate,
                [previousCertificate]));

        var exception = Assert.Throws<InvalidOperationException>(
            () => factory.CreateApiClient());

        Assert.That(
            exception!.Message,
            Does.Contain("must expire after every retained previous"));
    }

    [Test]
    public async Task DesktopCertificateRingRejectsAFutureDatedCurrentCertificate()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var currentCertificate =
            TestDataProtectionCertificate.CreateDesktopProtocolRsaCertificate(
                DateTimeOffset.UtcNow.AddYears(3),
                DateTimeOffset.UtcNow.AddDays(1));
        using var factory = new LicensingWebApplicationFactory(
            database,
            SignupTime.AddDays(1),
            desktopCertificateRing: new TestCertificateRing(currentCertificate, []));

        var exception = Assert.Throws<InvalidOperationException>(
            () => factory.CreateApiClient());

        Assert.That(exception!.Message, Does.Contain("must be currently valid"));
    }

    [Test]
    public async Task DesktopCertificateRingRejectsAnExpiredCurrentCertificate()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var currentCertificate =
            TestDataProtectionCertificate.CreateDesktopProtocolRsaCertificate(
                DateTimeOffset.UtcNow.AddDays(-1));
        using var factory = new LicensingWebApplicationFactory(
            database,
            SignupTime.AddDays(1),
            desktopCertificateRing: new TestCertificateRing(currentCertificate, []));

        var exception = Assert.Throws<InvalidOperationException>(
            () => factory.CreateApiClient());

        Assert.That(exception!.Message, Does.Contain("must be currently valid"));
    }

    [Test]
    public async Task DesktopCertificateRingRejectsEncryptionOnlyKeyUsage()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        using var factory = new LicensingWebApplicationFactory(
            database,
            SignupTime.AddDays(1),
            desktopCertificateRing: new TestCertificateRing(
                TestDataProtectionCertificate.CreateEncryptionOnlyRsaCertificate(),
                []));

        var exception = Assert.Throws<InvalidOperationException>(
            () => factory.CreateApiClient());

        Assert.That(
            exception!.Message,
            Does.Contain("must allow digital signatures and key encipherment"));
    }

    [Test]
    public async Task DeviceAndRefreshStateSurviveIndependentApiHosts()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(
            database,
            "desktop-restart-user",
            "desktop-restart@example.com");
        DeviceAuthorizationValues authorization;
        using (var initiatingFactory = new LicensingWebApplicationFactory(
            database,
            SignupTime.AddDays(1)))
        using (var initiatingClient = initiatingFactory.CreateApiClient(signup.UserId))
        {
            authorization = await StartAuthorizationAsync(
                initiatingClient,
                Guid.CreateVersion7());
        }

        DesktopTokenValues issued;
        using (var approvingFactory = new LicensingWebApplicationFactory(
            database,
            SignupTime.AddDays(1).AddMinutes(1)))
        using (var approvingClient = approvingFactory.CreateApiClient(signup.UserId))
        {
            using var approval = await ApproveAsync(
                approvingClient,
                authorization.UserCode,
                signup.SeatId);
            using var tokenResponse = await PollDeviceTokenAsync(
                approvingClient,
                authorization.DeviceCode);
            Assert.Multiple(() =>
            {
                Assert.That(approval.StatusCode, Is.EqualTo(HttpStatusCode.NoContent));
                Assert.That(tokenResponse.StatusCode, Is.EqualTo(HttpStatusCode.OK));
            });
            issued = await ReadTokenResponseAsync(tokenResponse);
        }

        using var refreshingFactory = new LicensingWebApplicationFactory(
            database,
            SignupTime.AddDays(1).AddMinutes(2));
        using var refreshingClient = refreshingFactory.CreateApiClient(signup.UserId);
        using var refreshResponse = await RefreshAsync(refreshingClient, issued.RefreshToken);
        var refreshed = await ReadTokenResponseAsync(refreshResponse);
        Assert.Multiple(() =>
        {
            Assert.That(refreshResponse.StatusCode, Is.EqualTo(HttpStatusCode.OK));
            Assert.That(refreshed.AccessToken, Is.Not.Empty);
            Assert.That(refreshed.RefreshToken, Is.Not.EqualTo(issued.RefreshToken));
        });
    }

    [TestCase(600_000L, true)]
    [TestCase(600_001L, false)]
    public async Task DeviceAuthorizationBoundaryAndExpiryAreStable(
        long elapsedMilliseconds,
        bool isExactBoundary)
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(
            database,
            $"desktop-expiry-{elapsedMilliseconds}",
            $"desktop-expiry-{elapsedMilliseconds}@example.com");
        var issuedAt = SignupTime.AddDays(1);
        DeviceAuthorizationValues authorization;
        using (var issuingFactory = new LicensingWebApplicationFactory(database, issuedAt))
        using (var issuingClient = issuingFactory.CreateApiClient(signup.UserId))
        {
            authorization = await StartAuthorizationAsync(
                issuingClient,
                Guid.CreateVersion7());
        }

        using var expiredFactory = new LicensingWebApplicationFactory(
            database,
            issuedAt.AddMilliseconds(elapsedMilliseconds));
        using var expiredClient = expiredFactory.CreateApiClient(signup.UserId);
        using var approvalResponse = await expiredClient.GetAsync(
            $"/desktop/v1/device/approval?user_code="
                + Uri.EscapeDataString(authorization.UserCode));
        using var tokenResponse = await PollDeviceTokenAsync(
            expiredClient,
            authorization.DeviceCode);
        using var tokenBody = JsonDocument.Parse(
            await tokenResponse.Content.ReadAsStringAsync());
        await using var verificationContext = database.CreateContext();
        Assert.Multiple(() =>
        {
            Assert.That(
                approvalResponse.StatusCode,
                Is.EqualTo(
                    isExactBoundary
                        ? HttpStatusCode.OK
                        : HttpStatusCode.NotFound));
            Assert.That(tokenResponse.StatusCode, Is.EqualTo(HttpStatusCode.BadRequest));
            Assert.That(
                tokenBody.RootElement.GetProperty("error").GetString(),
                Is.EqualTo(isExactBoundary ? "authorization_pending" : "expired_token"));
            Assert.That(verificationContext.DeviceActivations.Count(), Is.Zero);
        });
    }

    [TestCase(7_775_999_999L, true)]
    [TestCase(7_776_000_000L, false)]
    [TestCase(7_776_000_001L, false)]
    public async Task RefreshTokenNinetyDayExpiryBoundaryIsStable(
        long elapsedMilliseconds,
        bool expectedToBeValid)
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(
            database,
            $"desktop-refresh-expiry-{elapsedMilliseconds}",
            $"desktop-refresh-expiry-{elapsedMilliseconds}@example.com");
        var issuedAt = SignupTime.AddDays(1);
        await ProjectActiveSubscriptionAsync(
            database,
            signup.PersonalBillingAccountId,
            currentPeriodEndsAt: issuedAt.AddDays(365));
        DesktopTokenValues tokens;
        using (var issuingFactory = new LicensingWebApplicationFactory(database, issuedAt))
        using (var issuingClient = issuingFactory.CreateApiClient(signup.UserId))
        {
            tokens = await AuthorizeAndIssueTokensAsync(
                issuingClient,
                signup.SeatId,
                Guid.CreateVersion7());
        }

        using var expiredFactory = new LicensingWebApplicationFactory(
            database,
            issuedAt.AddMilliseconds(elapsedMilliseconds));
        using var expiredClient = expiredFactory.CreateApiClient(signup.UserId);
        using var response = await RefreshAsync(expiredClient, tokens.RefreshToken);
        using var body = JsonDocument.Parse(await response.Content.ReadAsStringAsync());
        if (expectedToBeValid)
        {
            Assert.Multiple(() =>
            {
                Assert.That(response.StatusCode, Is.EqualTo(HttpStatusCode.OK));
                Assert.That(
                    body.RootElement.GetProperty("access_token").GetString(),
                    Is.Not.Empty);
            });
        }
        else
        {
            Assert.Multiple(() =>
            {
                Assert.That(response.StatusCode, Is.EqualTo(HttpStatusCode.BadRequest));
                Assert.That(
                    body.RootElement.GetProperty("error").GetString(),
                    Is.EqualTo("invalid_grant"));
            });
        }
    }

    [Test]
    public async Task RefreshAfterTrialExpiryRevokesTheDesktopSessionAndTokenChain()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(
            database,
            "desktop-entitlement-expiry",
            "desktop-entitlement-expiry@example.com");
        DesktopTokenValues tokens;
        using (var issuingFactory = new LicensingWebApplicationFactory(
            database,
            SignupTime.AddDays(1)))
        using (var issuingClient = issuingFactory.CreateApiClient(signup.UserId))
        {
            tokens = await AuthorizeAndIssueTokensAsync(
                issuingClient,
                signup.SeatId,
                Guid.CreateVersion7());
        }

        var expiredAt = SignupTime.AddDays(30);
        using var expiredFactory = new LicensingWebApplicationFactory(database, expiredAt);
        using var expiredClient = expiredFactory.CreateApiClient(signup.UserId);
        using var response = await RefreshAsync(expiredClient, tokens.RefreshToken);
        using var body = JsonDocument.Parse(await response.Content.ReadAsStringAsync());

        await using var verificationContext = database.CreateContext();
        var authorizationStatuses = await verificationContext
            .Set<OpenIddictEntityFrameworkCoreAuthorization<Guid>>()
            .Select(authorization => authorization.Status)
            .ToArrayAsync();
        var tokenStatuses = await verificationContext
            .Set<OpenIddictEntityFrameworkCoreToken<Guid>>()
            .Select(token => token.Status)
            .ToArrayAsync();
        var session = await verificationContext.DesktopDeviceSessions.SingleAsync();
        Assert.Multiple(() =>
        {
            Assert.That(response.StatusCode, Is.EqualTo(HttpStatusCode.BadRequest));
            Assert.That(
                body.RootElement.GetProperty("error").GetString(),
                Is.EqualTo("invalid_grant"));
            Assert.That(
                authorizationStatuses,
                Has.All.EqualTo(OpenIddictConstants.Statuses.Revoked));
            Assert.That(tokenStatuses, Is.Not.Empty);
            Assert.That(
                tokenStatuses,
                Has.All.EqualTo(OpenIddictConstants.Statuses.Revoked));
            Assert.That(session.RevokedAt, Is.EqualTo(expiredAt));
        });
    }

    [Test]
    public async Task SeatRemovalRetainsDeviceUntilRefreshRevokesTheDesktopTokenChain()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(
            database,
            "desktop-seat-removal-owner",
            "owner@example.com");
        var member = await SignUpAsync(
            database,
            "desktop-seat-removal-member",
            "member@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Desktop Seat Removal",
            SignupTime.AddDays(1));
        var memberSeat = await AddOrganizationMemberAsync(
            database,
            organization,
            member.UserId,
            OrganizationRole.Member);
        var observedAt = SignupTime.AddDays(2);
        using var factory = new LicensingWebApplicationFactory(database, observedAt);
        using var ownerClient = factory.CreateApiClient(owner.UserId);
        using var memberClient = factory.CreateApiClient(member.UserId);
        var tokens = await AuthorizeAndIssueTokensAsync(
            memberClient,
            memberSeat.SeatId,
            Guid.CreateVersion7());

        using var removal = await ChangeOrganizationSeatAsync(
            ownerClient,
            organization.OrganizationId,
            memberSeat.MembershipId,
            assigned: false);
        using var deviceListResponse = await memberClient.GetAsync(
            $"/api/seats/{memberSeat.SeatId}/devices");
        var deviceList = await deviceListResponse.Content
            .ReadFromJsonAsync<DeviceListResponse>();
        await using (var retainedContext = database.CreateContext())
        {
            var retainedActivation = await retainedContext.DeviceActivations.SingleAsync();
            var retainedSession = await retainedContext.DesktopDeviceSessions.SingleAsync();
            Assert.Multiple(() =>
            {
                Assert.That(removal.StatusCode, Is.EqualTo(HttpStatusCode.OK));
                Assert.That(deviceListResponse.StatusCode, Is.EqualTo(HttpStatusCode.OK));
                Assert.That(deviceList!.ActiveDevices, Has.Count.EqualTo(1));
                Assert.That(
                    deviceList.ActiveDevices[0].ActivationId,
                    Is.EqualTo(retainedActivation.Id));
                Assert.That(retainedActivation.RevokedAt, Is.Null);
                Assert.That(retainedSession.RevokedAt, Is.Null);
            });
        }

        using var refresh = await RefreshAsync(memberClient, tokens.RefreshToken);
        using var refreshBody = JsonDocument.Parse(
            await refresh.Content.ReadAsStringAsync());
        await using var verificationContext = database.CreateContext();
        var activation = await verificationContext.DeviceActivations.SingleAsync();
        var session = await verificationContext.DesktopDeviceSessions.SingleAsync();
        var authorizationStatuses = await verificationContext
            .Set<OpenIddictEntityFrameworkCoreAuthorization<Guid>>()
            .Select(authorization => authorization.Status)
            .ToArrayAsync();
        var tokenStatuses = await verificationContext
            .Set<OpenIddictEntityFrameworkCoreToken<Guid>>()
            .Select(token => token.Status)
            .ToArrayAsync();
        Assert.Multiple(() =>
        {
            Assert.That(refresh.StatusCode, Is.EqualTo(HttpStatusCode.BadRequest));
            Assert.That(
                refreshBody.RootElement.GetProperty("error").GetString(),
                Is.EqualTo("invalid_grant"));
            Assert.That(activation.RevokedAt, Is.Null);
            Assert.That(session.RevokedAt, Is.EqualTo(observedAt));
            Assert.That(
                authorizationStatuses,
                Has.All.EqualTo(OpenIddictConstants.Statuses.Revoked));
            Assert.That(tokenStatuses, Is.Not.Empty);
            Assert.That(tokenStatuses, Has.All.EqualTo(OpenIddictConstants.Statuses.Revoked));
        });
    }

    [Test]
    public async Task RestoringSeatBeforeRejectedRefreshKeepsTheDesktopSessionUsable()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(
            database,
            "desktop-seat-restore-owner",
            "owner@example.com");
        var member = await SignUpAsync(
            database,
            "desktop-seat-restore-member",
            "member@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Desktop Seat Restore",
            SignupTime.AddDays(1));
        var memberSeat = await AddOrganizationMemberAsync(
            database,
            organization,
            member.UserId,
            OrganizationRole.Member);
        using var factory = new LicensingWebApplicationFactory(
            database,
            SignupTime.AddDays(2));
        using var ownerClient = factory.CreateApiClient(owner.UserId);
        using var memberClient = factory.CreateApiClient(member.UserId);
        var tokens = await AuthorizeAndIssueTokensAsync(
            memberClient,
            memberSeat.SeatId,
            Guid.CreateVersion7());

        using var removal = await ChangeOrganizationSeatAsync(
            ownerClient,
            organization.OrganizationId,
            memberSeat.MembershipId,
            assigned: false);
        using var restoration = await ChangeOrganizationSeatAsync(
            ownerClient,
            organization.OrganizationId,
            memberSeat.MembershipId,
            assigned: true);
        using var refresh = await RefreshAsync(memberClient, tokens.RefreshToken);
        _ = await ReadTokenResponseAsync(refresh);
        using var deviceListResponse = await memberClient.GetAsync(
            $"/api/seats/{memberSeat.SeatId}/devices");
        var deviceList = await deviceListResponse.Content
            .ReadFromJsonAsync<DeviceListResponse>();

        await using var verificationContext = database.CreateContext();
        var seat = await verificationContext.Seats.SingleAsync(
            candidate => candidate.Id == memberSeat.SeatId);
        var activation = await verificationContext.DeviceActivations.SingleAsync();
        var session = await verificationContext.DesktopDeviceSessions.SingleAsync();
        Assert.Multiple(() =>
        {
            Assert.That(removal.StatusCode, Is.EqualTo(HttpStatusCode.OK));
            Assert.That(restoration.StatusCode, Is.EqualTo(HttpStatusCode.OK));
            Assert.That(refresh.StatusCode, Is.EqualTo(HttpStatusCode.OK));
            Assert.That(deviceListResponse.StatusCode, Is.EqualTo(HttpStatusCode.OK));
            Assert.That(deviceList!.ActiveDevices, Has.Count.EqualTo(1));
            Assert.That(deviceList.ActiveDevices[0].ActivationId, Is.EqualTo(activation.Id));
            Assert.That(seat.ProductAccessEnabled, Is.True);
            Assert.That(activation.RevokedAt, Is.Null);
            Assert.That(session.RevokedAt, Is.Null);
        });
    }

    [TestCase(null)]
    [TestCase("unknown-desktop")]
    public async Task TokenEndpointsRejectAMissingOrUnknownDesktopClient(string? clientId)
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(
            database,
            $"desktop-client-{clientId ?? "missing"}",
            $"desktop-client-{clientId ?? "missing"}@example.com");
        using var factory = new LicensingWebApplicationFactory(database, SignupTime.AddDays(1));
        using var client = factory.CreateApiClient(signup.UserId);
        var authorization = await StartAuthorizationAsync(client, Guid.CreateVersion7());
        using var deviceResponse = await PollDeviceTokenAsync(
            client,
            authorization.DeviceCode,
            clientId);
        using var deviceBody = JsonDocument.Parse(
            await deviceResponse.Content.ReadAsStringAsync());

        var tokens = await AuthorizeAndIssueTokensAsync(
            client,
            signup.SeatId,
            Guid.CreateVersion7());
        using var refreshResponse = await RefreshAsync(
            client,
            tokens.RefreshToken,
            clientId);
        using var refreshBody = JsonDocument.Parse(
            await refreshResponse.Content.ReadAsStringAsync());
        Assert.Multiple(() =>
        {
            Assert.That(deviceResponse.StatusCode, Is.EqualTo(HttpStatusCode.Unauthorized));
            Assert.That(
                deviceBody.RootElement.GetProperty("error").GetString(),
                Is.EqualTo("invalid_client"));
            Assert.That(refreshResponse.StatusCode, Is.EqualTo(HttpStatusCode.Unauthorized));
            Assert.That(
                refreshBody.RootElement.GetProperty("error").GetString(),
                Is.EqualTo("invalid_client"));
        });

        using var validRefresh = await RefreshAsync(client, tokens.RefreshToken);
        Assert.That(validRefresh.StatusCode, Is.EqualTo(HttpStatusCode.OK));
    }

    [Test]
    public async Task SelectingOrganizationSeatBindsActivationAndDesktopTokenClaims()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(
            database,
            "desktop-organization-claims",
            "desktop-organization-claims@example.com");
        await ProjectActiveSubscriptionAsync(database, signup.PersonalBillingAccountId);
        var organization = await CreateOrganizationAsync(
            database,
            signup.UserId,
            "Claims Team",
            SignupTime.AddDays(1));
        using var factory = new LicensingWebApplicationFactory(
            database,
            SignupTime.AddDays(2),
            desktopIssuer: "https://licenses.example.com/");
        using var client = factory.CreateApiClient(signup.UserId);
        client.DefaultRequestHeaders.Host = "internal-api.local";
        var installationId = Guid.CreateVersion7();
        var authorization = await StartAuthorizationAsync(client, installationId);

        using var approval = await ApproveAsync(
            client,
            authorization.UserCode,
            organization.SeatId);
        using var tokenResponse = await PollDeviceTokenAsync(
            client,
            authorization.DeviceCode);
        var tokens = await ReadTokenResponseAsync(tokenResponse);

        await using var verificationContext = database.CreateContext();
        var activation = await verificationContext.DeviceActivations.SingleAsync();
        using var certificate = X509CertificateLoader.LoadPkcs12(
            Convert.FromBase64String(
                TestDataProtectionCertificate.EncodedDesktopCertificate),
            TestDataProtectionCertificate.Password,
            X509KeyStorageFlags.EphemeralKeySet);
        var validation = await new JsonWebTokenHandler().ValidateTokenAsync(
            tokens.AccessToken,
            new TokenValidationParameters
            {
                ValidIssuer = "https://licenses.example.com/",
                ValidAudience = "styrhous-desktop-api",
                IssuerSigningKey = new X509SecurityKey(certificate),
                TokenDecryptionKey = new X509SecurityKey(certificate),
                ValidateLifetime = false,
            });
        var claims = validation.ClaimsIdentity?.Claims.ToArray() ?? [];
        Assert.Multiple(() =>
        {
            Assert.That(approval.StatusCode, Is.EqualTo(HttpStatusCode.NoContent));
            Assert.That(tokenResponse.StatusCode, Is.EqualTo(HttpStatusCode.OK));
            Assert.That(activation.InstallationId, Is.EqualTo(installationId));
            Assert.That(activation.SeatId, Is.EqualTo(organization.SeatId));
            Assert.That(validation.IsValid, Is.True, validation.Exception?.ToString());
            Assert.That(
                claims.Single(claim => claim.Type == "sub").Value,
                Is.EqualTo(signup.UserId.ToString()));
            Assert.That(
                claims.Single(claim => claim.Type == "styrhous:seat_id").Value,
                Is.EqualTo(organization.SeatId.ToString()));
            Assert.That(
                claims.Single(claim => claim.Type == "styrhous:billing_account_id").Value,
                Is.EqualTo(organization.BillingAccountId.ToString()));
            Assert.That(
                claims.Single(claim => claim.Type == "styrhous:activation_id").Value,
                Is.EqualTo(activation.Id.ToString()));
            Assert.That(
                claims.Single(claim => claim.Type == "aud").Value,
                Is.EqualTo("styrhous-desktop-api"));
        });
    }

    [Test]
    public async Task RefreshReissuesEveryLicensingClaimFromFreshEntitlementState()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(
            database,
            "desktop-refresh-claims",
            "desktop-refresh-claims@example.com");
        var installationId = Guid.CreateVersion7();
        using var factory = new LicensingWebApplicationFactory(
            database,
            SignupTime.AddDays(1));
        using var client = factory.CreateApiClient(signup.UserId);
        var issued = await AuthorizeAndIssueTokensAsync(
            client,
            signup.SeatId,
            installationId);
        var initialClaims = await ValidateAccessTokenAsync(issued.AccessToken);
        await ProjectActiveSubscriptionAsync(database, signup.PersonalBillingAccountId);

        using var refreshResponse = await RefreshAsync(client, issued.RefreshToken);
        var refreshed = await ReadTokenResponseAsync(refreshResponse);
        var refreshedClaims = await ValidateAccessTokenAsync(refreshed.AccessToken);
        await using var context = database.CreateContext();
        var activationId = await context.DeviceActivations
            .Select(activation => activation.Id)
            .SingleAsync();

        Assert.Multiple(() =>
        {
            Assert.That(refreshResponse.StatusCode, Is.EqualTo(HttpStatusCode.OK));
            Assert.That(
                ClaimValue(initialClaims, DesktopProtocolConstants.Claims.EntitlementState),
                Is.EqualTo("trial"));
            Assert.That(
                ClaimValue(
                    initialClaims,
                    DesktopProtocolConstants.Claims.EntitlementReasonCode),
                Is.EqualTo(EntitlementReasonCodes.ActiveTrial));
            Assert.That(
                ClaimValue(refreshedClaims, "sub"),
                Is.EqualTo(signup.UserId.ToString()));
            Assert.That(
                ClaimValue(refreshedClaims, DesktopProtocolConstants.Claims.InstallationId),
                Is.EqualTo(installationId.ToString()));
            Assert.That(
                ClaimValue(refreshedClaims, DesktopProtocolConstants.Claims.SeatId),
                Is.EqualTo(signup.SeatId.ToString()));
            Assert.That(
                ClaimValue(refreshedClaims, DesktopProtocolConstants.Claims.BillingAccountId),
                Is.EqualTo(signup.PersonalBillingAccountId.ToString()));
            Assert.That(
                ClaimValue(refreshedClaims, DesktopProtocolConstants.Claims.ActivationId),
                Is.EqualTo(activationId.ToString()));
            Assert.That(
                ClaimValue(refreshedClaims, DesktopProtocolConstants.Claims.EntitlementState),
                Is.EqualTo("commercial"));
            Assert.That(
                ClaimValue(
                    refreshedClaims,
                    DesktopProtocolConstants.Claims.EntitlementReasonCode),
                Is.EqualTo(EntitlementReasonCodes.ActiveSubscription));
            Assert.That(
                ClaimValue(refreshedClaims, "aud"),
                Is.EqualTo(DesktopProtocolConstants.Resource));
        });
    }

    [Test]
    public async Task ConcurrentSeatDecisionsCommitOnlyTheAcceptedActivation()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(
            database,
            "desktop-concurrent-user",
            "desktop-concurrent@example.com");
        await ProjectActiveSubscriptionAsync(database, signup.PersonalBillingAccountId);

        var organization = await CreateOrganizationAsync(
            database,
            signup.UserId,
            "Concurrent Team",
            SignupTime.AddDays(1));
        var gate = new DatabaseCommandGate();
        var interceptor = new DatabaseCommandGateInterceptor(
            gate,
            "UPDATE user_accounts");
        using var factory = new LicensingWebApplicationFactory(
            database,
            SignupTime.AddDays(2),
            interceptors: [interceptor]);
        using var firstClient = factory.CreateApiClient(signup.UserId);
        using var secondClient = factory.CreateApiClient(signup.UserId);
        var authorization = await StartAuthorizationAsync(
            firstClient,
            Guid.CreateVersion7());
        using var firstRequest = await CreateApprovalRequestAsync(
            firstClient,
            authorization.UserCode,
            signup.SeatId);
        using var secondRequest = await CreateApprovalRequestAsync(
            secondClient,
            authorization.UserCode,
            organization.SeatId);

        var firstAttempt = SendApprovalAttemptAsync(firstClient, firstRequest);
        await gate.WaitUntilReachedAsync();
        var secondAttempt = await SendApprovalAttemptAsync(secondClient, secondRequest);
        gate.Release();
        var completedFirstAttempt = await firstAttempt;

        using (secondAttempt.Response)
        using (completedFirstAttempt.Response)
        {
            using var conflictBody = JsonDocument.Parse(
                await completedFirstAttempt.Response!.Content.ReadAsStringAsync());
            Assert.Multiple(() =>
            {
                Assert.That(secondAttempt.Exception, Is.Null);
                Assert.That(completedFirstAttempt.Exception, Is.Null);
                Assert.That(
                    secondAttempt.Response?.StatusCode,
                    Is.EqualTo(HttpStatusCode.NoContent));
                Assert.That(
                    completedFirstAttempt.Response.StatusCode,
                    Is.EqualTo(HttpStatusCode.Conflict));
                Assert.That(
                    PropertyNames(conflictBody.RootElement),
                    Is.EqualTo(["reasonCode"]));
                Assert.That(
                    conflictBody.RootElement.GetProperty("reasonCode").GetString(),
                    Is.EqualTo(
                        DesktopDeviceAuthorizationReasonCodes.ConcurrentModification));
            });
        }

        await using var verificationContext = database.CreateContext();
        var activation = await verificationContext.DeviceActivations.SingleAsync();
        var session = await verificationContext.DesktopDeviceSessions.SingleAsync();
        Assert.Multiple(() =>
        {
            Assert.That(activation.SeatId, Is.EqualTo(organization.SeatId));
            Assert.That(session.ActivationId, Is.EqualTo(activation.Id));
        });

        using var tokenResponse = await PollDeviceTokenAsync(
            firstClient,
            authorization.DeviceCode);
        Assert.That(tokenResponse.StatusCode, Is.EqualTo(HttpStatusCode.OK));
    }

    [Test]
    public async Task ConcurrentDenialRollsBackAnUnacceptedActivation()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(
            database,
            "desktop-concurrent-denial",
            "desktop-concurrent-denial@example.com");
        var gate = new DatabaseCommandGate();
        var interceptor = new DatabaseCommandGateInterceptor(
            gate,
            "UPDATE user_accounts");
        using var factory = new LicensingWebApplicationFactory(
            database,
            SignupTime.AddDays(1),
            interceptors: [interceptor]);
        using var approvalClient = factory.CreateApiClient(signup.UserId);
        using var denialClient = factory.CreateApiClient(signup.UserId);
        var authorization = await StartAuthorizationAsync(
            approvalClient,
            Guid.CreateVersion7());
        using var approvalRequest = await CreateApprovalRequestAsync(
            approvalClient,
            authorization.UserCode,
            signup.SeatId);
        using var denialRequest = await CreateDenialRequestAsync(
            denialClient,
            authorization.UserCode);

        var approvalAttempt = SendApprovalAttemptAsync(approvalClient, approvalRequest);
        await gate.WaitUntilReachedAsync();
        var denialAttempt = await SendApprovalAttemptAsync(denialClient, denialRequest);
        gate.Release();
        var completedApprovalAttempt = await approvalAttempt;

        using (denialAttempt.Response)
        using (completedApprovalAttempt.Response)
        {
            using var conflictBody = JsonDocument.Parse(
                await completedApprovalAttempt.Response!.Content.ReadAsStringAsync());
            Assert.Multiple(() =>
            {
                Assert.That(denialAttempt.Exception, Is.Null);
                Assert.That(completedApprovalAttempt.Exception, Is.Null);
                Assert.That(
                    denialAttempt.Response?.StatusCode,
                    Is.EqualTo(HttpStatusCode.NoContent));
                Assert.That(
                    completedApprovalAttempt.Response.StatusCode,
                    Is.EqualTo(HttpStatusCode.Conflict));
                Assert.That(
                    PropertyNames(conflictBody.RootElement),
                    Is.EqualTo(["reasonCode"]));
                Assert.That(
                    conflictBody.RootElement.GetProperty("reasonCode").GetString(),
                    Is.EqualTo(
                        DesktopDeviceAuthorizationReasonCodes.ConcurrentModification));
            });
        }

        await using (var verificationContext = database.CreateContext())
        {
            Assert.Multiple(() =>
            {
                Assert.That(verificationContext.DeviceActivations.Count(), Is.Zero);
                Assert.That(verificationContext.DesktopDeviceSessions.Count(), Is.Zero);
            });
        }

        using var tokenResponse = await PollDeviceTokenAsync(
            denialClient,
            authorization.DeviceCode);
        using var tokenBody = JsonDocument.Parse(
            await tokenResponse.Content.ReadAsStringAsync());
        Assert.Multiple(() =>
        {
            Assert.That(tokenResponse.StatusCode, Is.EqualTo(HttpStatusCode.BadRequest));
            Assert.That(
                tokenBody.RootElement.GetProperty("error").GetString(),
                Is.EqualTo("access_denied"));
        });
    }

    [Test]
    public async Task ConcurrentApprovalRejectsAnAlreadyStartedDenial()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(
            database,
            "desktop-concurrent-approval",
            "desktop-concurrent-approval@example.com");
        DeviceAuthorizationValues authorization;
        using (var initiatingFactory = new LicensingWebApplicationFactory(
            database,
            SignupTime.AddDays(1)))
        using (var initiatingClient = initiatingFactory.CreateApiClient(signup.UserId))
        {
            authorization = await StartAuthorizationAsync(
                initiatingClient,
                Guid.CreateVersion7());
        }

        var gate = new DatabaseCommandGate();
        var interceptor = new DatabaseCommandGateInterceptor(
            gate,
            "UPDATE \"OpenIddictTokens\"");
        using var decisionFactory = new LicensingWebApplicationFactory(
            database,
            SignupTime.AddDays(1),
            interceptors: [interceptor]);
        using var denialClient = decisionFactory.CreateApiClient(signup.UserId);
        using var approvalClient = decisionFactory.CreateApiClient(signup.UserId);
        using var denialRequest = await CreateDenialRequestAsync(
            denialClient,
            authorization.UserCode);
        using var approvalRequest = await CreateApprovalRequestAsync(
            approvalClient,
            authorization.UserCode,
            signup.SeatId);

        var denialAttempt = SendApprovalAttemptAsync(denialClient, denialRequest);
        await gate.WaitUntilReachedAsync();
        var approvalAttempt = await SendApprovalAttemptAsync(
            approvalClient,
            approvalRequest);
        gate.Release();
        var completedDenialAttempt = await denialAttempt;

        using (approvalAttempt.Response)
        using (completedDenialAttempt.Response)
        {
            using var conflictBody = JsonDocument.Parse(
                await completedDenialAttempt.Response!.Content.ReadAsStringAsync());
            Assert.Multiple(() =>
            {
                Assert.That(approvalAttempt.Exception, Is.Null);
                Assert.That(completedDenialAttempt.Exception, Is.Null);
                Assert.That(
                    approvalAttempt.Response?.StatusCode,
                    Is.EqualTo(HttpStatusCode.NoContent));
                Assert.That(
                    completedDenialAttempt.Response.StatusCode,
                    Is.EqualTo(HttpStatusCode.Conflict));
                Assert.That(
                    PropertyNames(conflictBody.RootElement),
                    Is.EqualTo(["reasonCode"]));
                Assert.That(
                    conflictBody.RootElement.GetProperty("reasonCode").GetString(),
                    Is.EqualTo(
                        DesktopDeviceAuthorizationReasonCodes.ConcurrentModification));
            });
        }

        await using (var verificationContext = database.CreateContext())
        {
            var session = await verificationContext.DesktopDeviceSessions.SingleAsync();
            var redeemedUserCodeCount = await verificationContext
                .Set<OpenIddictEntityFrameworkCoreToken<Guid>>()
                .CountAsync(token => token.Authorization != null
                    && token.Authorization.Id == session.AuthorizationId
                    && token.Status == OpenIddictConstants.Statuses.Redeemed);
            Assert.Multiple(() =>
            {
                Assert.That(verificationContext.DeviceActivations.Count(), Is.EqualTo(1));
                Assert.That(verificationContext.DesktopDeviceSessions.Count(), Is.EqualTo(1));
                Assert.That(
                    redeemedUserCodeCount,
                    Is.EqualTo(1),
                    "The approved authorization must contain its redeemed user-code token.");
            });
        }

        using var tokenResponse = await PollDeviceTokenAsync(
            approvalClient,
            authorization.DeviceCode);
        Assert.That(tokenResponse.StatusCode, Is.EqualTo(HttpStatusCode.OK));
    }

    [Test]
    public async Task StaleReplacementRevokesTheReplacedDevicesRefreshSession()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(
            database,
            "desktop-stale-session",
            "desktop-stale-session@example.com");
        var issuedAt = SignupTime.AddDays(1);
        DesktopTokenValues oldTokens;
        Guid oldActivationId;
        Guid oldAuthorizationId;
        using (var issuingFactory = new LicensingWebApplicationFactory(database, issuedAt))
        using (var issuingClient = issuingFactory.CreateApiClient(signup.UserId))
        {
            oldTokens = await AuthorizeAndIssueTokensAsync(
                issuingClient,
                signup.SeatId,
                Guid.CreateVersion7());
        }

        await using (var sessionContext = database.CreateContext())
        {
            var oldSession = await sessionContext.DesktopDeviceSessions.SingleAsync();
            oldActivationId = oldSession.ActivationId;
            oldAuthorizationId = oldSession.AuthorizationId;
        }

        await ActivateAsync(database, signup, 2, issuedAt.AddMinutes(1));
        await ActivateAsync(database, signup, 3, issuedAt.AddMinutes(2));
        var replacementTime = issuedAt.Add(DeviceActivation.StaleAfter).AddMinutes(3);
        using var replacementFactory = new LicensingWebApplicationFactory(
            database,
            replacementTime);
        using var replacementClient = replacementFactory.CreateApiClient(signup.UserId);
        var replacementAuthorization = await StartAuthorizationAsync(
            replacementClient,
            Guid.CreateVersion7());
        using var replacementApproval = await ApproveAsync(
            replacementClient,
            replacementAuthorization.UserCode,
            signup.SeatId);

        await using (var verificationContext = database.CreateContext())
        {
            var oldActivation = await verificationContext.DeviceActivations
                .SingleAsync(activation => activation.Id == oldActivationId);
            var oldSession = await verificationContext.DesktopDeviceSessions
                .SingleAsync(session => session.AuthorizationId == oldAuthorizationId);
            var oldTokenStatuses = await verificationContext
                .Set<OpenIddictEntityFrameworkCoreToken<Guid>>()
                .Where(token => token.Authorization != null
                    && token.Authorization.Id == oldAuthorizationId)
                .Select(token => token.Status)
                .ToArrayAsync();
            Assert.Multiple(() =>
            {
                Assert.That(
                    replacementApproval.StatusCode,
                    Is.EqualTo(HttpStatusCode.NoContent));
                Assert.That(
                    oldActivation.RevocationReason,
                    Is.EqualTo(DeviceRevocationReason.StaleDeviceReplaced));
                Assert.That(oldSession.RevokedAt, Is.EqualTo(replacementTime));
                Assert.That(oldTokenStatuses, Is.Not.Empty);
                Assert.That(
                    oldTokenStatuses,
                    Has.All.EqualTo(OpenIddictConstants.Statuses.Revoked));
            });
        }

        using var oldRefreshResponse = await RefreshAsync(
            replacementClient,
            oldTokens.RefreshToken);
        using var replacementTokenResponse = await PollDeviceTokenAsync(
            replacementClient,
            replacementAuthorization.DeviceCode);
        Assert.Multiple(() =>
        {
            Assert.That(oldRefreshResponse.StatusCode, Is.EqualTo(HttpStatusCode.BadRequest));
            Assert.That(replacementTokenResponse.StatusCode, Is.EqualTo(HttpStatusCode.OK));
        });
    }

    [Test]
    public async Task DesktopAccessTokenIssuesAnOfflineLeaseAndManagesItsOwnDevice()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(
            database,
            "desktop-offline-lease",
            "desktop-offline-lease@example.com");
        var observedAt = SignupTime.AddDays(1);
        var installationId = Guid.CreateVersion7();
        var previousCertificate =
            TestDataProtectionCertificate.CreateDesktopProtocolRsaCertificate(
                SignupTime.AddYears(1),
                SignupTime.AddDays(-1));
        var currentCertificate =
            TestDataProtectionCertificate.CreateDesktopProtocolRsaCertificate(
                SignupTime.AddYears(2),
                SignupTime.AddDays(-1));
        using var factory = new LicensingWebApplicationFactory(
            database,
            observedAt,
            desktopCertificateRing: new TestCertificateRing(
                currentCertificate,
                [previousCertificate]));
        using var client = factory.CreateApiClient(signup.UserId);
        var tokens = await AuthorizeAndIssueTokensAsync(
            client,
            signup.SeatId,
            installationId);
        client.DefaultRequestHeaders.Authorization =
            new System.Net.Http.Headers.AuthenticationHeaderValue("Bearer", tokens.AccessToken);

        using var keysResponse = await client.GetAsync(DesktopProtocolConstants.KeysPath);
        using var keys = JsonDocument.Parse(await keysResponse.Content.ReadAsStringAsync());
        using var leaseResponse = await client.PostAsync(
            DesktopProtocolConstants.EntitlementPath,
            content: null);
        using var leaseBody = JsonDocument.Parse(await leaseResponse.Content.ReadAsStringAsync());
        using var devicesResponse = await client.GetAsync(DesktopProtocolConstants.DevicesPath);
        var devices = await devicesResponse.Content.ReadFromJsonAsync<DeviceListResponse>();

        var lease = leaseBody.RootElement.GetProperty("lease").GetString()!;
        var parsedLease = new JsonWebTokenHandler().ReadJsonWebToken(lease);
        var keySet = keys.RootElement.GetProperty("keys");
        var currentKeyId = keySet[0].GetProperty("kid").GetString();
        var currentKey = keySet.EnumerateArray().Single(key =>
            string.Equals(
                key.GetProperty("kid").GetString(),
                parsedLease.Kid,
                StringComparison.Ordinal));
        var validation = await ValidateLeaseWithJsonWebKeyAsync(lease, currentKey);
        var leaseClaims = validation.ClaimsIdentity?.Claims.ToArray() ?? [];
        var activationId = devices!.ActiveDevices.Single().ActivationId;

        using var deleteResponse = await client.DeleteAsync(
            $"{DesktopProtocolConstants.DevicesPath}/{devices!.ActiveDevices.Single().ActivationId}");
        using var rejectedLease = await client.PostAsync(
            DesktopProtocolConstants.EntitlementPath,
            content: null);
        var rejectedLeaseBody =
            await rejectedLease.Content.ReadFromJsonAsync<DesktopProtocolErrorResponse>();

        Assert.Multiple(() =>
        {
            Assert.That(keysResponse.StatusCode, Is.EqualTo(HttpStatusCode.OK));
            Assert.That(keySet.GetArrayLength(), Is.EqualTo(2));
            Assert.That(
                keySet.EnumerateArray().Select(key => key.GetProperty("kid").GetString()),
                Is.Unique);
            Assert.That(parsedLease.Kid, Is.EqualTo(currentKeyId));
            Assert.That(leaseResponse.StatusCode, Is.EqualTo(HttpStatusCode.OK));
            Assert.That(
                leaseBody.RootElement.GetProperty("reasonCode").GetString(),
                Is.EqualTo("offline_lease_issued"));
            Assert.That(
                leaseBody.RootElement.GetProperty("expiresAt").GetDateTimeOffset(),
                Is.EqualTo(observedAt.AddDays(7)));
            Assert.That(
                leaseBody.RootElement.GetProperty("refreshAfter").GetDateTimeOffset(),
                Is.EqualTo(observedAt.AddHours(24)));
            Assert.That(validation.IsValid, Is.True, validation.Exception?.ToString());
            Assert.That(ClaimValue(leaseClaims, "schema_version"), Is.EqualTo("1"));
            Assert.That(ClaimValue(leaseClaims, "iss"), Is.EqualTo("https://localhost/"));
            Assert.That(
                ClaimValue(leaseClaims, "aud"),
                Is.EqualTo(DesktopProtocolConstants.LeaseAudience));
            Assert.That(ClaimValue(leaseClaims, "sub"), Is.EqualTo(signup.UserId.ToString()));
            Assert.That(
                ClaimValue(leaseClaims, "seat_id"),
                Is.EqualTo(signup.SeatId.ToString()));
            Assert.That(
                ClaimValue(leaseClaims, "billing_account_id"),
                Is.EqualTo(signup.PersonalBillingAccountId.ToString()));
            Assert.That(
                ClaimValue(leaseClaims, "installation_id"),
                Is.EqualTo(installationId.ToString()));
            Assert.That(
                ClaimValue(leaseClaims, "activation_id"),
                Is.EqualTo(activationId.ToString()));
            Assert.That(ClaimValue(leaseClaims, "state"), Is.EqualTo("trial"));
            Assert.That(
                ClaimValue(leaseClaims, "reason_code"),
                Is.EqualTo(EntitlementReasonCodes.ActiveTrial));
            Assert.That(
                ClaimValue(leaseClaims, "valid_from"),
                Is.EqualTo(signup.TrialStartedAt.ToUnixTimeSeconds().ToString(
                    CultureInfo.InvariantCulture)));
            Assert.That(
                ClaimValue(leaseClaims, "valid_until"),
                Is.EqualTo(signup.TrialEndsAt.ToUnixTimeSeconds().ToString(
                    CultureInfo.InvariantCulture)));
            Assert.That(
                ClaimValue(leaseClaims, "iat"),
                Is.EqualTo(observedAt.ToUnixTimeSeconds().ToString(
                    CultureInfo.InvariantCulture)));
            Assert.That(
                ClaimValue(leaseClaims, "nbf"),
                Is.EqualTo(observedAt.ToUnixTimeSeconds().ToString(
                    CultureInfo.InvariantCulture)));
            Assert.That(
                ClaimValue(leaseClaims, "exp"),
                Is.EqualTo(observedAt.AddDays(7).ToUnixTimeSeconds().ToString(
                    CultureInfo.InvariantCulture)));
            Assert.That(
                ClaimValue(leaseClaims, "refresh_after"),
                Is.EqualTo(observedAt.AddHours(24).ToUnixTimeSeconds().ToString(
                    CultureInfo.InvariantCulture)));
            Assert.That(Guid.Parse(ClaimValue(leaseClaims, "jti")).Version, Is.EqualTo(7));
            Assert.That(
                ClaimValue(leaseClaims, "signing_key_id"),
                Is.EqualTo(currentKeyId));
            Assert.That(devicesResponse.StatusCode, Is.EqualTo(HttpStatusCode.OK));
            Assert.That(devices.ActiveDevices, Has.Count.EqualTo(1));
            Assert.That(deleteResponse.StatusCode, Is.EqualTo(HttpStatusCode.OK));
            Assert.That(rejectedLease.StatusCode, Is.EqualTo(HttpStatusCode.Forbidden));
            Assert.That(rejectedLeaseBody?.ReasonCode, Is.EqualTo("device_not_active"));
        });
    }

    [Test]
    public async Task PreviousJsonWebKeyVerifiesALeaseIssuedBeforeRotation()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(
            database,
            "desktop-rotated-lease",
            "desktop-rotated-lease@example.com");
        var observedAt = SignupTime.AddDays(1);
        var previousCertificate =
            TestDataProtectionCertificate.CreateDesktopProtocolRsaCertificate(
                SignupTime.AddYears(1),
                SignupTime.AddDays(-1));
        var currentCertificate =
            TestDataProtectionCertificate.CreateDesktopProtocolRsaCertificate(
                SignupTime.AddYears(2),
                SignupTime.AddDays(-1));
        string oldLease;
        string oldKeyId;
        using (var oldFactory = new LicensingWebApplicationFactory(
            database,
            observedAt,
            desktopCertificateRing: new TestCertificateRing(previousCertificate, [])))
        using (var oldClient = oldFactory.CreateApiClient(signup.UserId))
        {
            var tokens = await AuthorizeAndIssueTokensAsync(
                oldClient,
                signup.SeatId,
                Guid.CreateVersion7());
            oldClient.DefaultRequestHeaders.Authorization =
                new System.Net.Http.Headers.AuthenticationHeaderValue(
                    "Bearer",
                    tokens.AccessToken);
            using var oldLeaseResponse = await oldClient.PostAsync(
                DesktopProtocolConstants.EntitlementPath,
                content: null);
            using var oldLeaseBody = JsonDocument.Parse(
                await oldLeaseResponse.Content.ReadAsStringAsync());
            Assert.That(oldLeaseResponse.StatusCode, Is.EqualTo(HttpStatusCode.OK));
            oldLease = oldLeaseBody.RootElement.GetProperty("lease").GetString()!;
            oldKeyId = new JsonWebTokenHandler().ReadJsonWebToken(oldLease).Kid;
        }

        using var rotatedFactory = new LicensingWebApplicationFactory(
            database,
            observedAt,
            desktopCertificateRing: new TestCertificateRing(
                currentCertificate,
                [previousCertificate]));
        using var rotatedClient = rotatedFactory.CreateApiClient();
        using var keysResponse = await rotatedClient.GetAsync(
            DesktopProtocolConstants.KeysPath);
        using var keys = JsonDocument.Parse(await keysResponse.Content.ReadAsStringAsync());
        var previousKey = keys.RootElement.GetProperty("keys")
            .EnumerateArray()
            .Single(key => string.Equals(
                key.GetProperty("kid").GetString(),
                oldKeyId,
                StringComparison.Ordinal));
        var validation = await ValidateLeaseWithJsonWebKeyAsync(oldLease, previousKey);

        Assert.Multiple(() =>
        {
            Assert.That(keysResponse.StatusCode, Is.EqualTo(HttpStatusCode.OK));
            Assert.That(validation.IsValid, Is.True, validation.Exception?.ToString());
            Assert.That(
                ClaimValue(validation.ClaimsIdentity!.Claims, "signing_key_id"),
                Is.EqualTo(oldKeyId));
        });
    }

    [Test]
    public async Task IneligibleEntitlementRevokesTheDesktopAuthorizationChain()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(
            database,
            "desktop-ineligible-lease",
            "desktop-ineligible-lease@example.com");
        var observedAt = SignupTime.AddDays(1);
        using var factory = new LicensingWebApplicationFactory(database, observedAt);
        using var client = factory.CreateApiClient(signup.UserId);
        var tokens = await AuthorizeAndIssueTokensAsync(
            client,
            signup.SeatId,
            Guid.CreateVersion7());
        await using (var context = database.CreateContext())
        {
            var trial = await context.Trials.SingleAsync();
            Assert.That(trial.TryTerminate(SignupTime.AddHours(12)), Is.True);
            await context.SaveChangesAsync();
        }
        client.DefaultRequestHeaders.Authorization =
            new System.Net.Http.Headers.AuthenticationHeaderValue("Bearer", tokens.AccessToken);

        using var response = await client.PostAsync(
            DesktopProtocolConstants.EntitlementPath,
            content: null);
        using var body = JsonDocument.Parse(await response.Content.ReadAsStringAsync());
        var state = await ReadDesktopSessionStateAsync(database);

        Assert.Multiple(() =>
        {
            Assert.That(response.StatusCode, Is.EqualTo(HttpStatusCode.Forbidden));
            Assert.That(
                body.RootElement.GetProperty("reasonCode").GetString(),
                Is.EqualTo(EntitlementReasonCodes.TrialExpired));
            Assert.That(
                state.AuthorizationStatuses,
                Has.All.EqualTo(OpenIddictConstants.Statuses.Revoked));
            Assert.That(state.TokenStatuses, Is.Not.Empty);
            Assert.That(
                state.TokenStatuses,
                Has.All.EqualTo(OpenIddictConstants.Statuses.Revoked));
            Assert.That(state.SessionRevokedAt, Is.EqualTo(observedAt));
        });
    }

    [Test]
    public async Task SubsecondRemainingEntitlementFailsClosedAndRevokesTheSession()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(
            database,
            "desktop-subsecond-lease",
            "desktop-subsecond-lease@example.com");
        var observedAt = SignupTime.AddDays(1);
        using var factory = new LicensingWebApplicationFactory(database, observedAt);
        using var client = factory.CreateApiClient(signup.UserId);
        var tokens = await AuthorizeAndIssueTokensAsync(
            client,
            signup.SeatId,
            Guid.CreateVersion7());
        await using (var context = database.CreateContext())
        {
            await context.Trials.ExecuteUpdateAsync(setters => setters.SetProperty(
                trial => trial.EndsAt,
                observedAt.AddMilliseconds(500)));
        }
        client.DefaultRequestHeaders.Authorization =
            new System.Net.Http.Headers.AuthenticationHeaderValue(
                "Bearer",
                tokens.AccessToken);

        using var response = await client.PostAsync(
            DesktopProtocolConstants.EntitlementPath,
            content: null);
        var error = await response.Content.ReadFromJsonAsync<DesktopProtocolErrorResponse>();
        var state = await ReadDesktopSessionStateAsync(database);

        Assert.Multiple(() =>
        {
            Assert.That(response.StatusCode, Is.EqualTo(HttpStatusCode.Forbidden));
            Assert.That(
                error?.ReasonCode,
                Is.EqualTo(EntitlementReasonCodes.NoValidEntitlement));
            Assert.That(
                state.AuthorizationStatuses,
                Has.All.EqualTo(OpenIddictConstants.Statuses.Revoked));
            Assert.That(
                state.TokenStatuses,
                Has.All.EqualTo(OpenIddictConstants.Statuses.Revoked));
            Assert.That(state.SessionRevokedAt, Is.EqualTo(observedAt));
        });
    }

    [Test]
    public async Task NearExpiryEntitlementClipsLeaseExpiryAndRefreshTime()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(
            database,
            "desktop-near-expiry-lease",
            "desktop-near-expiry-lease@example.com");
        var observedAt = SignupTime.AddDays(1);
        var entitlementEnd = observedAt.AddSeconds(90);
        using var factory = new LicensingWebApplicationFactory(database, observedAt);
        using var client = factory.CreateApiClient(signup.UserId);
        var tokens = await AuthorizeAndIssueTokensAsync(
            client,
            signup.SeatId,
            Guid.CreateVersion7());
        await using (var context = database.CreateContext())
        {
            await context.Trials.ExecuteUpdateAsync(setters => setters.SetProperty(
                trial => trial.EndsAt,
                entitlementEnd));
        }
        client.DefaultRequestHeaders.Authorization =
            new System.Net.Http.Headers.AuthenticationHeaderValue(
                "Bearer",
                tokens.AccessToken);

        using var response = await client.PostAsync(
            DesktopProtocolConstants.EntitlementPath,
            content: null);
        using var body = JsonDocument.Parse(await response.Content.ReadAsStringAsync());
        var lease = body.RootElement.GetProperty("lease").GetString()!;
        var claims = new JsonWebTokenHandler().ReadJsonWebToken(lease).Claims;
        var expectedUnix = entitlementEnd.ToUnixTimeSeconds().ToString(
            CultureInfo.InvariantCulture);

        Assert.Multiple(() =>
        {
            Assert.That(response.StatusCode, Is.EqualTo(HttpStatusCode.OK));
            Assert.That(
                body.RootElement.GetProperty("expiresAt").GetDateTimeOffset(),
                Is.EqualTo(entitlementEnd));
            Assert.That(
                body.RootElement.GetProperty("refreshAfter").GetDateTimeOffset(),
                Is.EqualTo(entitlementEnd));
            Assert.That(ClaimValue(claims, "valid_until"), Is.EqualTo(expectedUnix));
            Assert.That(ClaimValue(claims, "exp"), Is.EqualTo(expectedUnix));
            Assert.That(ClaimValue(claims, "refresh_after"), Is.EqualTo(expectedUnix));
            Assert.That(
                long.Parse(ClaimValue(claims, "exp"), CultureInfo.InvariantCulture),
                Is.GreaterThan(long.Parse(
                    ClaimValue(claims, "iat"),
                    CultureInfo.InvariantCulture)));
        });
    }

    [Test]
    public async Task EntitlementLossRetriesTheWholeAtomicRevocation()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(
            database,
            "desktop-lease-retry",
            "desktop-lease-retry@example.com");
        var observedAt = SignupTime.AddDays(1);
        DesktopTokenValues tokens;
        using (var issuingFactory = new LicensingWebApplicationFactory(database, observedAt))
        using (var issuingClient = issuingFactory.CreateApiClient(signup.UserId))
        {
            tokens = await AuthorizeAndIssueTokensAsync(
                issuingClient,
                signup.SeatId,
                Guid.CreateVersion7());
        }
        await TerminateTrialAsync(database);
        var interceptor = new ConcurrencyFailureInterceptor(attempt => attempt == 1);
        using var factory = new LicensingWebApplicationFactory(
            database,
            observedAt,
            interceptors: [interceptor]);
        using var client = factory.CreateApiClient();
        client.DefaultRequestHeaders.Authorization =
            new System.Net.Http.Headers.AuthenticationHeaderValue(
                "Bearer",
                tokens.AccessToken);

        using var response = await client.PostAsync(
            DesktopProtocolConstants.EntitlementPath,
            content: null);
        var state = await ReadDesktopSessionStateAsync(database);

        Assert.Multiple(() =>
        {
            Assert.That(response.StatusCode, Is.EqualTo(HttpStatusCode.Forbidden));
            Assert.That(interceptor.AttemptCount, Is.EqualTo(2));
            Assert.That(
                state.AuthorizationStatuses,
                Has.All.EqualTo(OpenIddictConstants.Statuses.Revoked));
            Assert.That(
                state.TokenStatuses,
                Has.All.EqualTo(OpenIddictConstants.Statuses.Revoked));
            Assert.That(state.SessionRevokedAt, Is.EqualTo(observedAt));
        });
    }

    [Test]
    public async Task ExhaustedEntitlementRevocationRetriesRollBackEveryMutation()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(
            database,
            "desktop-lease-rollback",
            "desktop-lease-rollback@example.com");
        var observedAt = SignupTime.AddDays(1);
        DesktopTokenValues tokens;
        using (var issuingFactory = new LicensingWebApplicationFactory(database, observedAt))
        using (var issuingClient = issuingFactory.CreateApiClient(signup.UserId))
        {
            tokens = await AuthorizeAndIssueTokensAsync(
                issuingClient,
                signup.SeatId,
                Guid.CreateVersion7());
        }
        await TerminateTrialAsync(database);
        var interceptor = new ConcurrencyFailureInterceptor(_ => true);
        using var factory = new LicensingWebApplicationFactory(
            database,
            observedAt,
            interceptors: [interceptor]);
        using var client = factory.CreateApiClient();
        client.DefaultRequestHeaders.Authorization =
            new System.Net.Http.Headers.AuthenticationHeaderValue(
                "Bearer",
                tokens.AccessToken);

        using var response = await client.PostAsync(
            DesktopProtocolConstants.EntitlementPath,
            content: null);
        var error = await response.Content.ReadFromJsonAsync<DesktopProtocolErrorResponse>();
        var state = await ReadDesktopSessionStateAsync(database);

        Assert.Multiple(() =>
        {
            Assert.That(response.StatusCode, Is.EqualTo(HttpStatusCode.Conflict));
            Assert.That(error?.ReasonCode, Is.EqualTo("concurrent_modification"));
            Assert.That(interceptor.AttemptCount, Is.EqualTo(3));
            Assert.That(
                state.AuthorizationStatuses,
                Has.All.EqualTo(OpenIddictConstants.Statuses.Valid));
            Assert.That(state.TokenStatuses, Is.Not.Empty);
            Assert.That(
                state.TokenStatuses,
                Has.None.EqualTo(OpenIddictConstants.Statuses.Revoked));
            Assert.That(state.SessionRevokedAt, Is.Null);
        });
    }

    [TestCase(CommercialSubscriptionStatus.Active)]
    [TestCase(CommercialSubscriptionStatus.PastDue)]
    public async Task DelayedRenewalPreservesTheSameRefreshCredentialAndNeverExtendsTheOldLease(CommercialSubscriptionStatus status)
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(database, "delayed-renewal", "delayed-renewal@example.com");
        var periodEnd = SignupTime.AddDays(1).AddMinutes(5);
        await ProjectActiveSubscriptionAsync(database, signup.PersonalBillingAccountId, periodEnd);
        await TerminateTrialAsync(database);
        if (status == CommercialSubscriptionStatus.PastDue)
        {
            await using var context = database.CreateContext();
            var current = await context.CommercialSubscriptions.SingleAsync();
            current.TryApplyProjection(new CommercialSubscriptionProjection(current.ExternalCustomerId,
                current.ExternalSubscriptionId, current.ExternalPriceId, status, 1, false,
                current.CurrentPeriodStartedAt, periodEnd, SignupTime.AddHours(1)));
            await context.SaveChangesAsync();
        }
        DesktopTokenValues tokens;
        using (var issuing = new LicensingWebApplicationFactory(database, periodEnd.AddSeconds(-1)))
        using (var client = issuing.CreateApiClient(signup.UserId))
        {
            tokens = await AuthorizeAndIssueTokensAsync(client, signup.SeatId, Guid.CreateVersion7());
            client.DefaultRequestHeaders.Authorization = new("Bearer", tokens.AccessToken);
            using var leaseResponse = await client.PostAsync(DesktopProtocolConstants.EntitlementPath, null);
            using var lease = JsonDocument.Parse(await leaseResponse.Content.ReadAsStringAsync());
            Assert.That(leaseResponse.StatusCode, Is.EqualTo(HttpStatusCode.OK));
            Assert.That(lease.RootElement.GetProperty("expiresAt").GetDateTimeOffset(), Is.EqualTo(periodEnd));
        }
        var before = await ReadDesktopSessionStateAsync(database);
        using (var waiting = new LicensingWebApplicationFactory(database, periodEnd))
        using (var client = waiting.CreateApiClient())
        {
            for (var attempt = 0; attempt < 2; attempt++)
            {
                using var refresh = await RefreshAsync(client, tokens.RefreshToken);
                using var error = JsonDocument.Parse(await refresh.Content.ReadAsStringAsync());
                Assert.That(refresh.StatusCode, Is.EqualTo(HttpStatusCode.ServiceUnavailable));
                Assert.That(error.RootElement.GetProperty("error").GetString(), Is.EqualTo("temporarily_unavailable"));
            }
            client.DefaultRequestHeaders.Authorization = new("Bearer", tokens.AccessToken);
            using var leaseResponse = await client.PostAsync(DesktopProtocolConstants.EntitlementPath, null);
            using var errorBody = JsonDocument.Parse(await leaseResponse.Content.ReadAsStringAsync());
            Assert.That(leaseResponse.StatusCode, Is.EqualTo(HttpStatusCode.ServiceUnavailable));
            Assert.That(errorBody.RootElement.GetProperty("reasonCode").GetString(), Is.EqualTo(EntitlementReasonCodes.SubscriptionRenewalPending));
            Assert.That(errorBody.RootElement.TryGetProperty("lease", out _), Is.False);
        }
        var pending = await ReadDesktopSessionStateAsync(database);
        Assert.Multiple(() =>
        {
            Assert.That(pending.AuthorizationStatuses, Is.EquivalentTo(before.AuthorizationStatuses));
            Assert.That(pending.TokenStatuses, Is.EquivalentTo(before.TokenStatuses));
            Assert.That(pending.SessionRevokedAt, Is.Null);
        });
        await using (var context = database.CreateContext())
        {
            var current = await context.CommercialSubscriptions.SingleAsync();
            Assert.That(current.TryApplyProjection(new CommercialSubscriptionProjection(current.ExternalCustomerId,
                current.ExternalSubscriptionId, current.ExternalPriceId, CommercialSubscriptionStatus.Active, 1, false,
                periodEnd, periodEnd.AddMonths(1), periodEnd.AddMinutes(1))), Is.True);
            await context.SaveChangesAsync();
        }
        using var recovered = new LicensingWebApplicationFactory(database, periodEnd.AddMinutes(1));
        using var recoveredClient = recovered.CreateApiClient();
        using var successfulRefresh = await RefreshAsync(recoveredClient, tokens.RefreshToken);
        Assert.That(successfulRefresh.StatusCode, Is.EqualTo(HttpStatusCode.OK));
        var rotated = await ReadTokenResponseAsync(successfulRefresh);
        recoveredClient.DefaultRequestHeaders.Authorization = new("Bearer", rotated.AccessToken);
        using var renewedLease = await recoveredClient.PostAsync(DesktopProtocolConstants.EntitlementPath, null);
        using var renewedBody = JsonDocument.Parse(await renewedLease.Content.ReadAsStringAsync());
        Assert.That(renewedLease.StatusCode, Is.EqualTo(HttpStatusCode.OK));
        Assert.That(renewedBody.RootElement.GetProperty("expiresAt").GetDateTimeOffset(), Is.GreaterThan(periodEnd));
        await using var verification = database.CreateContext();
        Assert.That(verification.DesktopDeviceSessions.Count(), Is.EqualTo(1));
    }

    [Test]
    public async Task RenewalCommittedDuringRefreshDoesNotRevokeAnUnredeemedCredential()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(database, "racing-renewal", "racing-renewal@example.com");
        var periodEnd = SignupTime.AddDays(1).AddMinutes(5);
        await ProjectActiveSubscriptionAsync(database, signup.PersonalBillingAccountId, periodEnd);
        await TerminateTrialAsync(database);
        DesktopTokenValues tokens;
        using (var issuing = new LicensingWebApplicationFactory(database, periodEnd.AddSeconds(-1)))
        using (var client = issuing.CreateApiClient(signup.UserId))
        {
            tokens = await AuthorizeAndIssueTokensAsync(client, signup.SeatId, Guid.CreateVersion7());
        }
        var before = await ReadDesktopSessionStateAsync(database);
        var gate = new DatabaseCommandGate();
        using var factory = new LicensingWebApplicationFactory(database, periodEnd,
            interceptors: [new DatabaseCommandGateInterceptor(gate, "UPDATE billing_accounts")]);
        using var refreshClient = factory.CreateApiClient();
        var refreshTask = RefreshAsync(refreshClient, tokens.RefreshToken);
        try
        {
            await gate.WaitUntilReachedAsync();
            await using var context = database.CreateContext();
            var current = await context.CommercialSubscriptions.AsNoTracking().SingleAsync();
            var credential = await context.Set<OpenIddictEntityFrameworkCoreToken<Guid>>()
                .SingleAsync(token => token.Type == OpenIddictConstants.TokenTypeIdentifiers.RefreshToken);
            Assert.Multiple(() =>
            {
                Assert.That(credential.Status, Is.EqualTo(OpenIddictConstants.Statuses.Valid));
                Assert.That(credential.Subject, Is.EqualTo(signup.UserId.ToString()));
                Assert.That(credential.ExpirationDate, Is.GreaterThan(periodEnd.UtcDateTime));
            });
            var store = new PostgresCommercialSubscriptionProjectionStore(context);
            var result = await store.ApplyAsync(new AuthoritativeCommercialSubscription(signup.PersonalBillingAccountId,
                new CommercialSubscriptionProjection(current.ExternalCustomerId, current.ExternalSubscriptionId,
                    current.ExternalPriceId, CommercialSubscriptionStatus.Active, 1, false,
                    periodEnd, periodEnd.AddMonths(1), periodEnd), ProviderReadRevision: 1), CancellationToken.None);
            Assert.That(result.Status, Is.EqualTo(CommercialSubscriptionProjectionStatus.Updated));
        }
        finally
        {
            gate.Release();
        }
        using var interrupted = await refreshTask;
        using var error = JsonDocument.Parse(await interrupted.Content.ReadAsStringAsync());
        var retained = await ReadDesktopSessionStateAsync(database);
        Assert.Multiple(() =>
        {
            Assert.That(interrupted.StatusCode, Is.EqualTo(HttpStatusCode.ServiceUnavailable));
            Assert.That(error.RootElement.GetProperty("error").GetString(), Is.EqualTo("temporarily_unavailable"));
            Assert.That(retained.TokenStatuses, Is.EquivalentTo(before.TokenStatuses));
            Assert.That(retained.AuthorizationStatuses, Is.EquivalentTo(before.AuthorizationStatuses));
            Assert.That(retained.SessionRevokedAt, Is.Null);
        });
        using var retried = await RefreshAsync(refreshClient, tokens.RefreshToken);
        Assert.That(retried.StatusCode, Is.EqualTo(HttpStatusCode.OK));
        var rotated = await ReadTokenResponseAsync(retried);
        refreshClient.DefaultRequestHeaders.Authorization = new("Bearer", rotated.AccessToken);
        using var leaseResponse = await refreshClient.PostAsync(DesktopProtocolConstants.EntitlementPath, null);
        using var lease = JsonDocument.Parse(await leaseResponse.Content.ReadAsStringAsync());
        Assert.That(leaseResponse.StatusCode, Is.EqualTo(HttpStatusCode.OK));
        Assert.That(lease.RootElement.GetProperty("expiresAt").GetDateTimeOffset(), Is.GreaterThan(periodEnd));
    }

    [TestCase(CommercialSubscriptionStatus.Active)]
    [TestCase(CommercialSubscriptionStatus.Canceled)]
    public async Task ConfirmedCancellationAfterRenewalDelayRevokesTheRefreshSession(CommercialSubscriptionStatus status)
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(database, "canceled-renewal", "canceled-renewal@example.com");
        var periodEnd = SignupTime.AddDays(1).AddMinutes(5);
        await ProjectActiveSubscriptionAsync(database, signup.PersonalBillingAccountId, periodEnd);
        await TerminateTrialAsync(database);
        DesktopTokenValues tokens;
        using (var issuing = new LicensingWebApplicationFactory(database, periodEnd.AddSeconds(-1)))
        using (var client = issuing.CreateApiClient(signup.UserId))
        {
            tokens = await AuthorizeAndIssueTokensAsync(client, signup.SeatId, Guid.CreateVersion7());
        }
        using var factory = new LicensingWebApplicationFactory(database, periodEnd);
        using var refreshClient = factory.CreateApiClient();
        using var pending = await RefreshAsync(refreshClient, tokens.RefreshToken);
        Assert.That(pending.StatusCode, Is.EqualTo(HttpStatusCode.ServiceUnavailable));
        await using (var context = database.CreateContext())
        {
            var current = await context.CommercialSubscriptions.SingleAsync();
            Assert.That(current.TryApplyProjection(new CommercialSubscriptionProjection(current.ExternalCustomerId,
                current.ExternalSubscriptionId, current.ExternalPriceId, status, 1, true,
                current.CurrentPeriodStartedAt, periodEnd, periodEnd)), Is.True);
            await context.SaveChangesAsync();
        }
        using var rejected = await RefreshAsync(refreshClient, tokens.RefreshToken);
        using var error = JsonDocument.Parse(await rejected.Content.ReadAsStringAsync());
        var state = await ReadDesktopSessionStateAsync(database);
        Assert.Multiple(() =>
        {
            Assert.That(rejected.StatusCode, Is.EqualTo(HttpStatusCode.BadRequest));
            Assert.That(error.RootElement.GetProperty("error").GetString(), Is.EqualTo("invalid_grant"));
            Assert.That(state.AuthorizationStatuses, Has.All.EqualTo(OpenIddictConstants.Statuses.Revoked));
            Assert.That(state.TokenStatuses, Has.All.EqualTo(OpenIddictConstants.Statuses.Revoked));
            Assert.That(state.SessionRevokedAt, Is.EqualTo(periodEnd));
        });
    }

    private static async Task<DeviceAuthorizationValues> StartAuthorizationAsync(
        HttpClient client,
        Guid installationId)
    {
        using var response = await client.PostAsync(
            "/desktop/v1/device/authorize",
            DeviceAuthorizationForm(installationId));
        using var body = JsonDocument.Parse(await response.Content.ReadAsStringAsync());
        Assert.That(response.StatusCode, Is.EqualTo(HttpStatusCode.OK));
        return new DeviceAuthorizationValues(
            body.RootElement.GetProperty("device_code").GetString()!,
            body.RootElement.GetProperty("user_code").GetString()!);
    }

    private static Task<HttpResponseMessage> PollDeviceTokenAsync(
        HttpClient client,
        string deviceCode,
        string? clientId = ClientId)
    {
        var fields = new List<KeyValuePair<string, string>>
        {
            new("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
            new("device_code", deviceCode),
        };
        if (clientId is not null)
        {
            fields.Add(new KeyValuePair<string, string>("client_id", clientId));
        }

        return client.PostAsync(
            "/desktop/v1/token",
            new FormUrlEncodedContent(fields));
    }

    private static Task<HttpResponseMessage> RefreshAsync(
        HttpClient client,
        string refreshToken,
        string? clientId = ClientId)
    {
        var fields = new List<KeyValuePair<string, string>>
        {
            new("grant_type", "refresh_token"),
            new("refresh_token", refreshToken),
        };
        if (clientId is not null)
        {
            fields.Add(new KeyValuePair<string, string>("client_id", clientId));
        }

        return client.PostAsync(
            "/desktop/v1/token",
            new FormUrlEncodedContent(fields));
    }

    private static async Task<HttpResponseMessage> ChangeOrganizationSeatAsync(
        HttpClient client,
        Guid organizationId,
        Guid membershipId,
        bool assigned)
    {
        var request = new HttpRequestMessage(
            HttpMethod.Patch,
            $"/api/organizations/{organizationId}/members/{membershipId}/seat")
        {
            Content = JsonContent.Create(new { assigned }),
        };
        AntiforgeryTestClient.AddToken(
            request,
            await AntiforgeryTestClient.GetTokenAsync(client));
        return await client.SendAsync(request);
    }

    private static Task<HttpResponseMessage> RevokeTokenAsync(
        HttpClient client,
        string token,
        string? clientId = ClientId,
        string? tokenTypeHint = "refresh_token")
    {
        var fields = new List<KeyValuePair<string, string>>
        {
            new("token", token),
        };
        if (tokenTypeHint is not null)
        {
            fields.Add(new KeyValuePair<string, string>("token_type_hint", tokenTypeHint));
        }

        if (clientId is not null)
        {
            fields.Add(new KeyValuePair<string, string>("client_id", clientId));
        }

        return client.PostAsync(
            "/desktop/v1/token/revoke",
            new FormUrlEncodedContent(fields));
    }

    private static async Task<DesktopTokenValues> AuthorizeAndIssueTokensAsync(
        HttpClient client,
        Guid seatId,
        Guid installationId)
    {
        var authorization = await StartAuthorizationAsync(client, installationId);
        using var approval = await ApproveAsync(client, authorization.UserCode, seatId);
        using var response = await PollDeviceTokenAsync(client, authorization.DeviceCode);
        Assert.Multiple(() =>
        {
            Assert.That(approval.StatusCode, Is.EqualTo(HttpStatusCode.NoContent));
            Assert.That(response.StatusCode, Is.EqualTo(HttpStatusCode.OK));
        });
        return await ReadTokenResponseAsync(response);
    }

    private static async Task<HttpResponseMessage> ApproveAsync(
        HttpClient client,
        string userCode,
        Guid seatId)
    {
        var antiforgeryToken = await AntiforgeryTestClient.GetTokenAsync(client);
        var request = new HttpRequestMessage(
            HttpMethod.Post,
            "/desktop/v1/device/approval")
        {
            Content = ApprovalForm(userCode, seatId),
        };
        AntiforgeryTestClient.AddToken(request, antiforgeryToken);
        return await client.SendAsync(request);
    }

    private static async Task<HttpRequestMessage> CreateApprovalRequestAsync(
        HttpClient client,
        string userCode,
        Guid seatId)
    {
        var antiforgeryToken = await AntiforgeryTestClient.GetTokenAsync(client);
        var request = new HttpRequestMessage(
            HttpMethod.Post,
            "/desktop/v1/device/approval")
        {
            Content = ApprovalForm(userCode, seatId),
        };
        AntiforgeryTestClient.AddToken(request, antiforgeryToken);
        return request;
    }

    private static async Task<HttpRequestMessage> CreateDenialRequestAsync(
        HttpClient client,
        string userCode)
    {
        var antiforgeryToken = await AntiforgeryTestClient.GetTokenAsync(client);
        var request = new HttpRequestMessage(
            HttpMethod.Post,
            "/desktop/v1/device/approval")
        {
            Content = new FormUrlEncodedContent(
            [
                new("user_code", userCode),
                new("decision", "deny"),
            ]),
        };
        AntiforgeryTestClient.AddToken(request, antiforgeryToken);
        return request;
    }

    private static async Task<ApprovalAttempt> SendApprovalAttemptAsync(
        HttpClient client,
        HttpRequestMessage request)
    {
        try
        {
            return new ApprovalAttempt(await client.SendAsync(request), Exception: null);
        }
        catch (Exception exception)
        {
            return new ApprovalAttempt(Response: null, exception);
        }
    }

    private static async Task<HttpResponseMessage> DenyAsync(
        HttpClient client,
        string userCode)
    {
        var antiforgeryToken = await AntiforgeryTestClient.GetTokenAsync(client);
        var request = new HttpRequestMessage(
            HttpMethod.Post,
            "/desktop/v1/device/approval")
        {
            Content = new FormUrlEncodedContent(
            [
                new("user_code", userCode),
                new("decision", "deny"),
            ]),
        };
        AntiforgeryTestClient.AddToken(request, antiforgeryToken);
        return await client.SendAsync(request);
    }

    private static async Task<HttpResponseMessage> RevokeAsync(
        HttpClient client,
        Guid activationId)
    {
        var antiforgeryToken = await AntiforgeryTestClient.GetTokenAsync(client);
        var request = new HttpRequestMessage(
            HttpMethod.Delete,
            $"/api/device-activations/{activationId}");
        AntiforgeryTestClient.AddToken(request, antiforgeryToken);
        return await client.SendAsync(request);
    }

    private static FormUrlEncodedContent ApprovalForm(string userCode, Guid seatId)
    {
        return new(
        [
            new("user_code", userCode),
            new("decision", "approve"),
            new("seat_id", seatId.ToString()),
        ]);
    }

    private static async Task<DesktopTokenValues> ReadTokenResponseAsync(
        HttpResponseMessage response)
    {
        using var body = JsonDocument.Parse(await response.Content.ReadAsStringAsync());
        Assert.That(
            PropertyNames(body.RootElement),
            Is.EquivalentTo(
            [
                "access_token",
                "expires_in",
                "refresh_token",
                "scope",
                "token_type",
            ]));
        return new DesktopTokenValues(
            body.RootElement.GetProperty("access_token").GetString()!,
            body.RootElement.GetProperty("refresh_token").GetString()!,
            body.RootElement.GetProperty("token_type").GetString()!,
            body.RootElement.GetProperty("expires_in").GetInt32(),
            body.RootElement.GetProperty("scope").GetString()!);
    }

    private static async Task<Claim[]> ValidateAccessTokenAsync(
        string accessToken,
        string issuer = "https://localhost/")
    {
        using var certificate = X509CertificateLoader.LoadPkcs12(
            Convert.FromBase64String(
                TestDataProtectionCertificate.EncodedDesktopCertificate),
            TestDataProtectionCertificate.Password,
            X509KeyStorageFlags.EphemeralKeySet);
        var validation = await new JsonWebTokenHandler().ValidateTokenAsync(
            accessToken,
            new TokenValidationParameters
            {
                ValidIssuer = issuer,
                ValidAudience = DesktopProtocolConstants.Resource,
                IssuerSigningKey = new X509SecurityKey(certificate),
                TokenDecryptionKey = new X509SecurityKey(certificate),
                ValidateLifetime = false,
            });
        Assert.That(validation.IsValid, Is.True, validation.Exception?.ToString());
        return validation.ClaimsIdentity?.Claims.ToArray() ?? [];
    }

    private static async Task<TokenValidationResult> ValidateLeaseWithJsonWebKeyAsync(
        string lease,
        JsonElement jsonWebKey)
    {
        using var rsa = RSA.Create();
        rsa.ImportParameters(new RSAParameters
        {
            Modulus = Base64UrlEncoder.DecodeBytes(
                jsonWebKey.GetProperty("n").GetString()!),
            Exponent = Base64UrlEncoder.DecodeBytes(
                jsonWebKey.GetProperty("e").GetString()!),
        });
        return await new JsonWebTokenHandler().ValidateTokenAsync(
            lease,
            new TokenValidationParameters
            {
                ValidIssuer = "https://localhost/",
                ValidAudience = DesktopProtocolConstants.LeaseAudience,
                IssuerSigningKey = new RsaSecurityKey(rsa),
                ValidateLifetime = false,
            });
    }

    private static string ClaimValue(IEnumerable<Claim> claims, string claimType)
    {
        return claims.Single(claim => claim.Type == claimType).Value;
    }

    private static string[] PropertyNames(JsonElement element)
    {
        return element.EnumerateObject()
            .Select(property => property.Name)
            .Order(StringComparer.Ordinal)
            .ToArray();
    }

    private sealed class UnknownLengthFormContent : HttpContent
    {
        private readonly byte[] _body;

        public UnknownLengthFormContent(string body)
        {
            _body = Encoding.UTF8.GetBytes(body);
            Headers.ContentType = new("application/x-www-form-urlencoded");
        }

        protected override Task SerializeToStreamAsync(
            Stream stream,
            TransportContext? context)
        {
            return stream.WriteAsync(_body, 0, _body.Length);
        }

        protected override bool TryComputeLength(out long length)
        {
            length = 0;
            return false;
        }
    }

    private static async Task ProjectActiveSubscriptionAsync(
        PostgresTestDatabase database,
        Guid billingAccountId,
        DateTimeOffset? currentPeriodEndsAt = null)
    {
        var suffix = billingAccountId.ToString("N");
        await using var context = database.CreateContext();
        context.CommercialSubscriptions.Add(
            CommercialSubscription.Create(
                billingAccountId,
                new CommercialSubscriptionProjection(
                    $"cus_{suffix}",
                    $"sub_{suffix}",
                    "price_desktop",
                    CommercialSubscriptionStatus.Active,
                    seatQuantity: 1,
                    cancelAtPeriodEnd: false,
                    SignupTime,
                    currentPeriodEndsAt ?? SignupTime.AddDays(30),
                    SignupTime)));
        await context.SaveChangesAsync();
    }

    private static async Task TerminateTrialAsync(PostgresTestDatabase database)
    {
        await using var context = database.CreateContext();
        var trial = await context.Trials.SingleAsync();
        Assert.That(trial.TryTerminate(SignupTime.AddHours(12)), Is.True);
        await context.SaveChangesAsync();
    }

    private static async Task<DesktopSessionState> ReadDesktopSessionStateAsync(
        PostgresTestDatabase database)
    {
        await using var context = database.CreateContext();
        return new DesktopSessionState(
            await context.Set<OpenIddictEntityFrameworkCoreAuthorization<Guid>>()
                .Select(authorization => authorization.Status)
                .ToArrayAsync(),
            await context.Set<OpenIddictEntityFrameworkCoreToken<Guid>>()
                .Select(token => token.Status)
                .ToArrayAsync(),
            (await context.DesktopDeviceSessions.SingleAsync()).RevokedAt);
    }

    private sealed record DeviceAuthorizationValues(string DeviceCode, string UserCode);

    private sealed record DesktopTokenValues(
        string AccessToken,
        string RefreshToken,
        string TokenType,
        int ExpiresIn,
        string Scope);

    private sealed record DesktopSessionState(
        string?[] AuthorizationStatuses,
        string?[] TokenStatuses,
        DateTimeOffset? SessionRevokedAt);

    private sealed record ApprovalAttempt(
        HttpResponseMessage? Response,
        Exception? Exception);

    private static FormUrlEncodedContent DeviceAuthorizationForm(Guid installationId)
    {
        return DeviceAuthorizationForm(installationId.ToString());
    }

    private static FormUrlEncodedContent DeviceAuthorizationForm(string installationId)
    {
        return DeviceAuthorizationForm(
            installationId,
            ClientId,
            "styrhous.desktop offline_access");
    }

    private static FormUrlEncodedContent DeviceAuthorizationForm(
        Guid installationId,
        string clientId,
        string scope)
    {
        return DeviceAuthorizationForm(installationId.ToString(), clientId, scope);
    }

    private static FormUrlEncodedContent DeviceAuthorizationForm(
        string installationId,
        string clientId,
        string scope)
    {
        return new(
        [
            new("client_id", clientId),
            new("scope", scope),
            new("installation_id", installationId),
            new("display_name", "Rasmus's workstation"),
            new("platform", "linux"),
            new("architecture", "x86_64"),
            new("styrhous_version", "0.1.0"),
        ]);
    }

    private sealed class FailAfterDesktopApprovalInterceptor : SaveChangesInterceptor
    {
        public override ValueTask<int> SavedChangesAsync(
            SaveChangesCompletedEventData eventData,
            int result,
            CancellationToken cancellationToken = default)
        {
            if (eventData.Context?.ChangeTracker
                .Entries<DesktopDeviceSession>()
                .Any() is true)
            {
                throw new InvalidOperationException(
                    "Injected failure after the desktop approval was persisted.");
            }

            return ValueTask.FromResult(result);
        }
    }
}
