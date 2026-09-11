using System.Net;
using System.Net.Http.Json;
using System.Text.Json;
using Microsoft.AspNetCore.Http;
using Microsoft.EntityFrameworkCore;
using Styrhous.Licensing.Application.Devices;
using Styrhous.Licensing.Domain.Auditing;
using Styrhous.Licensing.Domain.Devices;
using Styrhous.Licensing.Persistence;
using Styrhous.Licensing.Tests.Persistence;
using static Styrhous.Licensing.Tests.Persistence.DevicePersistenceScenario;
using static Styrhous.Licensing.Tests.Persistence.LicensingPersistenceScenario;

namespace Styrhous.Licensing.Tests.Api;

[TestFixture]
[Parallelizable(ParallelScope.All)]
public sealed class DeviceApiTests
{
    private const string DeviceNotActiveReason = "device_not_active";
    private const string SeatNotFoundReason = "seat_not_found";

    private static readonly JsonSerializerOptions WebJson =
        new(JsonSerializerDefaults.Web);

    private static readonly string[] ErrorProperties = ["reasonCode"];

    private static readonly string[] ListProperties =
    [
        "reasonCode",
        "seatId",
        "deviceLimit",
        "activeDevices",
    ];

    private static readonly string[] DeviceProperties =
    [
        "activationId",
        "installationId",
        "displayName",
        "platform",
        "architecture",
        "styrhousVersion",
        "activatedAt",
        "lastSeenAt",
    ];

    private static readonly string[] RevocationProperties =
    [
        "reasonCode",
        "activationId",
        "correlationId",
    ];

    [Test]
    public async Task AuthenticatedOwnerListsActiveDevicesInLastSeenOrder()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(database, "api-devices", "devices@example.com");
        var oldestInstallation = CreateInstallation(1);
        var newestInstallation = CreateInstallation(2);
        var oldest = await ActivateAsync(
            database,
            signup,
            oldestInstallation,
            SignupTime);
        var newest = await ActivateAsync(
            database,
            signup,
            newestInstallation,
            SignupTime.AddDays(1));
        await using (var updateContext = database.CreateContext())
        {
            await updateContext.Seats
                .Where(seat => seat.Id == signup.SeatId)
                .ExecuteUpdateAsync(
                    setters => setters.SetProperty(seat => seat.DeviceLimit, 5));
        }

        using var factory = new LicensingWebApplicationFactory(database, SignupTime.AddDays(2));
        using var client = factory.CreateApiClient(signup.UserId);

        using var response = await client.GetAsync(DevicePath(signup.SeatId));
        var responseBody = await response.Content.ReadAsStringAsync();
        var body = JsonSerializer.Deserialize<DeviceListResponse>(responseBody, WebJson);
        using var responseJson = JsonDocument.Parse(responseBody);

        Assert.Multiple(() =>
        {
            Assert.That(response.StatusCode, Is.EqualTo(HttpStatusCode.OK));
            Assert.That(response.Content.Headers.ContentType?.MediaType,
                Is.EqualTo("application/json"));
            Assert.That(response.Headers.CacheControl?.NoStore, Is.True);
            Assert.That(
                responseJson.RootElement.EnumerateObject().Select(property => property.Name),
                Is.EquivalentTo(ListProperties));
            Assert.That(
                responseJson.RootElement.GetProperty("activeDevices")[0]
                    .EnumerateObject()
                    .Select(property => property.Name),
                Is.EquivalentTo(DeviceProperties));
            Assert.That(body!.ReasonCode, Is.EqualTo("devices_listed"));
            Assert.That(body.SeatId, Is.EqualTo(signup.SeatId));
            Assert.That(body.DeviceLimit, Is.EqualTo(5));
            Assert.That(
                body.ActiveDevices.Select(device => device.ActivationId),
                Is.EqualTo(new[] { newest.ActivationId, oldest.ActivationId }));
        });
        AssertDevice(body!.ActiveDevices[0], newest, newestInstallation, SignupTime.AddDays(1));
        AssertDevice(body.ActiveDevices[1], oldest, oldestInstallation, SignupTime);
    }

    [Test]
    public async Task ListingExcludesRevokedDevicesAfterEntitlementExpiry()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(
            database,
            "api-devices-expired",
            "expired-devices@example.com");
        var revoked = await ActivateAsync(database, signup, 1, SignupTime);
        var retained = await ActivateAsync(database, signup, 2, SignupTime.AddDays(1));
        await using var serviceTest1 = ServiceTestBase<DeviceRevocationService>.ForDatabase(database, SignupTime.AddDays(2));
        await serviceTest1.Service
            .RevokeAsync(signup.UserId, revoked.ActivationId!.Value);
        await using (var updateContext = database.CreateContext())
        {
            await updateContext.Trials.ExecuteUpdateAsync(
                setters => setters
                    .SetProperty(trial => trial.StartedAt, SignupTime.AddDays(-31))
                    .SetProperty(trial => trial.EndsAt, SignupTime.AddDays(-1)));
        }

        using var factory = new LicensingWebApplicationFactory(database, SignupTime.AddDays(3));
        using var client = factory.CreateApiClient(signup.UserId);

        using var response = await client.GetAsync(DevicePath(signup.SeatId));
        var body = await response.Content.ReadFromJsonAsync<DeviceListResponse>(WebJson);

        Assert.Multiple(() =>
        {
            Assert.That(response.StatusCode, Is.EqualTo(HttpStatusCode.OK));
            Assert.That(body!.DeviceLimit, Is.EqualTo(3));
            Assert.That(
                body.ActiveDevices.Select(device => device.ActivationId),
                Is.EqualTo(new[] { retained.ActivationId }));
        });
    }

    [Test]
    public async Task UnknownAndOtherUsersSeatsReturnTheSamePrivateNotFoundResponse()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(database, "api-device-owner", "device-owner@example.com");
        var other = await SignUpAsync(database, "api-device-other", "device-other@example.com");
        await ActivateAsync(database, owner, 1, SignupTime);
        using var factory = new LicensingWebApplicationFactory(database, SignupTime.AddDays(1));
        using var otherClient = factory.CreateApiClient(other.UserId);
        using var ownerClient = factory.CreateApiClient(owner.UserId);

        using var otherUsersResponse = await otherClient.GetAsync(DevicePath(owner.SeatId));
        using var unknownResponse = await ownerClient.GetAsync(DevicePath(Guid.CreateVersion7()));
        var otherUsersBody = await ReadReasonCodeAsync(otherUsersResponse);
        var unknownBody = await ReadReasonCodeAsync(unknownResponse);

        Assert.Multiple(() =>
        {
            Assert.That(otherUsersResponse.StatusCode, Is.EqualTo(HttpStatusCode.NotFound));
            Assert.That(unknownResponse.StatusCode, Is.EqualTo(HttpStatusCode.NotFound));
            Assert.That(otherUsersResponse.Headers.CacheControl?.NoStore, Is.True);
            Assert.That(unknownResponse.Headers.CacheControl?.NoStore, Is.True);
            Assert.That(otherUsersBody, Is.EqualTo(SeatNotFoundReason));
            Assert.That(unknownBody, Is.EqualTo(SeatNotFoundReason));
        });
    }

    [TestCase(TestIdentifiers.MalformedText)]
    [TestCase("00000000-0000-0000-0000-000000000000")]
    [TestCase("4f06ed53-3d60-4f91-91d9-735e5b75c8d2")]
    public async Task MalformedOrUnknownSeatIdentifiersUseThePrivateNotFoundResponse(string seatId)
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(database, "api-device-invalid", "invalid-device@example.com");
        using var factory = new LicensingWebApplicationFactory(database, SignupTime);
        using var client = factory.CreateApiClient(signup.UserId);

        using var response = await client.GetAsync($"/api/seats/{seatId}/devices");
        var reasonCode = await ReadReasonCodeAsync(response);

        Assert.Multiple(() =>
        {
            Assert.That(response.StatusCode, Is.EqualTo(HttpStatusCode.NotFound));
            Assert.That(reasonCode, Is.EqualTo(SeatNotFoundReason));
        });
    }

    [TestCase(null)]
    [TestCase("")]
    [TestCase(TestIdentifiers.MalformedText)]
    public async Task MissingOrInvalidAuthenticationIsChallengedWithoutRedirect(string? userId)
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        using var factory = new LicensingWebApplicationFactory(database, SignupTime);
        using var client = factory.CreateApiClient();
        if (userId is not null)
        {
            client.DefaultRequestHeaders.Add(
                LicensingWebApplicationFactory.UserIdHeader,
                userId);
        }

        using var response = await client.GetAsync(DevicePath(Guid.CreateVersion7()));

        Assert.Multiple(() =>
        {
            Assert.That(response.StatusCode, Is.EqualTo(HttpStatusCode.Unauthorized));
            Assert.That(response.Headers.Location, Is.Null);
        });
    }

    [Test]
    public async Task ClientCancellationStopsTheServerSideDeviceQuery()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(
            database,
            "api-device-cancel",
            "device-cancel@example.com");
        await ActivateAsync(database, signup, 1, SignupTime);
        var gate = new DatabaseCommandGate();
        var path = DevicePath(signup.SeatId);
        var requestCompletionObserver = new RequestCompletionObserver(HttpMethods.Get, path);
        var queryGateInterceptor = new DatabaseCommandGateInterceptor(
            gate,
            "FROM device_activations");
        using var factory = new LicensingWebApplicationFactory(
            database,
            SignupTime.AddDays(1),
            useTestAuthentication: true,
            requestCompletionObserver: requestCompletionObserver,
            interceptors: [queryGateInterceptor]);
        using var client = factory.CreateApiClient(signup.UserId);
        using var cancellation = new CancellationTokenSource();

        var responseTask = client.GetAsync(path, cancellation.Token);
        await gate.WaitUntilReachedAsync();
        try
        {
            cancellation.Cancel();
            Assert.ThrowsAsync<TaskCanceledException>(async () => await responseTask);
            await queryGateInterceptor.WaitUntilCancellationObservedAsync();
            await requestCompletionObserver.WaitUntilCompletedAsync();
        }
        finally
        {
            gate.Release();
        }

        Assert.That(requestCompletionObserver.WasCanceled, Is.True);
    }

    [Test]
    public async Task AuthenticatedOwnerRevokesDeviceAndReceivesCorrelatedResult()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(
            database,
            "api-device-revoke",
            "device-revoke@example.com");
        var activation = await ActivateAsync(database, signup, 1, SignupTime);
        var revokedAt = SignupTime.AddDays(1);
        using var factory = new LicensingWebApplicationFactory(database, revokedAt);
        using var client = factory.CreateApiClient(signup.UserId);

        using var response = await RevokeDeviceAsync(
            client,
            activation.ActivationId!.Value);
        var responseBody = await response.Content.ReadAsStringAsync();
        var body = JsonSerializer.Deserialize<DeviceRevocationResponse>(responseBody, WebJson);
        using var responseJson = JsonDocument.Parse(responseBody);

        Assert.Multiple(() =>
        {
            Assert.That(response.StatusCode, Is.EqualTo(HttpStatusCode.OK));
            Assert.That(response.Content.Headers.ContentType?.MediaType,
                Is.EqualTo("application/json"));
            Assert.That(response.Headers.CacheControl?.NoStore, Is.True);
            Assert.That(
                responseJson.RootElement.EnumerateObject().Select(property => property.Name),
                Is.EquivalentTo(RevocationProperties));
            Assert.That(body!.ReasonCode, Is.EqualTo("device_revoked"));
            Assert.That(body.ActivationId, Is.EqualTo(activation.ActivationId));
            Assert.That(body.ActivationId.Version, Is.EqualTo(7));
            Assert.That(body.CorrelationId.Version, Is.EqualTo(7));
        });
        await using var verificationContext = database.CreateContext();
        var persisted = await verificationContext.DeviceActivations.SingleAsync();
        var audit = await verificationContext.AuditRecords.SingleAsync(
            record => record.Action == AuditAction.DeviceRevokedManual);
        Assert.Multiple(() =>
        {
            Assert.That(persisted.RevokedAt, Is.EqualTo(revokedAt));
            Assert.That(persisted.RevocationReason, Is.EqualTo(DeviceRevocationReason.Manual));
            Assert.That(audit.CorrelationId, Is.EqualTo(body!.CorrelationId));
            Assert.That(audit.ActorUserId, Is.EqualTo(signup.UserId));
            Assert.That(audit.TargetId, Is.EqualTo(activation.ActivationId));
        });
    }

    [Test]
    public async Task UnknownRevokedAndOtherUsersDevicesSharePrivateNotFoundResponse()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(
            database,
            "api-revoke-owner",
            "api-revoke-owner@example.com");
        var other = await SignUpAsync(
            database,
            "api-revoke-other",
            "api-revoke-other@example.com");
        var activation = await ActivateAsync(database, owner, 1, SignupTime);
        using var factory = new LicensingWebApplicationFactory(
            database,
            SignupTime.AddDays(1));
        using var ownerClient = factory.CreateApiClient(owner.UserId);
        using var otherClient = factory.CreateApiClient(other.UserId);

        using var otherUsersResponse = await RevokeDeviceAsync(
            otherClient,
            activation.ActivationId!.Value);
        using var unknownResponse = await RevokeDeviceAsync(
            ownerClient,
            Guid.CreateVersion7());
        using var revokedResponse = await RevokeDeviceAsync(
            ownerClient,
            activation.ActivationId.Value);
        using var repeatedResponse = await RevokeDeviceAsync(
            ownerClient,
            activation.ActivationId.Value);
        var privateResponses = new[]
        {
            otherUsersResponse,
            unknownResponse,
            repeatedResponse,
        };

        Assert.Multiple(() =>
        {
            Assert.That(revokedResponse.StatusCode, Is.EqualTo(HttpStatusCode.OK));
            Assert.That(
                privateResponses.Select(response => response.StatusCode),
                Is.All.EqualTo(HttpStatusCode.NotFound));
            Assert.That(
                privateResponses.Select(response => response.Headers.CacheControl?.NoStore),
                Is.All.True);
        });
        foreach (var response in privateResponses)
        {
            Assert.That(
                await ReadReasonCodeAsync(response),
                Is.EqualTo(DeviceNotActiveReason));
        }

        await using var verificationContext = database.CreateContext();
        Assert.That(
            await verificationContext.AuditRecords.CountAsync(
                record => record.Action == AuditAction.DeviceRevokedManual),
            Is.EqualTo(1));
    }

    [TestCase(TestIdentifiers.MalformedText)]
    [TestCase("00000000-0000-0000-0000-000000000000")]
    [TestCase("4f06ed53-3d60-4f91-91d9-735e5b75c8d2")]
    public async Task MalformedOrUnknownActivationIdentifiersUsePrivateNotFoundResponse(string activationId)
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(
            database,
            "api-revoke-invalid",
            "api-revoke-invalid@example.com");
        using var factory = new LicensingWebApplicationFactory(database, SignupTime);
        using var client = factory.CreateApiClient(signup.UserId);

        using var response = await RevokeDeviceAsync(client, activationId);

        Assert.Multiple(() =>
        {
            Assert.That(response.StatusCode, Is.EqualTo(HttpStatusCode.NotFound));
            Assert.That(response.Headers.CacheControl?.NoStore, Is.True);
        });
        Assert.That(
            await ReadReasonCodeAsync(response),
            Is.EqualTo(DeviceNotActiveReason));
    }

    [TestCase(null)]
    [TestCase("")]
    [TestCase(TestIdentifiers.MalformedText)]
    public async Task RevocationRequiresValidAuthenticationWithoutRedirect(string? userId)
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        using var factory = new LicensingWebApplicationFactory(database, SignupTime);
        using var client = factory.CreateApiClient();
        if (userId is not null)
        {
            client.DefaultRequestHeaders.Add(
                LicensingWebApplicationFactory.UserIdHeader,
                userId);
        }

        using var response = await client.DeleteAsync(DeviceRevocationPath(Guid.CreateVersion7()));

        Assert.Multiple(() =>
        {
            Assert.That(response.StatusCode, Is.EqualTo(HttpStatusCode.Unauthorized));
            Assert.That(response.Headers.Location, Is.Null);
        });
    }

    [Test]
    public async Task MissingAntiforgeryTokenRejectsRevocationWithoutWriting()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(
            database,
            "api-revoke-csrf",
            "api-revoke-csrf@example.com");
        var activation = await ActivateAsync(database, signup, 1, SignupTime);
        using var factory = new LicensingWebApplicationFactory(
            database,
            SignupTime.AddDays(1));
        using var client = factory.CreateApiClient(signup.UserId);

        using var response = await client.DeleteAsync(
            DeviceRevocationPath(activation.ActivationId!.Value));

        Assert.That(response.StatusCode, Is.EqualTo(HttpStatusCode.BadRequest));
        await using var verificationContext = database.CreateContext();
        var persisted = await verificationContext.DeviceActivations.SingleAsync();
        var manualAuditCount = await verificationContext.AuditRecords.CountAsync(
            record => record.Action == AuditAction.DeviceRevokedManual);
        Assert.Multiple(() =>
        {
            Assert.That(persisted.RevokedAt, Is.Null);
            Assert.That(manualAuditCount, Is.Zero);
        });
    }

    [Test]
    public async Task ClientCancellationAfterSavingRollsBackRevocationAndAudit()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(
            database,
            "api-revoke-cancel",
            "api-revoke-cancel@example.com");
        var activation = await ActivateAsync(database, signup, 1, SignupTime);
        var gate = new DatabaseCommandGate();
        var path = DeviceRevocationPath(activation.ActivationId!.Value);
        var requestCompletionObserver = new RequestCompletionObserver(HttpMethods.Delete, path);
        var saveGateInterceptor = new SavedChangesGateInterceptor(gate);
        using var factory = new LicensingWebApplicationFactory(
            database,
            SignupTime.AddDays(1),
            useTestAuthentication: true,
            requestCompletionObserver: requestCompletionObserver,
            interceptors: [saveGateInterceptor]);
        using var client = factory.CreateApiClient(signup.UserId);
        var token = await AntiforgeryTestClient.GetTokenAsync(client);
        using var request = CreateRevocationRequest(path, token);
        using var cancellation = new CancellationTokenSource();

        var responseTask = client.SendAsync(request, cancellation.Token);
        await gate.WaitUntilReachedAsync();
        try
        {
            cancellation.Cancel();
            Assert.ThrowsAsync<TaskCanceledException>(async () => await responseTask);
            await requestCompletionObserver.WaitUntilCompletedAsync();
        }
        finally
        {
            gate.Release();
        }

        Assert.Multiple(() =>
        {
            Assert.That(saveGateInterceptor.CancellationObserved, Is.True);
            Assert.That(requestCompletionObserver.WasCanceled, Is.True);
        });
        await using var verificationContext = database.CreateContext();
        var persisted = await verificationContext.DeviceActivations.SingleAsync();
        var manualAuditCount = await verificationContext.AuditRecords.CountAsync(
            record => record.Action == AuditAction.DeviceRevokedManual);
        Assert.Multiple(() =>
        {
            Assert.That(persisted.RevokedAt, Is.Null);
            Assert.That(manualAuditCount, Is.Zero);
        });
    }

    private static string DevicePath(Guid seatId)
    {
        return $"/api/seats/{seatId}/devices";
    }

    private static string DeviceRevocationPath(Guid activationId)
    {
        return $"/api/device-activations/{activationId}";
    }

    private static async Task<HttpResponseMessage> RevokeDeviceAsync(
        HttpClient client,
        Guid activationId)
    {
        return await RevokeDeviceAsync(client, activationId.ToString());
    }

    private static async Task<HttpResponseMessage> RevokeDeviceAsync(
        HttpClient client,
        string activationId)
    {
        var token = await AntiforgeryTestClient.GetTokenAsync(client);
        using var request = CreateRevocationRequest(
            $"/api/device-activations/{activationId}",
            token);
        return await client.SendAsync(request);
    }

    private static HttpRequestMessage CreateRevocationRequest(
        string path,
        string antiforgeryToken)
    {
        var request = new HttpRequestMessage(HttpMethod.Delete, path);
        AntiforgeryTestClient.AddToken(request, antiforgeryToken);
        return request;
    }

    private static async Task<string?> ReadReasonCodeAsync(HttpResponseMessage response)
    {
        using var body = await JsonDocument.ParseAsync(
            await response.Content.ReadAsStreamAsync());
        Assert.That(
            body.RootElement.EnumerateObject().Select(property => property.Name),
            Is.EqualTo(ErrorProperties));
        return body.RootElement.GetProperty("reasonCode").GetString();
    }

    private static void AssertDevice(
        ActiveDeviceResponse device,
        DeviceActivationResult activation,
        DesktopInstallation installation,
        DateTimeOffset observedAt)
    {
        Assert.Multiple(() =>
        {
            Assert.That(device.ActivationId, Is.EqualTo(activation.ActivationId));
            Assert.That(device.InstallationId, Is.EqualTo(installation.InstallationId));
            Assert.That(device.DisplayName, Is.EqualTo(installation.DisplayName));
            Assert.That(device.Platform, Is.EqualTo(installation.Platform));
            Assert.That(device.Architecture, Is.EqualTo(installation.Architecture));
            Assert.That(device.StyrhousVersion, Is.EqualTo(installation.StyrhousVersion));
            Assert.That(device.ActivatedAt, Is.EqualTo(observedAt));
            Assert.That(device.LastSeenAt, Is.EqualTo(observedAt));
            Assert.That(device.ActivationId.Version, Is.EqualTo(7));
            Assert.That(device.InstallationId.Version, Is.EqualTo(7));
        });
    }

    private sealed record DeviceListResponse(
        string ReasonCode,
        Guid SeatId,
        int DeviceLimit,
        IReadOnlyList<ActiveDeviceResponse> ActiveDevices);

    private sealed record ActiveDeviceResponse(
        Guid ActivationId,
        Guid InstallationId,
        string DisplayName,
        string Platform,
        string Architecture,
        string StyrhousVersion,
        DateTimeOffset ActivatedAt,
        DateTimeOffset LastSeenAt);

    private sealed record DeviceRevocationResponse(
        string ReasonCode,
        Guid ActivationId,
        Guid CorrelationId);
}
