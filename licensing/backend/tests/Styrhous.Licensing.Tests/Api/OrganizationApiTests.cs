using System.Net;
using System.Net.Http.Json;
using System.Text;
using System.Text.Json;
using Microsoft.AspNetCore.Authentication;
using Microsoft.AspNetCore.Authentication.Cookies;
using Microsoft.AspNetCore.Http;
using Microsoft.EntityFrameworkCore;
using Microsoft.Extensions.DependencyInjection;
using Microsoft.Extensions.Options;
using Styrhous.Licensing.Api.Organizations;
using Styrhous.Licensing.Api.Authentication;
using Styrhous.Licensing.Application.Organizations;
using Styrhous.Licensing.Domain.Auditing;
using Styrhous.Licensing.Domain.Organizations;
using Styrhous.Licensing.Tests.Persistence;
using static Styrhous.Licensing.Tests.Persistence.LicensingPersistenceScenario;

namespace Styrhous.Licensing.Tests.Api;

[TestFixture]
[Parallelizable(ParallelScope.All)]
public sealed class OrganizationApiTests
{
    private static readonly JsonSerializerOptions WebJson =
        new(JsonSerializerDefaults.Web);

    private static readonly string[] OrganizationResponseProperties =
    [
        "userId",
        "organizationId",
        "billingAccountId",
        "ownerMembershipId",
        "seatId",
        "correlationId",
        "trialWasTransferred",
    ];

    private static IEnumerable<string> InvalidOrganizationNames()
    {
        yield return string.Empty;
        yield return "   ";
        yield return new string('a', Organization.MaximumNameLength + 1);
    }

    [Test]
    public async Task AuthenticatedActorCreatesCompleteOrganizationGraph()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(database, "api-owner", "api-owner@example.com");
        var otherUserId = Guid.CreateVersion7();
        using var factory = new LicensingWebApplicationFactory(
            database,
            SignupTime.AddDays(1));
        using var client = factory.CreateApiClient(signup.UserId);
        var antiforgeryToken = await AntiforgeryTestClient.GetTokenAsync(client);
        using var request = CreateOrganizationRequest(
            antiforgeryToken,
            JsonContent.Create(new
            {
                name = "API Organization",
                ownerUserId = otherUserId,
            }));

        using var response = await client.SendAsync(request);
        var responseBody = await response.Content.ReadAsStringAsync();
        var result = JsonSerializer.Deserialize<OrganizationCreationResponse>(
            responseBody,
            WebJson)!;
        using var responseJson = JsonDocument.Parse(responseBody);

        Assert.Multiple(() =>
        {
            Assert.That(response.StatusCode, Is.EqualTo(HttpStatusCode.Created));
            Assert.That(response.Content.Headers.ContentType?.MediaType,
                Is.EqualTo("application/json"));
            Assert.That(response.Headers.Location?.ToString(),
                Is.EqualTo($"/api/organizations/{result.OrganizationId}"));
            Assert.That(result.UserId, Is.EqualTo(signup.UserId));
            Assert.That(result.UserId, Is.Not.EqualTo(otherUserId));
            Assert.That(result.TrialWasTransferred, Is.True);
            Assert.That(
                new[]
                {
                    result.OrganizationId,
                    result.BillingAccountId,
                    result.OwnerMembershipId,
                    result.SeatId,
                    result.CorrelationId,
                },
                Has.All.Property(nameof(Guid.Version)).EqualTo(7));
            Assert.That(
                responseJson.RootElement.EnumerateObject().Select(property => property.Name),
                Is.EquivalentTo(OrganizationResponseProperties));
        });
        await using var verificationContext = database.CreateContext();
        var organization = await verificationContext.Organizations.SingleAsync();
        var billingAccount = await verificationContext.BillingAccounts.SingleAsync(
            account => account.Id == result.BillingAccountId);
        var membership = await verificationContext.OrganizationMemberships.SingleAsync();
        var seat = await verificationContext.Seats.SingleAsync(
            candidate => candidate.Id == result.SeatId);
        var auditRecords = await verificationContext.AuditRecords
            .Where(record => record.CorrelationId == result.CorrelationId)
            .ToArrayAsync();
        Assert.Multiple(() =>
        {
            Assert.That(organization.Id, Is.EqualTo(result.OrganizationId));
            Assert.That(organization.BillingAccountId, Is.EqualTo(billingAccount.Id));
            Assert.That(organization.CreatedByUserId, Is.EqualTo(signup.UserId));
            Assert.That(membership.Id, Is.EqualTo(result.OwnerMembershipId));
            Assert.That(membership.OrganizationId, Is.EqualTo(result.OrganizationId));
            Assert.That(seat.BillingAccountId, Is.EqualTo(result.BillingAccountId));
            Assert.That(seat.AssignedUserId, Is.EqualTo(signup.UserId));
            Assert.That(auditRecords, Has.All.Property(nameof(AuditRecord.ActorUserId))
                .EqualTo(signup.UserId));
            Assert.That(AuditTargets(auditRecords),
                Is.EquivalentTo(ExpectedAuditTargets(result, signup.TrialId)));
        });
    }

    [Test]
    public async Task ProductionCookieChallengesReturnStatusCodesWithoutRedirects()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        using var factory = new LicensingWebApplicationFactory(
            database,
            SignupTime,
            useTestAuthentication: false);
        using var client = factory.CreateApiClient();

        using var tokenResponse = await client.GetAsync("/api/antiforgery");
        using var mutationResponse = await client.PostAsJsonAsync(
            "/api/organizations",
            new { name = "Unauthorized Organization" });
        var cookieOptions = factory.Services
            .GetRequiredService<IOptionsMonitor<CookieAuthenticationOptions>>()
            .Get(AccountAuthentication.SessionScheme);
        var deniedHttpContext = new DefaultHttpContext();
        var deniedContext = new RedirectContext<CookieAuthenticationOptions>(
            deniedHttpContext,
            new AuthenticationScheme(
                AccountAuthentication.SessionScheme,
                displayName: null,
                typeof(CookieAuthenticationHandler)),
            cookieOptions,
            new AuthenticationProperties(),
            "https://localhost/access-denied");
        await cookieOptions.Events.RedirectToAccessDenied(deniedContext);

        Assert.Multiple(() =>
        {
            Assert.That(tokenResponse.StatusCode, Is.EqualTo(HttpStatusCode.Unauthorized));
            Assert.That(tokenResponse.Headers.Location, Is.Null);
            Assert.That(mutationResponse.StatusCode, Is.EqualTo(HttpStatusCode.Unauthorized));
            Assert.That(mutationResponse.Headers.Location, Is.Null);
            Assert.That(deniedHttpContext.Response.StatusCode,
                Is.EqualTo(StatusCodes.Status403Forbidden));
            Assert.That(deniedHttpContext.Response.Headers.Location, Is.Empty);
        });
    }

    [Test]
    public async Task MissingAntiforgeryTokenRejectsMutationWithoutWriting()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(database, "api-csrf", "api-csrf@example.com");
        var baseline = await CapturePersistenceAsync(database);
        using var factory = new LicensingWebApplicationFactory(database, SignupTime.AddDays(1));
        using var client = factory.CreateApiClient(signup.UserId);

        using var response = await client.PostAsJsonAsync(
            "/api/organizations",
            new { name = "Rejected Organization" });

        Assert.That(response.StatusCode, Is.EqualTo(HttpStatusCode.BadRequest));
        AssertPersistenceUnchanged(baseline, await CapturePersistenceAsync(database));
    }

    [Test]
    public async Task InvalidOrUnpairedAntiforgeryTokensRejectMutationWithoutWriting()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(database, "csrf-pair", "csrf-pair@example.com");
        var baseline = await CapturePersistenceAsync(database);
        using var factory = new LicensingWebApplicationFactory(database, SignupTime.AddDays(1));
        using var cookieClient = factory.CreateApiClient(signup.UserId);
        var validToken = await AntiforgeryTestClient.GetTokenAsync(cookieClient);
        using var malformedRequest = CreateOrganizationRequest(
            "not-an-antiforgery-token",
            JsonContent.Create(new { name = "Malformed Token" }));
        using var malformedResponse = await cookieClient.SendAsync(malformedRequest);
        using var noCookieClient = factory.CreateApiClient(signup.UserId);
        using var unpairedRequest = CreateOrganizationRequest(
            validToken,
            JsonContent.Create(new { name = "Unpaired Token" }));
        using var unpairedResponse = await noCookieClient.SendAsync(unpairedRequest);

        Assert.Multiple(() =>
        {
            Assert.That(malformedResponse.StatusCode, Is.EqualTo(HttpStatusCode.BadRequest));
            Assert.That(unpairedResponse.StatusCode, Is.EqualTo(HttpStatusCode.BadRequest));
        });
        AssertPersistenceUnchanged(baseline, await CapturePersistenceAsync(database));
    }

    [Test]
    public async Task AntiforgeryTokenCannotBeReusedAsAnotherAuthenticatedUser()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var first = await SignUpAsync(database, "csrf-first", "csrf-first@example.com");
        var second = await SignUpAsync(database, "csrf-second", "csrf-second@example.com");
        var baseline = await CapturePersistenceAsync(database);
        using var factory = new LicensingWebApplicationFactory(database, SignupTime.AddDays(1));
        using var client = factory.CreateApiClient(first.UserId);
        var firstUsersToken = await AntiforgeryTestClient.GetTokenAsync(client);
        client.DefaultRequestHeaders.Remove(LicensingWebApplicationFactory.UserIdHeader);
        client.DefaultRequestHeaders.Add(
            LicensingWebApplicationFactory.UserIdHeader,
            second.UserId.ToString());
        using var request = CreateOrganizationRequest(
            firstUsersToken,
            JsonContent.Create(new { name = "Cross-user CSRF" }));

        using var response = await client.SendAsync(request);

        Assert.That(response.StatusCode, Is.EqualTo(HttpStatusCode.BadRequest));
        AssertPersistenceUnchanged(baseline, await CapturePersistenceAsync(database));
    }

    [Test]
    public async Task AntiforgeryAndSessionCookiePoliciesAreSecureAndTokenIsNotCacheable()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(database, "cookie-policy", "cookies@example.com");
        using var factory = new LicensingWebApplicationFactory(database, SignupTime);
        using var client = factory.CreateApiClient(signup.UserId);

        using var response = await client.GetAsync("/api/antiforgery");
        var antiforgeryCookie = response.Headers.GetValues("Set-Cookie")
            .Single(value => value.StartsWith(
                "__Host-styrhous-antiforgery=",
                StringComparison.Ordinal));
        var sessionCookie = factory.Services
            .GetRequiredService<IOptionsMonitor<CookieAuthenticationOptions>>()
            .Get(AccountAuthentication.SessionScheme)
            .Cookie;

        Assert.Multiple(() =>
        {
            Assert.That(response.StatusCode, Is.EqualTo(HttpStatusCode.OK));
            Assert.That(response.Headers.CacheControl?.NoStore, Is.True);
            Assert.That(antiforgeryCookie, Does.Contain("path=/").IgnoreCase);
            Assert.That(antiforgeryCookie, Does.Contain("secure").IgnoreCase);
            Assert.That(antiforgeryCookie, Does.Contain("httponly").IgnoreCase);
            Assert.That(antiforgeryCookie, Does.Contain("samesite=strict").IgnoreCase);
            Assert.That(antiforgeryCookie, Does.Not.Contain("domain=").IgnoreCase);
            Assert.That(sessionCookie.Name, Is.EqualTo("__Host-styrhous-session"));
            Assert.That(sessionCookie.HttpOnly, Is.True);
            Assert.That(sessionCookie.SecurePolicy, Is.EqualTo(CookieSecurePolicy.Always));
            Assert.That(sessionCookie.SameSite, Is.EqualTo(SameSiteMode.Lax));
            Assert.That(sessionCookie.Path, Is.EqualTo("/"));
            Assert.That(sessionCookie.Domain, Is.Null);
        });
    }

    [Test]
    public async Task StaleAuthenticatedUserIsRejectedWithoutWriting()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var baseline = await CapturePersistenceAsync(database);
        using var factory = new LicensingWebApplicationFactory(database, SignupTime);
        using var client = factory.CreateApiClient(Guid.CreateVersion7());
        using var response = await PostOrganizationAsync(
            client,
            JsonContent.Create(new { name = "Orphan Organization" }));

        Assert.That(response.StatusCode, Is.EqualTo(HttpStatusCode.Unauthorized));
        AssertPersistenceUnchanged(baseline, await CapturePersistenceAsync(database));
    }

    [TestCaseSource(nameof(InvalidOrganizationNames))]
    public async Task InvalidOrganizationNameReturnsValidationProblemWithoutWriting(string name)
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(database, "api-validation", "validation@example.com");
        var baseline = await CapturePersistenceAsync(database);
        using var factory = new LicensingWebApplicationFactory(database, SignupTime);
        using var client = factory.CreateApiClient(signup.UserId);

        using var response = await PostOrganizationAsync(
            client,
            JsonContent.Create(new { name }));
        using var body = await JsonDocument.ParseAsync(
            await response.Content.ReadAsStreamAsync());

        Assert.Multiple(() =>
        {
            Assert.That(response.StatusCode, Is.EqualTo(HttpStatusCode.BadRequest));
            Assert.That(response.Content.Headers.ContentType?.MediaType,
                Is.EqualTo("application/problem+json"));
            Assert.That(
                body.RootElement.GetProperty("errors").GetProperty("name")[0].GetString(),
                Is.EqualTo("A valid organization name is required."));
        });
        AssertPersistenceUnchanged(baseline, await CapturePersistenceAsync(database));
    }

    [TestCase("{", "application/json", HttpStatusCode.BadRequest)]
    [TestCase("{}", "application/json", HttpStatusCode.BadRequest)]
    [TestCase("{\"name\":null}", "application/json", HttpStatusCode.BadRequest)]
    [TestCase("null", "application/json", HttpStatusCode.BadRequest)]
    [TestCase("{\"name\":\"Wrong content type\"}", "text/plain", HttpStatusCode.UnsupportedMediaType)]
    public async Task InvalidRequestBodiesAreControlledAndDoNotWrite(
        string body,
        string mediaType,
        HttpStatusCode expectedStatus)
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(
            database,
            $"invalid-body-{Guid.CreateVersion7():N}",
            $"invalid-body-{Guid.CreateVersion7():N}@example.com");
        var baseline = await CapturePersistenceAsync(database);
        using var factory = new LicensingWebApplicationFactory(database, SignupTime);
        using var client = factory.CreateApiClient(signup.UserId);
        var token = await AntiforgeryTestClient.GetTokenAsync(client);
        using var request = CreateOrganizationRequest(
            token,
            new StringContent(body, Encoding.UTF8, mediaType));

        using var response = await client.SendAsync(request);

        Assert.That(response.StatusCode, Is.EqualTo(expectedStatus));
        AssertPersistenceUnchanged(baseline, await CapturePersistenceAsync(database));
    }

    [TestCase(false)]
    [TestCase(true)]
    public async Task MaximumLengthOrganizationNameIsAccepted(bool padded)
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(database, "maximum-name", "maximum@example.com");
        using var factory = new LicensingWebApplicationFactory(database, SignupTime);
        using var client = factory.CreateApiClient(signup.UserId);
        var name = new string('a', Organization.MaximumNameLength);

        using var response = await PostOrganizationAsync(
            client,
            JsonContent.Create(new { name = padded ? $"  {name}  " : name }));

        Assert.That(response.StatusCode, Is.EqualTo(HttpStatusCode.Created));
        await using var verificationContext = database.CreateContext();
        Assert.That((await verificationContext.Organizations.SingleAsync()).Name,
            Is.EqualTo(name));
    }

    [Test]
    public async Task ConcurrentRequestsCreateIsolatedGraphsAndTransferTrialOnce()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(database, "api-concurrent", "concurrent@example.com");
        using var factory = new LicensingWebApplicationFactory(database, SignupTime.AddDays(1));
        using var firstClient = factory.CreateApiClient(signup.UserId);
        using var secondClient = factory.CreateApiClient(signup.UserId);

        var firstTask = PostOrganizationAsync(
            firstClient,
            JsonContent.Create(new { name = "First Concurrent API Organization" }));
        var secondTask = PostOrganizationAsync(
            secondClient,
            JsonContent.Create(new { name = "Second Concurrent API Organization" }));
        var responses = await Task.WhenAll(firstTask, secondTask);
        using var firstResponse = responses[0];
        using var secondResponse = responses[1];
        var results = new[]
        {
            (await firstResponse.Content.ReadFromJsonAsync<OrganizationCreationResponse>())!,
            (await secondResponse.Content.ReadFromJsonAsync<OrganizationCreationResponse>())!,
        };

        Assert.Multiple(() =>
        {
            Assert.That(responses.Select(response => response.StatusCode),
                Is.All.EqualTo(HttpStatusCode.Created));
            Assert.That(results.Select(result => result.CorrelationId), Is.Unique);
            Assert.That(results.Count(result => result.TrialWasTransferred), Is.EqualTo(1));
        });
        await using var verificationContext = database.CreateContext();
        var organizations = await verificationContext.Organizations.ToArrayAsync();
        var memberships = await verificationContext.OrganizationMemberships.ToArrayAsync();
        var organizationSeats = await verificationContext.Seats
            .Where(seat => results.Select(result => result.SeatId).Contains(seat.Id))
            .ToArrayAsync();
        var correlationIds = results.Select(result => result.CorrelationId).ToArray();
        var audits = await verificationContext.AuditRecords
            .Where(record => correlationIds.Contains(record.CorrelationId))
            .ToArrayAsync();
        Assert.Multiple(() =>
        {
            Assert.That(organizations.Select(organization => organization.Id),
                Is.EquivalentTo(results.Select(result => result.OrganizationId)));
            Assert.That(memberships.Select(membership => membership.Id),
                Is.EquivalentTo(results.Select(result => result.OwnerMembershipId)));
            Assert.That(organizationSeats.Select(seat => seat.Id),
                Is.EquivalentTo(results.Select(result => result.SeatId)));
        });
        foreach (var result in results)
        {
            Assert.That(
                AuditTargets(audits.Where(
                    audit => audit.CorrelationId == result.CorrelationId)),
                Is.EquivalentTo(
                    ExpectedAuditTargets(
                        result,
                        result.TrialWasTransferred ? signup.TrialId : null)));
        }
    }

    [Test]
    public async Task CancelledRequestRollsBackGraphTrialTransferAndAudits()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(database, "api-cancel", "cancel@example.com");
        var baseline = await CapturePersistenceAsync(database);
        var gate = new DatabaseCommandGate();
        var requestCompletionObserver = new RequestCompletionObserver(
            HttpMethods.Post,
            "/api/organizations");
        using var factory = new LicensingWebApplicationFactory(
            database,
            SignupTime.AddDays(1),
            useTestAuthentication: true,
            requestCompletionObserver: requestCompletionObserver,
            interceptors: [new OrganizationWriteGateInterceptor(gate)]);
        using var client = factory.CreateApiClient(signup.UserId);
        var token = await AntiforgeryTestClient.GetTokenAsync(client);
        using var request = CreateOrganizationRequest(
            token,
            JsonContent.Create(new { name = "Cancelled Organization" }));
        using var cancellation = new CancellationTokenSource();

        var responseTask = client.SendAsync(request, cancellation.Token);
        await gate.WaitUntilReachedAsync();
        try
        {
            cancellation.Cancel();
            Assert.ThrowsAsync<TaskCanceledException>(async () => await responseTask);
        }
        finally
        {
            gate.Release();
        }

        await requestCompletionObserver.WaitUntilCompletedAsync();
        Assert.That(requestCompletionObserver.WasCanceled, Is.True);
        AssertPersistenceUnchanged(baseline, await CapturePersistenceAsync(database));
    }

    private static async Task<HttpResponseMessage> PostOrganizationAsync(
        HttpClient client,
        HttpContent content,
        CancellationToken cancellationToken = default)
    {
        var token = await AntiforgeryTestClient.GetTokenAsync(client);
        using var request = CreateOrganizationRequest(token, content);
        return await client.SendAsync(request, cancellationToken);
    }

    private static HttpRequestMessage CreateOrganizationRequest(
        string antiforgeryToken,
        HttpContent content)
    {
        var request = new HttpRequestMessage(HttpMethod.Post, "/api/organizations")
        {
            Content = content,
        };
        AntiforgeryTestClient.AddToken(request, antiforgeryToken);
        return request;
    }

    private static IEnumerable<(AuditAction Action, AuditTargetType TargetType, Guid TargetId)>
        AuditTargets(IEnumerable<AuditRecord> records)
    {
        return records.Select(record => (record.Action, record.TargetType, record.TargetId));
    }

    private static IEnumerable<(
        AuditAction Action,
        AuditTargetType TargetType,
        Guid TargetId)> ExpectedAuditTargets(
            OrganizationCreationResponse result,
            Guid? transferredTrialId = null)
    {
        yield return (
            AuditAction.OrganizationCreated,
            AuditTargetType.Organization,
            result.OrganizationId);
        yield return (
            AuditAction.OrganizationOwnerAssigned,
            AuditTargetType.OrganizationMembership,
            result.OwnerMembershipId);
        yield return (AuditAction.SeatAssigned, AuditTargetType.Seat, result.SeatId);
        if (transferredTrialId is not null)
        {
            yield return (
                AuditAction.TrialTransferred,
                AuditTargetType.Trial,
                transferredTrialId.Value);
        }
    }

    private static async Task<PersistenceSnapshot> CapturePersistenceAsync(
        PostgresTestDatabase database)
    {
        await using var context = database.CreateContext();
        var trialValues = await context.Trials
            .AsNoTracking()
            .OrderBy(trial => trial.Id)
            .Select(trial => new
            {
                trial.Id,
                trial.BillingAccountId,
                trial.TransferredAt,
            })
            .ToArrayAsync();
        return new PersistenceSnapshot(
            await context.UserAccounts.CountAsync(),
            await context.Organizations.CountAsync(),
            await context.OrganizationMemberships.CountAsync(),
            await context.BillingAccounts.CountAsync(),
            await context.Seats.CountAsync(),
            await context.Trials.CountAsync(),
            await context.AuditRecords.CountAsync(),
            trialValues.Select(trial => new TrialSnapshot(
                    trial.Id,
                    trial.BillingAccountId,
                    trial.TransferredAt))
                .ToArray());
    }

    private static void AssertPersistenceUnchanged(
        PersistenceSnapshot before,
        PersistenceSnapshot after)
    {
        Assert.Multiple(() =>
        {
            Assert.That(after.UserCount, Is.EqualTo(before.UserCount));
            Assert.That(after.OrganizationCount, Is.EqualTo(before.OrganizationCount));
            Assert.That(after.MembershipCount, Is.EqualTo(before.MembershipCount));
            Assert.That(after.BillingAccountCount, Is.EqualTo(before.BillingAccountCount));
            Assert.That(after.SeatCount, Is.EqualTo(before.SeatCount));
            Assert.That(after.TrialCount, Is.EqualTo(before.TrialCount));
            Assert.That(after.AuditCount, Is.EqualTo(before.AuditCount));
            Assert.That(after.Trials, Is.EqualTo(before.Trials));
        });
    }

    private sealed record PersistenceSnapshot(
        int UserCount,
        int OrganizationCount,
        int MembershipCount,
        int BillingAccountCount,
        int SeatCount,
        int TrialCount,
        int AuditCount,
        IReadOnlyList<TrialSnapshot> Trials);

    private sealed record TrialSnapshot(
        Guid Id,
        Guid BillingAccountId,
        DateTimeOffset? TransferredAt);
}
