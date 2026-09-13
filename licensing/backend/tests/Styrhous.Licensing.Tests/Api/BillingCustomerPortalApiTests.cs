using System.Net;
using System.Net.Http.Json;
using System.Text.Json;
using Styrhous.Licensing.Application.Billing;
using Styrhous.Licensing.Domain.Billing;
using Styrhous.Licensing.Domain.Organizations;
using Styrhous.Licensing.Persistence;
using Styrhous.Licensing.Tests.Persistence;
using static Styrhous.Licensing.Tests.Persistence.LicensingPersistenceScenario;

namespace Styrhous.Licensing.Tests.Api;

[TestFixture]
[Parallelizable(ParallelScope.All)]
public sealed class BillingCustomerPortalApiTests
{
    private static readonly string[] SuccessProperties = ["reasonCode", "redirectUrl"];
    private static readonly string[] PersonalCustomerIds = ["cus_portal_api_owner"];
    private static readonly string[] AuthoritativeCustomerIds =
        ["cus_portal_api_authoritative"];
    private static readonly string[] OrganizationCustomerIds =
        ["cus_portal_api_organization"];

    [Test]
    public async Task PersonalOwnerCreatesFrontendReadyCustomerPortalSession()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(
            database,
            "customer-portal-api-owner",
            "customer-portal-api-owner@example.com");
        await ProjectSubscriptionAsync(
            database,
            signup.PersonalBillingAccountId,
            "cus_portal_api_owner");
        var provider = new ApiCustomerPortalProvider();
        using var factory = CreateFactory(database, provider);
        using var client = factory.CreateApiClient(signup.UserId);

        using var response = await PostCustomerPortalAsync(
            client,
            signup.PersonalBillingAccountId);
        using var body = JsonDocument.Parse(await response.Content.ReadAsStringAsync());

        Assert.Multiple(() =>
        {
            Assert.That(response.StatusCode, Is.EqualTo(HttpStatusCode.OK));
            Assert.That(response.Headers.CacheControl?.NoStore, Is.True);
            Assert.That(
                body.RootElement.EnumerateObject().Select(property => property.Name),
                Is.EqualTo(SuccessProperties));
            Assert.That(
                body.RootElement.GetProperty("reasonCode").GetString(),
                Is.EqualTo("customer_portal_session_created"));
            Assert.That(
                body.RootElement.GetProperty("redirectUrl").GetString(),
                Is.EqualTo("https://billing.stripe.test/api-session"));
            Assert.That(provider.ExternalCustomerIds, Is.EqualTo(PersonalCustomerIds));
        });
    }

    [Test]
    public async Task BrowserOwnedStripeInputsCannotOverrideLocalProjection()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(
            database,
            "customer-portal-api-forged-input",
            "customer-portal-api-forged-input@example.com");
        await ProjectSubscriptionAsync(
            database,
            signup.PersonalBillingAccountId,
            "cus_portal_api_authoritative");
        var provider = new ApiCustomerPortalProvider();
        using var factory = CreateFactory(database, provider);
        using var client = factory.CreateApiClient(signup.UserId);

        using var response = await PostCustomerPortalAsync(
            client,
            signup.PersonalBillingAccountId.ToString(),
            new
            {
                customer = "cus_attacker",
                configuration = "bpc_attacker",
                returnUrl = "https://attacker.example/return",
                redirectUrl = "https://attacker.example/session",
            });
        using var body = JsonDocument.Parse(await response.Content.ReadAsStringAsync());

        Assert.Multiple(() =>
        {
            Assert.That(response.StatusCode, Is.EqualTo(HttpStatusCode.OK));
            Assert.That(
                body.RootElement.EnumerateObject().Select(property => property.Name),
                Is.EqualTo(SuccessProperties));
            Assert.That(
                body.RootElement.GetProperty("redirectUrl").GetString(),
                Is.EqualTo("https://billing.stripe.test/api-session"));
            Assert.That(
                provider.ExternalCustomerIds,
                Is.EqualTo(AuthoritativeCustomerIds));
        });
    }

    [Test]
    public async Task OrganizationOwnerCanCreateCustomerPortalSession()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(
            database,
            "customer-portal-api-organization-owner",
            "customer-portal-api-organization-owner@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Customer Portal API organization",
            SignupTime.AddDays(1));
        await ProjectSubscriptionAsync(
            database,
            organization.BillingAccountId,
            "cus_portal_api_organization");
        var provider = new ApiCustomerPortalProvider();
        using var factory = CreateFactory(database, provider);
        using var client = factory.CreateApiClient(owner.UserId);

        using var response = await PostCustomerPortalAsync(
            client,
            organization.BillingAccountId);

        Assert.Multiple(() =>
        {
            Assert.That(response.StatusCode, Is.EqualTo(HttpStatusCode.OK));
            Assert.That(provider.ExternalCustomerIds, Is.EqualTo(OrganizationCustomerIds));
        });
    }

    [Test]
    public async Task AuthenticationAndAntiforgeryAreRequiredBeforeCustomerPortal()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(
            database,
            "customer-portal-api-security",
            "customer-portal-api-security@example.com");
        var provider = new ApiCustomerPortalProvider();
        using var factory = CreateFactory(database, provider);
        using var anonymous = factory.CreateApiClient();
        using var stale = factory.CreateApiClient(Guid.CreateVersion7());
        using var owner = factory.CreateApiClient(signup.UserId);

        using var anonymousResponse = await anonymous.PostAsync(
            Path(signup.PersonalBillingAccountId),
            content: null);
        using var staleResponse = await PostCustomerPortalAsync(
            stale,
            signup.PersonalBillingAccountId);
        using var noTokenResponse = await owner.PostAsync(
            Path(signup.PersonalBillingAccountId),
            content: null);

        Assert.Multiple(() =>
        {
            Assert.That(anonymousResponse.StatusCode, Is.EqualTo(HttpStatusCode.Unauthorized));
            Assert.That(staleResponse.StatusCode, Is.EqualTo(HttpStatusCode.Unauthorized));
            Assert.That(noTokenResponse.StatusCode, Is.EqualTo(HttpStatusCode.BadRequest));
            Assert.That(provider.ExternalCustomerIds, Is.Empty);
        });
    }

    [Test]
    public async Task AccountVisibilityPermissionAndSubscriptionUseStableErrors()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(
            database,
            "customer-portal-api-authorization-owner",
            "customer-portal-api-authorization-owner@example.com");
        var administrator = await SignUpAsync(
            database,
            "customer-portal-api-authorization-admin",
            "customer-portal-api-authorization-admin@example.com");
        var outsider = await SignUpAsync(
            database,
            "customer-portal-api-authorization-outsider",
            "customer-portal-api-authorization-outsider@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Customer Portal API authorization",
            SignupTime.AddDays(1));
        await AddOrganizationMemberAsync(
            database,
            organization,
            administrator.UserId,
            OrganizationRole.Admin);
        await ProjectSubscriptionAsync(
            database,
            organization.BillingAccountId,
            "cus_portal_api_authorization");
        var provider = new ApiCustomerPortalProvider();
        using var factory = CreateFactory(database, provider);
        using var administratorClient = factory.CreateApiClient(administrator.UserId);
        using var outsiderClient = factory.CreateApiClient(outsider.UserId);
        using var ownerClient = factory.CreateApiClient(owner.UserId);

        using var forbidden = await PostCustomerPortalAsync(
            administratorClient,
            organization.BillingAccountId);
        using var hidden = await PostCustomerPortalAsync(
            outsiderClient,
            organization.BillingAccountId);
        using var missing = await PostCustomerPortalAsync(
            ownerClient,
            Guid.CreateVersion7());
        using var noSubscription = await PostCustomerPortalAsync(
            ownerClient,
            owner.PersonalBillingAccountId);

        await AssertErrorAsync(
            forbidden,
            HttpStatusCode.Forbidden,
            "insufficient_permission");
        await AssertErrorAsync(
            hidden,
            HttpStatusCode.NotFound,
            "billing_account_not_found");
        await AssertErrorAsync(
            missing,
            HttpStatusCode.NotFound,
            "billing_account_not_found");
        await AssertErrorAsync(
            noSubscription,
            HttpStatusCode.Conflict,
            "subscription_not_found");
        Assert.That(provider.ExternalCustomerIds, Is.Empty);
    }

    [Test]
    public async Task ProviderOutageUsesAStableRetryableError()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(
            database,
            "customer-portal-api-outage",
            "customer-portal-api-outage@example.com");
        await ProjectSubscriptionAsync(
            database,
            signup.PersonalBillingAccountId,
            "cus_portal_api_outage");
        var provider = new ApiCustomerPortalProvider { Fail = true };
        using var factory = CreateFactory(database, provider);
        using var client = factory.CreateApiClient(signup.UserId);

        using var response = await PostCustomerPortalAsync(
            client,
            signup.PersonalBillingAccountId);

        await AssertErrorAsync(
            response,
            HttpStatusCode.ServiceUnavailable,
            "customer_portal_provider_unavailable");
    }

    [Test]
    public async Task InvalidBillingAccountIdentifierIsHiddenBeforeProviderCall()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(
            database,
            "customer-portal-api-invalid-id",
            "customer-portal-api-invalid-id@example.com");
        var provider = new ApiCustomerPortalProvider();
        using var factory = CreateFactory(database, provider);
        using var client = factory.CreateApiClient(signup.UserId);

        using var response = await PostCustomerPortalAsync(client, "not-a-uuid");

        await AssertErrorAsync(
            response,
            HttpStatusCode.NotFound,
            "billing_account_not_found");
        Assert.That(provider.ExternalCustomerIds, Is.Empty);
    }

    private static LicensingWebApplicationFactory CreateFactory(
        PostgresTestDatabase database,
        IBillingCustomerPortalProvider provider)
    {
        return new(
            database,
            SignupTime.AddDays(3),
            billingCustomerPortalProvider: provider);
    }

    private static Task<HttpResponseMessage> PostCustomerPortalAsync(
        HttpClient client,
        Guid billingAccountId)
    {
        return PostCustomerPortalAsync(client, billingAccountId.ToString());
    }

    private static async Task<HttpResponseMessage> PostCustomerPortalAsync(
        HttpClient client,
        string billingAccountId,
        object? browserOwnedBody = null)
    {
        var token = await AntiforgeryTestClient.GetTokenAsync(client);
        using var request = new HttpRequestMessage(HttpMethod.Post, Path(billingAccountId));
        if (browserOwnedBody is not null)
        {
            request.Content = JsonContent.Create(browserOwnedBody);
        }

        AntiforgeryTestClient.AddToken(request, token);
        return await client.SendAsync(request);
    }

    private static string Path(Guid billingAccountId)
    {
        return Path(billingAccountId.ToString());
    }

    private static string Path(string billingAccountId)
    {
        return $"/api/billing-accounts/{billingAccountId}/customer-portal-sessions";
    }

    private static async Task AssertErrorAsync(
        HttpResponseMessage response,
        HttpStatusCode expectedStatus,
        string expectedReasonCode)
    {
        Assert.That(response.StatusCode, Is.EqualTo(expectedStatus));
        using var body = JsonDocument.Parse(await response.Content.ReadAsStringAsync());
        Assert.That(
            body.RootElement.GetProperty("reasonCode").GetString(),
            Is.EqualTo(expectedReasonCode));
    }

    private static async Task ProjectSubscriptionAsync(
        PostgresTestDatabase database,
        Guid billingAccountId,
        string externalCustomerId)
    {
        await using var context = database.CreateContext();
        await using var serviceTest1 = ServiceTestBase<CommercialSubscriptionProjectionService>.ForDatabase(database, SignupTime);
        await serviceTest1.Service
            .ApplyAsync(
                billingAccountId,
                new CommercialSubscriptionProjection(
                    externalCustomerId,
                    $"sub_{externalCustomerId}",
                    "price_test_monthly",
                    CommercialSubscriptionStatus.Active,
                    seatQuantity: 1,
                    cancelAtPeriodEnd: false,
                    SignupTime,
                    SignupTime.AddMonths(1),
                    SignupTime.AddDays(2)),
                CancellationToken.None);
    }

    private sealed class ApiCustomerPortalProvider : IBillingCustomerPortalProvider
    {
        public List<string> ExternalCustomerIds { get; } = [];

        public bool Fail { get; init; }

        public Task<BillingCustomerPortalProviderSession> CreateSessionAsync(
            string externalCustomerId,
            CancellationToken cancellationToken)
        {
            cancellationToken.ThrowIfCancellationRequested();
            ExternalCustomerIds.Add(externalCustomerId);
            if (Fail)
            {
                throw new BillingCustomerPortalProviderUnavailableException(
                    "Simulated Stripe outage.");
            }

            return Task.FromResult(
                new BillingCustomerPortalProviderSession(
                    new Uri("https://billing.stripe.test/api-session")));
        }
    }
}
