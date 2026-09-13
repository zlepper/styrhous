using System.Net;
using System.Net.Http.Json;
using System.Text.Json;
using Microsoft.AspNetCore.Http;
using Styrhous.Licensing.Api.Billing;
using Styrhous.Licensing.Domain.Billing;
using Styrhous.Licensing.Domain.Organizations;
using Styrhous.Licensing.Persistence;
using Styrhous.Licensing.Tests.Persistence;
using static Styrhous.Licensing.Tests.Persistence.LicensingPersistenceScenario;

namespace Styrhous.Licensing.Tests.Api;

[TestFixture]
[Parallelizable(ParallelScope.All)]
public sealed class BillingAccountApiTests
{
    private static readonly JsonSerializerOptions WebJson =
        new(JsonSerializerDefaults.Web);

    private static readonly string[] RootProperties = ["reasonCode", "accounts"];

    private static readonly string[] AccountProperties =
    [
        "billingAccountId",
        "accountKind",
        "organizationId",
        "organizationName",
        "organizationRole",
        "canManageBilling",
        "assignedSeatCount",
        "entitlement",
        "trial",
        "subscription",
        "canStartCheckout",
    ];

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

    private static readonly string[] SubscriptionProperties =
    [
        "subscriptionId",
        "status",
        "seatQuantity",
        "cancelAtPeriodEnd",
        "currentPeriodStartedAt",
        "currentPeriodEndsAt",
        "projectedAt",
    ];

    private static readonly string[] TrialProperties =
    [
        "trialId",
        "startedAt",
        "endsAt",
        "terminatedAt",
        "transferredAt",
        "isActive",
    ];

    [Test]
    public async Task AuthenticatedOwnerReceivesFrontendReadyBillingState()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(
            database,
            "api-billing-account",
            "api-billing-account@example.com");
        await using (var context = database.CreateContext())
        {
            context.CommercialSubscriptions.Add(
                CommercialSubscription.Create(
                    signup.PersonalBillingAccountId,
                    new CommercialSubscriptionProjection(
                        "cus_must_not_leak",
                        "sub_must_not_leak",
                        "price_must_not_leak",
                        CommercialSubscriptionStatus.Active,
                        seatQuantity: 1,
                        cancelAtPeriodEnd: false,
                        SignupTime.AddDays(1),
                        SignupTime.AddDays(31),
                        SignupTime.AddDays(2))));
            await context.SaveChangesAsync();
        }
        using var factory = new LicensingWebApplicationFactory(
            database,
            SignupTime.AddDays(3));
        using var client = factory.CreateApiClient(signup.UserId);

        using var response = await client.GetAsync("/api/billing-accounts");
        var responseBody = await response.Content.ReadAsStringAsync();
        var body = JsonSerializer.Deserialize<BillingAccountListResponse>(responseBody, WebJson);
        using var json = JsonDocument.Parse(responseBody);
        var account = body!.Accounts.Single();

        Assert.Multiple(() =>
        {
            Assert.That(response.StatusCode, Is.EqualTo(HttpStatusCode.OK));
            Assert.That(response.Content.Headers.ContentType?.MediaType, Is.EqualTo("application/json"));
            Assert.That(response.Headers.CacheControl?.NoStore, Is.True);
            Assert.That(
                json.RootElement.EnumerateObject().Select(property => property.Name),
                Is.EqualTo(RootProperties));
            Assert.That(
                json.RootElement.GetProperty("accounts")[0]
                    .EnumerateObject()
                    .Select(property => property.Name),
                Is.EqualTo(AccountProperties));
            Assert.That(
                json.RootElement.GetProperty("accounts")[0]
                    .GetProperty("entitlement")
                    .EnumerateObject()
                    .Select(property => property.Name),
                Is.EqualTo(EntitlementProperties));
            Assert.That(
                json.RootElement.GetProperty("accounts")[0]
                    .GetProperty("trial")
                    .EnumerateObject()
                    .Select(property => property.Name),
                Is.EqualTo(TrialProperties));
            Assert.That(
                json.RootElement.GetProperty("accounts")[0]
                    .GetProperty("subscription")
                    .EnumerateObject()
                    .Select(property => property.Name),
                Is.EqualTo(SubscriptionProperties));
            Assert.That(responseBody, Does.Not.Contain("must_not_leak"));
            Assert.That(body.ReasonCode, Is.EqualTo(BillingAccountReasonCodes.Listed));
            Assert.That(account.BillingAccountId, Is.EqualTo(signup.PersonalBillingAccountId));
            Assert.That(account.AccountKind, Is.EqualTo("personal"));
            Assert.That(account.OrganizationId, Is.Null);
            Assert.That(account.OrganizationName, Is.Null);
            Assert.That(account.OrganizationRole, Is.Null);
            Assert.That(account.CanManageBilling, Is.True);
            Assert.That(account.AssignedSeatCount, Is.EqualTo(1));
            Assert.That(account.Entitlement.SeatId, Is.EqualTo(signup.SeatId));
            Assert.That(account.Entitlement.BillingAccountId,
                Is.EqualTo(signup.PersonalBillingAccountId));
            Assert.That(account.Entitlement.State, Is.EqualTo("commercial"));
            Assert.That(account.Entitlement.ReasonCode, Is.EqualTo("active_subscription"));
            Assert.That(account.Entitlement.IsEligible, Is.True);
            Assert.That(account.Trial, Is.Not.Null);
            Assert.That(account.Trial!.TrialId.Version, Is.EqualTo(7));
            Assert.That(account.Trial.TerminatedAt, Is.Null);
            Assert.That(account.Trial.IsActive, Is.True);
            Assert.That(account.Subscription, Is.Not.Null);
            Assert.That(account.Subscription!.SubscriptionId.Version, Is.EqualTo(7));
            Assert.That(account.Subscription.Status, Is.EqualTo("active"));
        });
    }

    [TestCase(null, true)]
    [TestCase(CommercialSubscriptionStatus.Canceled, true)]
    [TestCase(CommercialSubscriptionStatus.IncompleteExpired, true)]
    [TestCase(CommercialSubscriptionStatus.Active, false)]
    [TestCase(CommercialSubscriptionStatus.PastDue, false)]
    [TestCase(CommercialSubscriptionStatus.Unpaid, false)]
    [TestCase(CommercialSubscriptionStatus.Paused, false)]
    [TestCase(CommercialSubscriptionStatus.Incomplete, false)]
    [TestCase(CommercialSubscriptionStatus.Trialing, false)]
    public async Task CheckoutCapabilityReflectsSubscriptionLifecycle(CommercialSubscriptionStatus? status, bool expected)
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var user = await SignUpAsync(database, "checkout-capability", "checkout-capability@example.com");
        if (status is { } subscriptionStatus)
        {
            await using var context = database.CreateContext();
            context.CommercialSubscriptions.Add(CommercialSubscription.Create(user.PersonalBillingAccountId,
                new CommercialSubscriptionProjection("cus_capability", "sub_capability", "price_monthly", subscriptionStatus,
                    1, false, SignupTime, SignupTime.AddDays(1), SignupTime.AddDays(2))));
            await context.SaveChangesAsync();
        }
        using var factory = new LicensingWebApplicationFactory(database, SignupTime.AddDays(3));
        using var client = factory.CreateApiClient(user.UserId);
        var response = await client.GetFromJsonAsync<BillingAccountListResponse>("/api/billing-accounts");
        Assert.That(response!.Accounts.Single().CanStartCheckout, Is.EqualTo(expected));
    }

    [TestCase(OrganizationRole.Owner, "owner", true)]
    [TestCase(OrganizationRole.Admin, "admin", false)]
    [TestCase(OrganizationRole.Member, "member", false)]
    public async Task OrganizationRoleControlsManageabilityWithoutHidingStatus(
        OrganizationRole role,
        string expectedRole,
        bool expectedCanManage)
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(
            database,
            $"api-billing-owner-{role}",
            $"owner-{role}@example.com");
        var actor = role == OrganizationRole.Owner
            ? owner
            : await SignUpAsync(
                database,
                $"api-billing-actor-{role}",
                $"actor-{role}@example.com");
        var outsider = await SignUpAsync(
            database,
            $"api-billing-outsider-{role}",
            $"outsider-{role}@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            $"Visible {role} Billing",
            SignupTime.AddDays(1));
        if (role != OrganizationRole.Owner)
        {
            await AddOrganizationMemberAsync(
                database,
                organization,
                actor.UserId,
                role,
                joinedAt: SignupTime.AddDays(2));
        }

        await CreateOrganizationAsync(
            database,
            outsider.UserId,
            $"Hidden {role} Billing",
            SignupTime.AddDays(1));
        await using (var context = database.CreateContext())
        {
            context.CommercialSubscriptions.Add(
                CommercialSubscription.Create(
                    organization.BillingAccountId,
                    new CommercialSubscriptionProjection(
                        $"cus_org_must_not_leak_{role}",
                        $"sub_org_must_not_leak_{role}",
                        $"price_org_must_not_leak_{role}",
                        CommercialSubscriptionStatus.PastDue,
                        seatQuantity: 4,
                        cancelAtPeriodEnd: false,
                        SignupTime.AddDays(1),
                        SignupTime.AddDays(31),
                        SignupTime.AddDays(2))));
            await context.SaveChangesAsync();
        }

        using var factory = new LicensingWebApplicationFactory(
            database,
            SignupTime.AddDays(3));
        using var client = factory.CreateApiClient(actor.UserId);

        using var response = await client.GetAsync("/api/billing-accounts");
        var responseBody = await response.Content.ReadAsStringAsync();
        var body = JsonSerializer.Deserialize<BillingAccountListResponse>(responseBody, WebJson);
        var accounts = body!.Accounts;
        using var json = JsonDocument.Parse(responseBody);
        var organizationJson = json.RootElement.GetProperty("accounts")[1];

        Assert.Multiple(() =>
        {
            Assert.That(response.StatusCode, Is.EqualTo(HttpStatusCode.OK));
            Assert.That(
                organizationJson.EnumerateObject().Select(property => property.Name),
                Is.EqualTo(AccountProperties));
            Assert.That(
                organizationJson.GetProperty("entitlement")
                    .EnumerateObject()
                    .Select(property => property.Name),
                Is.EqualTo(EntitlementProperties));
            Assert.That(
                organizationJson.GetProperty("subscription")
                    .EnumerateObject()
                    .Select(property => property.Name),
                Is.EqualTo(SubscriptionProperties));
            Assert.That(accounts, Has.Count.EqualTo(2));
            Assert.That(accounts[0].AccountKind, Is.EqualTo("personal"));
            Assert.That(accounts[1].BillingAccountId, Is.EqualTo(organization.BillingAccountId));
            Assert.That(accounts[1].AccountKind, Is.EqualTo("organization"));
            Assert.That(accounts[1].OrganizationId, Is.EqualTo(organization.OrganizationId));
            Assert.That(accounts[1].OrganizationName, Is.EqualTo($"Visible {role} Billing"));
            Assert.That(accounts[1].OrganizationRole, Is.EqualTo(expectedRole));
            Assert.That(accounts[1].CanManageBilling, Is.EqualTo(expectedCanManage));
            Assert.That(accounts[1].CanStartCheckout, Is.False);
            Assert.That(accounts[1].Entitlement.State, Is.EqualTo("grace"));
            Assert.That(accounts[1].Entitlement.ReasonCode,
                Is.EqualTo("subscription_past_due"));
            Assert.That(accounts[1].Subscription!.Status, Is.EqualTo("past_due"));
            Assert.That(responseBody, Does.Not.Contain("must_not_leak"));
            Assert.That(responseBody, Does.Not.Contain("Hidden"));
        });
    }

    [Test]
    public async Task ClientCancellationStopsServerSideBillingQuery()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(
            database,
            "api-billing-cancel",
            "api-billing-cancel@example.com");
        var gate = new DatabaseCommandGate();
        const string path = "/api/billing-accounts";
        var completionObserver = new RequestCompletionObserver(HttpMethods.Get, path);
        var queryGateInterceptor = new DatabaseCommandGateInterceptor(
            gate,
            "FROM seats AS s");
        using var factory = new LicensingWebApplicationFactory(
            database,
            SignupTime.AddDays(2),
            useTestAuthentication: true,
            requestCompletionObserver: completionObserver,
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
            await completionObserver.WaitUntilCompletedAsync();
        }
        finally
        {
            gate.Release();
        }

        Assert.That(completionObserver.WasCanceled, Is.True);
    }

    [Test]
    public async Task StaleAuthenticatedUserIsRejectedWithoutBillingDetails()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        using var factory = new LicensingWebApplicationFactory(database, SignupTime);
        using var client = factory.CreateApiClient(Guid.CreateVersion7());

        using var response = await client.GetAsync("/api/billing-accounts");

        Assert.Multiple(() =>
        {
            Assert.That(response.StatusCode, Is.EqualTo(HttpStatusCode.Unauthorized));
            Assert.That(response.Headers.CacheControl?.NoStore, Is.True);
            Assert.That(response.Content.Headers.ContentLength, Is.EqualTo(0));
        });
    }

    [Test]
    public async Task MissingAuthenticationIsChallengedWithoutRedirect()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        using var factory = new LicensingWebApplicationFactory(database, SignupTime);
        using var client = factory.CreateApiClient();

        using var response = await client.GetAsync("/api/billing-accounts");

        Assert.Multiple(() =>
        {
            Assert.That(response.StatusCode, Is.EqualTo(HttpStatusCode.Unauthorized));
            Assert.That(response.Headers.Location, Is.Null);
            Assert.That(response.Headers.CacheControl?.NoStore, Is.True);
        });
    }
}
