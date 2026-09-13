using System.Net;
using System.Net.Http.Json;
using System.Text.Json;
using Microsoft.AspNetCore.Http;
using Microsoft.EntityFrameworkCore;
using Styrhous.Licensing.Application.Organizations;
using Styrhous.Licensing.Domain.Billing;
using Styrhous.Licensing.Domain.Organizations;
using Styrhous.Licensing.Persistence;
using Styrhous.Licensing.Tests.Persistence;
using static Styrhous.Licensing.Tests.Persistence.LicensingPersistenceScenario;

namespace Styrhous.Licensing.Tests.Api;

[TestFixture]
[Parallelizable(ParallelScope.All)]
public sealed class EntitlementApiTests
{
    private static readonly JsonSerializerOptions WebJson =
        new(JsonSerializerDefaults.Web);

    private static readonly string[] RootProperties = ["entitlements"];

    private static readonly string[] EntitlementProperties =
    [
        "seatId",
        "billingAccountId",
        "state",
        "reasonCode",
        "isEligible",
        "validFrom",
        "validUntil",
    ];

    [Test]
    public async Task AuthenticatedUserReceivesFrontendReadyActiveTrial()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(database, "api-entitlement", "license@example.com");
        using var factory = new LicensingWebApplicationFactory(database, SignupTime.AddDays(2));
        using var client = factory.CreateApiClient(signup.UserId);

        using var response = await client.GetAsync("/api/entitlements");
        var responseBody = await response.Content.ReadAsStringAsync();
        var body = JsonSerializer.Deserialize<EntitlementListResponse>(responseBody, WebJson);
        using var responseJson = JsonDocument.Parse(responseBody);
        var entitlement = body!.Entitlements.Single();

        Assert.Multiple(() =>
        {
            Assert.That(response.StatusCode, Is.EqualTo(HttpStatusCode.OK));
            Assert.That(response.Content.Headers.ContentType?.MediaType,
                Is.EqualTo("application/json"));
            Assert.That(response.Headers.CacheControl?.NoStore, Is.True);
            Assert.That(
                responseJson.RootElement.EnumerateObject().Select(property => property.Name),
                Is.EqualTo(RootProperties));
            Assert.That(
                responseJson.RootElement.GetProperty("entitlements")[0]
                    .EnumerateObject()
                    .Select(property => property.Name),
                Is.EquivalentTo(EntitlementProperties));
            Assert.That(entitlement.SeatId, Is.EqualTo(signup.SeatId));
            Assert.That(entitlement.BillingAccountId,
                Is.EqualTo(signup.PersonalBillingAccountId));
            Assert.That(entitlement.State, Is.EqualTo("trial"));
            Assert.That(entitlement.ReasonCode, Is.EqualTo("active_trial"));
            Assert.That(entitlement.IsEligible, Is.True);
            Assert.That(entitlement.ValidFrom, Is.EqualTo(SignupTime));
            Assert.That(entitlement.ValidUntil, Is.EqualTo(SignupTime.AddDays(30)));
            Assert.That(entitlement.SeatId.Version, Is.EqualTo(7));
            Assert.That(entitlement.BillingAccountId.Version, Is.EqualTo(7));
        });
    }

    [Test]
    public async Task AuthenticatedUserReceivesFrontendReadyCommercialGrace()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(
            database,
            "api-commercial-entitlement",
            "commercial@example.com");
        await using (var context = database.CreateContext())
        {
            context.CommercialSubscriptions.Add(
                CommercialSubscription.Create(
                    signup.PersonalBillingAccountId,
                    new CommercialSubscriptionProjection(
                        "cus_api",
                        "sub_api",
                        "price_api",
                        CommercialSubscriptionStatus.PastDue,
                        seatQuantity: 1,
                        cancelAtPeriodEnd: false,
                        SignupTime.AddDays(30),
                        SignupTime.AddDays(60),
                        SignupTime.AddDays(30))));
            await context.SaveChangesAsync();
        }
        using var factory = new LicensingWebApplicationFactory(database, SignupTime.AddDays(31));
        using var client = factory.CreateApiClient(signup.UserId);

        using var response = await client.GetAsync("/api/entitlements");
        var body = await response.Content.ReadFromJsonAsync<EntitlementListResponse>(WebJson);
        var entitlement = body!.Entitlements.Single();

        Assert.Multiple(() =>
        {
            Assert.That(response.StatusCode, Is.EqualTo(HttpStatusCode.OK));
            AssertEntitlement(
                entitlement,
                signup.PersonalBillingAccountId,
                "grace",
                "subscription_past_due",
                isEligible: true,
                SignupTime.AddDays(30),
                SignupTime.AddDays(60));
        });
    }

    [Test]
    public async Task FundedScheduledCancellationReturnsFrontendReadyCommercialState()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(
            database,
            "api-commercial-cancellation",
            "cancellation@example.com");
        await using (var context = database.CreateContext())
        {
            context.CommercialSubscriptions.Add(
                CommercialSubscription.Create(
                    signup.PersonalBillingAccountId,
                    new CommercialSubscriptionProjection(
                        "cus_api_cancellation",
                        "sub_api_cancellation",
                        "price_api_cancellation",
                        CommercialSubscriptionStatus.Active,
                        seatQuantity: 1,
                        cancelAtPeriodEnd: true,
                        SignupTime.AddDays(30),
                        SignupTime.AddDays(60),
                        SignupTime.AddDays(30))));
            await context.SaveChangesAsync();
        }
        using var factory = new LicensingWebApplicationFactory(
            database,
            SignupTime.AddDays(31));
        using var client = factory.CreateApiClient(signup.UserId);

        using var response = await client.GetAsync("/api/entitlements");
        var body = await response.Content.ReadFromJsonAsync<EntitlementListResponse>(WebJson);
        var entitlement = body!.Entitlements.Single();

        Assert.Multiple(() =>
        {
            Assert.That(response.StatusCode, Is.EqualTo(HttpStatusCode.OK));
            Assert.That(entitlement.SeatId, Is.EqualTo(signup.SeatId));
            AssertEntitlement(
                entitlement,
                signup.PersonalBillingAccountId,
                "commercial",
                "subscription_cancels_at_period_end",
                isEligible: true,
                SignupTime.AddDays(30),
                SignupTime.AddDays(60));
        });
    }

    [Test]
    public async Task UnfundedOrganizationSeatReturnsStableCapacityReasonDuringTrial()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(
            database,
            "api-capacity-owner",
            "owner@example.com");
        var member = await SignUpAsync(
            database,
            "api-capacity-member",
            "member@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "API Capacity",
            SignupTime.AddDays(1));
        var memberSeat = await AddOrganizationMemberAsync(
            database,
            organization,
            member.UserId,
            OrganizationRole.Member,
            joinedAt: SignupTime.AddDays(2));
        await using (var context = database.CreateContext())
        {
            context.CommercialSubscriptions.Add(
                CommercialSubscription.Create(
                    organization.BillingAccountId,
                    new CommercialSubscriptionProjection(
                        "cus_api_capacity",
                        "sub_api_capacity",
                        "price_api_capacity",
                        CommercialSubscriptionStatus.Active,
                        seatQuantity: 1,
                        cancelAtPeriodEnd: false,
                        SignupTime,
                        SignupTime.AddDays(60),
                        SignupTime)));
            await context.SaveChangesAsync();
        }
        using var factory = new LicensingWebApplicationFactory(
            database,
            SignupTime.AddDays(2));
        using var client = factory.CreateApiClient(member.UserId);

        using var response = await client.GetAsync("/api/entitlements");
        var body = await response.Content.ReadFromJsonAsync<EntitlementListResponse>(WebJson);
        var entitlement = body!.Entitlements.Single(
            item => item.BillingAccountId == organization.BillingAccountId);

        Assert.Multiple(() =>
        {
            Assert.That(response.StatusCode, Is.EqualTo(HttpStatusCode.OK));
            Assert.That(entitlement.SeatId, Is.EqualTo(memberSeat.SeatId));
            AssertEntitlement(
                entitlement,
                organization.BillingAccountId,
                "evaluation",
                "subscription_seat_capacity_exceeded",
                isEligible: false,
                SignupTime,
                SignupTime.AddDays(60));
        });
    }

    [Test]
    public async Task ListingReflectsTransferredTrialAcrossEveryOwnedSeat()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(
            database,
            "api-entitlement-transfer",
            "transfer@example.com");
        var trialOrganization = await CreateOrganizationAsync(
            database,
            signup.UserId,
            "Trial Organization",
            SignupTime.AddDays(1));
        var unlicensedOrganization = await CreateOrganizationAsync(
            database,
            signup.UserId,
            "Unlicensed Organization",
            SignupTime.AddDays(1).AddMinutes(1));
        using var factory = new LicensingWebApplicationFactory(database, SignupTime.AddDays(2));
        using var client = factory.CreateApiClient(signup.UserId);

        using var response = await client.GetAsync("/api/entitlements");
        var responseBody = await response.Content.ReadAsStringAsync();
        var body = JsonSerializer.Deserialize<EntitlementListResponse>(responseBody, WebJson);
        using var responseJson = JsonDocument.Parse(responseBody);

        Assert.That(response.StatusCode, Is.EqualTo(HttpStatusCode.OK));
        Assert.That(
            body!.Entitlements.Select(entitlement => entitlement.SeatId),
            Is.EqualTo(
                new[]
                {
                    signup.SeatId,
                    trialOrganization.SeatId,
                    unlicensedOrganization.SeatId,
                }));
        Assert.That(
            responseJson.RootElement.GetProperty("entitlements")[0]
                .GetProperty("validFrom").ValueKind,
            Is.EqualTo(JsonValueKind.Null));
        Assert.That(
            responseJson.RootElement.GetProperty("entitlements")[0]
                .GetProperty("validUntil").ValueKind,
            Is.EqualTo(JsonValueKind.Null));
        Assert.Multiple(() =>
        {
            AssertEntitlement(
                body.Entitlements[0],
                signup.PersonalBillingAccountId,
                "evaluation",
                "no_valid_entitlement",
                isEligible: false,
                validFrom: null,
                validUntil: null);
            AssertEntitlement(
                body.Entitlements[1],
                trialOrganization.BillingAccountId,
                "trial",
                "active_trial",
                isEligible: true,
                SignupTime,
                SignupTime.AddDays(30));
            AssertEntitlement(
                body.Entitlements[2],
                unlicensedOrganization.BillingAccountId,
                "evaluation",
                "no_valid_entitlement",
                isEligible: false,
                validFrom: null,
                validUntil: null);
        });
    }

    [Test]
    public async Task ClientCancellationStopsTheServerSideEntitlementQuery()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(database, "api-read-cancel", "read-cancel@example.com");
        var gate = new DatabaseCommandGate();
        var requestCompletionObserver = new RequestCompletionObserver(
            HttpMethods.Get,
            "/api/entitlements");
        var queryGateInterceptor = new DatabaseCommandGateInterceptor(gate, "FROM seats");
        using var factory = new LicensingWebApplicationFactory(
            database,
            SignupTime,
            useTestAuthentication: true,
            requestCompletionObserver: requestCompletionObserver,
            interceptors: [queryGateInterceptor]);
        using var client = factory.CreateApiClient(signup.UserId);
        using var cancellation = new CancellationTokenSource();

        var responseTask = client.GetAsync("/api/entitlements", cancellation.Token);
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
    public async Task ClientCancellationStopsTheCommercialSeatFundingQuery()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(
            database,
            "api-funding-cancel",
            "funding-cancel@example.com");
        await using (var context = database.CreateContext())
        {
            context.CommercialSubscriptions.Add(
                CommercialSubscription.Create(
                    signup.PersonalBillingAccountId,
                    new CommercialSubscriptionProjection(
                        "cus_api_funding_cancel",
                        "sub_api_funding_cancel",
                        "price_api_funding_cancel",
                        CommercialSubscriptionStatus.Active,
                        seatQuantity: 1,
                        cancelAtPeriodEnd: false,
                        SignupTime,
                        SignupTime.AddDays(60),
                        SignupTime)));
            await context.SaveChangesAsync();
        }
        var gate = new DatabaseCommandGate();
        var requestCompletionObserver = new RequestCompletionObserver(
            HttpMethods.Get,
            "/api/entitlements");
        var queryGateInterceptor = new DatabaseCommandGateInterceptor(
            gate,
            "ORDER BY s.billing_account_id");
        using var factory = new LicensingWebApplicationFactory(
            database,
            SignupTime.AddDays(1),
            useTestAuthentication: true,
            requestCompletionObserver: requestCompletionObserver,
            interceptors: [queryGateInterceptor]);
        using var client = factory.CreateApiClient(signup.UserId);
        using var cancellation = new CancellationTokenSource();

        var responseTask = client.GetAsync("/api/entitlements", cancellation.Token);
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
    public async Task TrialEndBoundaryReturnsExpiredEvaluation()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(database, "api-expired", "expired@example.com");
        using var factory = new LicensingWebApplicationFactory(database, SignupTime.AddDays(30));
        using var client = factory.CreateApiClient(signup.UserId);

        using var response = await client.GetAsync("/api/entitlements");
        var body = await response.Content.ReadFromJsonAsync<EntitlementListResponse>(WebJson);
        var entitlement = body!.Entitlements.Single();

        Assert.Multiple(() =>
        {
            Assert.That(response.StatusCode, Is.EqualTo(HttpStatusCode.OK));
            Assert.That(entitlement.State, Is.EqualTo("evaluation"));
            Assert.That(entitlement.ReasonCode, Is.EqualTo("trial_expired"));
            Assert.That(entitlement.IsEligible, Is.False);
            Assert.That(entitlement.ValidFrom, Is.EqualTo(SignupTime));
            Assert.That(entitlement.ValidUntil, Is.EqualTo(SignupTime.AddDays(30)));
        });
    }

    [Test]
    public async Task QueryStringCannotSelectAnotherUsersEntitlements()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var actor = await SignUpAsync(database, "api-scope-actor", "actor@example.com");
        var other = await SignUpAsync(database, "api-scope-other", "other@example.com");
        using var factory = new LicensingWebApplicationFactory(database, SignupTime.AddDays(1));
        using var client = factory.CreateApiClient(actor.UserId);

        using var response = await client.GetAsync(
            $"/api/entitlements?userId={other.UserId}");
        var body = await response.Content.ReadFromJsonAsync<EntitlementListResponse>(WebJson);

        Assert.That(response.StatusCode, Is.EqualTo(HttpStatusCode.OK));
        Assert.That(
            body!.Entitlements.Select(entitlement => entitlement.SeatId),
            Is.EqualTo(new[] { actor.SeatId }));
    }

    [Test]
    public async Task StaleAuthenticatedUserIsRejectedWithoutEntitlementDetails()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        using var factory = new LicensingWebApplicationFactory(database, SignupTime);
        using var client = factory.CreateApiClient(Guid.CreateVersion7());

        using var response = await client.GetAsync("/api/entitlements");
        var responseBody = await response.Content.ReadAsStringAsync();

        Assert.Multiple(() =>
        {
            Assert.That(response.StatusCode, Is.EqualTo(HttpStatusCode.Unauthorized));
            Assert.That(response.Headers.CacheControl?.NoStore, Is.True);
            Assert.That(responseBody, Is.Empty);
        });
    }

    [TestCase("36c1e76c-8841-44f9-8864-6e7ffe632ef1")]
    [TestCase(TestIdentifiers.Version7WithNonRfcVariantText)]
    public async Task UnknownAuthenticatedUserIsRejectedRegardlessOfIdentifierVersion(
        string userId)
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var commandCounter = new DatabaseCommandCounterInterceptor();
        using var factory = new LicensingWebApplicationFactory(
            database,
            SignupTime,
            useTestAuthentication: true,
            requestCompletionObserver: null,
            interceptors: [commandCounter]);
        using var client = factory.CreateApiClient(Guid.Parse(userId));

        using var response = await client.GetAsync("/api/entitlements");

        Assert.Multiple(() =>
        {
            Assert.That(response.StatusCode, Is.EqualTo(HttpStatusCode.Unauthorized));
            Assert.That(commandCounter.CommandCount, Is.GreaterThan(0));
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

        using var response = await client.GetAsync("/api/entitlements");

        Assert.Multiple(() =>
        {
            Assert.That(response.StatusCode, Is.EqualTo(HttpStatusCode.Unauthorized));
            Assert.That(response.Headers.Location, Is.Null);
        });
    }

    private static void AssertEntitlement(
        EntitlementResponse entitlement,
        Guid billingAccountId,
        string state,
        string reasonCode,
        bool isEligible,
        DateTimeOffset? validFrom,
        DateTimeOffset? validUntil)
    {
        Assert.That(entitlement.BillingAccountId, Is.EqualTo(billingAccountId));
        Assert.That(entitlement.State, Is.EqualTo(state));
        Assert.That(entitlement.ReasonCode, Is.EqualTo(reasonCode));
        Assert.That(entitlement.IsEligible, Is.EqualTo(isEligible));
        Assert.That(entitlement.ValidFrom, Is.EqualTo(validFrom));
        Assert.That(entitlement.ValidUntil, Is.EqualTo(validUntil));
    }

    private sealed record EntitlementListResponse(
        IReadOnlyList<EntitlementResponse> Entitlements);

    private sealed record EntitlementResponse(
        Guid SeatId,
        Guid BillingAccountId,
        string State,
        string ReasonCode,
        bool IsEligible,
        DateTimeOffset? ValidFrom,
        DateTimeOffset? ValidUntil);
}
