using System.Net;
using System.Net.Http.Json;
using System.Text.Json;
using Microsoft.EntityFrameworkCore;
using Styrhous.Licensing.Application.Billing;
using Styrhous.Licensing.Domain.Billing;
using Styrhous.Licensing.Domain.Organizations;
using Styrhous.Licensing.Tests.Persistence;
using static Styrhous.Licensing.Tests.Persistence.LicensingPersistenceScenario;

namespace Styrhous.Licensing.Tests.Api;

[TestFixture]
[Parallelizable(ParallelScope.All)]
public sealed class BillingCheckoutApiTests
{
    private static readonly string[] SuccessProperties =
        ["reasonCode", "billingOperationId", "redirectUrl"];

    private static readonly DateTimeOffset CheckoutTime = SignupTime.AddDays(3);

    [Test]
    public async Task PersonalOwnerCreatesFrontendReadyCheckoutSession()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(
            database,
            "checkout-api-owner",
            "checkout-api-owner@example.com");
        var provider = new ApiCheckoutProvider();
        using var factory = CreateFactory(database, provider);
        using var client = factory.CreateApiClient(signup.UserId);

        using var response = await PostCheckoutAsync(
            client,
            signup.PersonalBillingAccountId,
            "monthly",
            seatQuantity: 1);
        using var body = JsonDocument.Parse(await response.Content.ReadAsStringAsync());
        var operationId = body.RootElement.GetProperty("billingOperationId").GetGuid();

        Assert.Multiple(() =>
        {
            Assert.That(response.StatusCode, Is.EqualTo(HttpStatusCode.OK));
            Assert.That(response.Headers.CacheControl?.NoStore, Is.True);
            Assert.That(
                body.RootElement.EnumerateObject().Select(property => property.Name),
                Is.EqualTo(SuccessProperties));
            Assert.That(
                body.RootElement.GetProperty("reasonCode").GetString(),
                Is.EqualTo("checkout_session_created"));
            Assert.That(operationId.Version, Is.EqualTo(7));
            Assert.That(
                body.RootElement.GetProperty("redirectUrl").GetString(),
                Is.EqualTo("https://checkout.stripe.test/api-session"));
            Assert.That(provider.Requests.Single().BillingOperationId, Is.EqualTo(operationId));
            Assert.That(provider.Requests.Single().Cadence, Is.EqualTo(BillingCadence.Monthly));
        });

        await using var context = database.CreateContext();
        Assert.That(
            (await context.BillingOperations.SingleAsync()).Status,
            Is.EqualTo(BillingOperationStatus.ProviderSessionCreated));
    }

    [Test]
    public async Task OrganizationOwnerCanPurchaseAnyPositiveQuantityAboveCurrentNeed()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(
            database,
            "checkout-api-org-owner",
            "checkout-api-org-owner@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Checkout API organization",
            SignupTime.AddDays(1));
        var provider = new ApiCheckoutProvider();
        using var factory = CreateFactory(database, provider);
        using var client = factory.CreateApiClient(owner.UserId);

        using var response = await PostCheckoutAsync(
            client,
            organization.BillingAccountId,
            "annual",
            int.MaxValue);

        Assert.Multiple(() =>
        {
            Assert.That(response.StatusCode, Is.EqualTo(HttpStatusCode.OK));
            Assert.That(provider.Requests.Single().SeatQuantity, Is.EqualTo(int.MaxValue));
            Assert.That(provider.Requests.Single().Cadence, Is.EqualTo(BillingCadence.Annual));
        });
    }

    [Test]
    public async Task AuthenticationAndAntiforgeryAreRequiredBeforeCheckout()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(
            database,
            "checkout-api-security",
            "checkout-api-security@example.com");
        var provider = new ApiCheckoutProvider();
        using var factory = CreateFactory(database, provider);
        using var anonymous = factory.CreateApiClient();
        using var stale = factory.CreateApiClient(Guid.CreateVersion7());
        using var owner = factory.CreateApiClient(signup.UserId);

        using var anonymousResponse = await anonymous.PostAsJsonAsync(
            Path(signup.PersonalBillingAccountId),
            new { cadence = "monthly", seatQuantity = 1 });
        using var staleResponse = await PostCheckoutAsync(
            stale,
            signup.PersonalBillingAccountId,
            "monthly",
            seatQuantity: 1);
        using var noTokenResponse = await owner.PostAsJsonAsync(
            Path(signup.PersonalBillingAccountId),
            new { cadence = "monthly", seatQuantity = 1 });

        Assert.Multiple(() =>
        {
            Assert.That(anonymousResponse.StatusCode, Is.EqualTo(HttpStatusCode.Unauthorized));
            Assert.That(staleResponse.StatusCode, Is.EqualTo(HttpStatusCode.Unauthorized));
            Assert.That(noTokenResponse.StatusCode, Is.EqualTo(HttpStatusCode.BadRequest));
            Assert.That(provider.Requests, Is.Empty);
        });
    }

    [TestCase("not-a-uuid", "monthly", 1, null, "billing_account_not_found", HttpStatusCode.NotFound)]
    [TestCase("valid", "weekly", 1, null, null, HttpStatusCode.BadRequest)]
    [TestCase("valid", "monthly", 0, null, null, HttpStatusCode.BadRequest)]
    [TestCase("valid", "monthly", 1, "not-a-uuid", null, HttpStatusCode.BadRequest)]
    public async Task InvalidCheckoutInputIsRejectedBeforeProviderCall(
        string accountId,
        string cadence,
        int seatQuantity,
        string? operationId,
        string? expectedReasonCode,
        HttpStatusCode expectedStatus)
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(
            database,
            $"checkout-api-input-{cadence}-{seatQuantity}-{operationId}",
            $"checkout-api-input-{Guid.CreateVersion7():N}@example.com");
        var provider = new ApiCheckoutProvider();
        using var factory = CreateFactory(database, provider);
        using var client = factory.CreateApiClient(signup.UserId);

        using var response = await PostCheckoutAsync(
            client,
            accountId == "valid"
                ? signup.PersonalBillingAccountId.ToString()
                : accountId,
            cadence,
            seatQuantity,
            operationId);
        var text = await response.Content.ReadAsStringAsync();

        Assert.Multiple(() =>
        {
            Assert.That(response.StatusCode, Is.EqualTo(expectedStatus));
            Assert.That(provider.Requests, Is.Empty);
            if (expectedReasonCode is not null)
            {
                using var body = JsonDocument.Parse(text);
                Assert.That(
                    body.RootElement.GetProperty("reasonCode").GetString(),
                    Is.EqualTo(expectedReasonCode));
            }
        });
    }

    [Test]
    public async Task MissingAccountAndInvalidPersonalQuantityUseStableReasonCodes()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(
            database,
            "checkout-api-stable-rejections",
            "checkout-api-stable-rejections@example.com");
        var provider = new ApiCheckoutProvider();
        using var factory = CreateFactory(database, provider);
        using var client = factory.CreateApiClient(signup.UserId);

        using var missing = await PostCheckoutAsync(
            client,
            Guid.CreateVersion7(),
            "monthly",
            1);
        using var invalidPersonalQuantity = await PostCheckoutAsync(
            client,
            signup.PersonalBillingAccountId,
            "monthly",
            2);
        using var missingBody = JsonDocument.Parse(
            await missing.Content.ReadAsStringAsync());
        using var quantityBody = JsonDocument.Parse(
            await invalidPersonalQuantity.Content.ReadAsStringAsync());

        Assert.Multiple(() =>
        {
            Assert.That(missing.StatusCode, Is.EqualTo(HttpStatusCode.NotFound));
            Assert.That(
                missingBody.RootElement.GetProperty("reasonCode").GetString(),
                Is.EqualTo("billing_account_not_found"));
            Assert.That(
                invalidPersonalQuantity.StatusCode,
                Is.EqualTo(HttpStatusCode.Conflict));
            Assert.That(
                quantityBody.RootElement.GetProperty("reasonCode").GetString(),
                Is.EqualTo("personal_seat_quantity_invalid"));
            Assert.That(
                quantityBody.RootElement.GetProperty("requiredSeatQuantity").GetInt32(),
                Is.EqualTo(1));
            Assert.That(provider.Requests, Is.Empty);
        });
    }

    [Test]
    public async Task MissingOrMismatchedRetryUsesStableOperationNotFoundResponse()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(
            database,
            "checkout-api-retry-not-found",
            "checkout-api-retry-not-found@example.com");
        var provider = new ApiCheckoutProvider { FailNextRequest = true };
        using var factory = CreateFactory(database, provider);
        using var client = factory.CreateApiClient(signup.UserId);

        using var unavailable = await PostCheckoutAsync(
            client,
            signup.PersonalBillingAccountId,
            "monthly",
            1);
        using var unavailableBody = JsonDocument.Parse(
            await unavailable.Content.ReadAsStringAsync());
        var operationId = unavailableBody.RootElement
            .GetProperty("billingOperationId")
            .GetGuid();
        using var missing = await PostCheckoutAsync(
            client,
            signup.PersonalBillingAccountId,
            "monthly",
            1,
            Guid.CreateVersion7().ToString());
        using var mismatched = await PostCheckoutAsync(
            client,
            signup.PersonalBillingAccountId,
            "annual",
            1,
            operationId.ToString());

        foreach (var response in new[] { missing, mismatched })
        {
            using var body = JsonDocument.Parse(await response.Content.ReadAsStringAsync());
            Assert.Multiple(() =>
            {
                Assert.That(response.StatusCode, Is.EqualTo(HttpStatusCode.NotFound));
                Assert.That(
                    body.RootElement.GetProperty("reasonCode").GetString(),
                    Is.EqualTo("billing_operation_not_found"));
            });
        }

        Assert.That(provider.Requests, Has.Count.EqualTo(1));
    }

    [TestCase(OrganizationRole.Admin)]
    [TestCase(OrganizationRole.Member)]
    public async Task NonOwnerOrganizationMemberReceivesStablePermissionError(
        OrganizationRole role)
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(
            database,
            $"checkout-api-permission-owner-{role}",
            $"checkout-api-permission-owner-{role}@example.com");
        var actor = await SignUpAsync(
            database,
            $"checkout-api-permission-actor-{role}",
            $"checkout-api-permission-actor-{role}@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            $"Checkout API permission {role}",
            SignupTime.AddDays(1));
        await AddOrganizationMemberAsync(database, organization, actor.UserId, role);
        var provider = new ApiCheckoutProvider();
        using var factory = CreateFactory(database, provider);
        using var client = factory.CreateApiClient(actor.UserId);

        using var response = await PostCheckoutAsync(
            client,
            organization.BillingAccountId,
            "monthly",
            seatQuantity: 2);
        using var body = JsonDocument.Parse(await response.Content.ReadAsStringAsync());

        Assert.Multiple(() =>
        {
            Assert.That(response.StatusCode, Is.EqualTo(HttpStatusCode.Forbidden));
            Assert.That(
                body.RootElement.GetProperty("reasonCode").GetString(),
                Is.EqualTo("insufficient_permission"));
            Assert.That(provider.Requests, Is.Empty);
        });
    }

    [Test]
    public async Task CapacityAndExistingSubscriptionConflictsAreFrontendReady()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(
            database,
            "checkout-api-conflict-owner",
            "checkout-api-conflict-owner@example.com");
        var member = await SignUpAsync(
            database,
            "checkout-api-conflict-member",
            "checkout-api-conflict-member@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Checkout API conflicts",
            SignupTime.AddDays(1));
        await AddOrganizationMemberAsync(
            database,
            organization,
            member.UserId,
            OrganizationRole.Member);
        await using (var setup = database.CreateContext())
        {
            setup.CommercialSubscriptions.Add(
                CommercialSubscription.Create(
                    owner.PersonalBillingAccountId,
                    new CommercialSubscriptionProjection(
                        "cus_api_existing",
                        "sub_api_existing",
                        "price_api_existing",
                        CommercialSubscriptionStatus.Active,
                        seatQuantity: 1,
                        cancelAtPeriodEnd: false,
                        CheckoutTime.AddDays(-1),
                        CheckoutTime.AddDays(29),
                        CheckoutTime)));
            await setup.SaveChangesAsync();
        }

        var provider = new ApiCheckoutProvider();
        using var factory = CreateFactory(database, provider);
        using var client = factory.CreateApiClient(owner.UserId);
        using var tooSmall = await PostCheckoutAsync(
            client,
            organization.BillingAccountId,
            "monthly",
            seatQuantity: 1);
        using var existing = await PostCheckoutAsync(
            client,
            owner.PersonalBillingAccountId,
            "monthly",
            seatQuantity: 1);
        using var tooSmallBody = JsonDocument.Parse(
            await tooSmall.Content.ReadAsStringAsync());
        using var existingBody = JsonDocument.Parse(
            await existing.Content.ReadAsStringAsync());

        Assert.Multiple(() =>
        {
            Assert.That(tooSmall.StatusCode, Is.EqualTo(HttpStatusCode.Conflict));
            Assert.That(
                tooSmallBody.RootElement.GetProperty("reasonCode").GetString(),
                Is.EqualTo("seat_quantity_too_small"));
            Assert.That(
                tooSmallBody.RootElement.GetProperty("requiredSeatQuantity").GetInt32(),
                Is.EqualTo(2));
            Assert.That(existing.StatusCode, Is.EqualTo(HttpStatusCode.Conflict));
            Assert.That(
                existingBody.RootElement.GetProperty("reasonCode").GetString(),
                Is.EqualTo("subscription_already_exists"));
            Assert.That(provider.Requests, Is.Empty);
        });
    }

    [Test]
    public async Task ProviderFailureReturnsOperationThatThePortalCanRetry()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(
            database,
            "checkout-api-retry",
            "checkout-api-retry@example.com");
        var provider = new ApiCheckoutProvider { FailNextRequest = true };
        using var factory = CreateFactory(database, provider);
        using var client = factory.CreateApiClient(signup.UserId);

        using var unavailable = await PostCheckoutAsync(
            client,
            signup.PersonalBillingAccountId,
            "annual",
            seatQuantity: 1);
        using var unavailableBody = JsonDocument.Parse(
            await unavailable.Content.ReadAsStringAsync());
        var operationId = unavailableBody.RootElement
            .GetProperty("billingOperationId")
            .GetGuid();
        using var retried = await PostCheckoutAsync(
            client,
            signup.PersonalBillingAccountId,
            "annual",
            seatQuantity: 1,
            operationId.ToString());
        using var retriedBody = JsonDocument.Parse(
            await retried.Content.ReadAsStringAsync());

        Assert.Multiple(() =>
        {
            Assert.That(unavailable.StatusCode, Is.EqualTo(HttpStatusCode.ServiceUnavailable));
            Assert.That(
                unavailableBody.RootElement.GetProperty("reasonCode").GetString(),
                Is.EqualTo("checkout_provider_unavailable"));
            Assert.That(operationId.Version, Is.EqualTo(7));
            Assert.That(retried.StatusCode, Is.EqualTo(HttpStatusCode.OK));
            Assert.That(
                retriedBody.RootElement.GetProperty("billingOperationId").GetGuid(),
                Is.EqualTo(operationId));
            Assert.That(
                provider.Requests.Select(request => request.BillingOperationId),
                Is.All.EqualTo(operationId));
        });
    }

    [Test]
    public async Task ChangedDraftReturnsTheRecoverableLiveCheckoutAttempt()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(
            database,
            "checkout-api-live-operation",
            "checkout-api-live-operation@example.com");
        var provider = new ApiCheckoutProvider { FailNextRequest = true };
        using var factory = CreateFactory(database, provider);
        using var client = factory.CreateApiClient(signup.UserId);

        using var unavailable = await PostCheckoutAsync(
            client,
            signup.PersonalBillingAccountId,
            "monthly",
            1);
        using var unavailableBody = JsonDocument.Parse(
            await unavailable.Content.ReadAsStringAsync());
        var operationId = unavailableBody.RootElement
            .GetProperty("billingOperationId")
            .GetGuid();
        using var changed = await PostCheckoutAsync(
            client,
            signup.PersonalBillingAccountId,
            "annual",
            1);
        using var changedBody = JsonDocument.Parse(
            await changed.Content.ReadAsStringAsync());

        Assert.Multiple(() =>
        {
            Assert.That(changed.StatusCode, Is.EqualTo(HttpStatusCode.Conflict));
            Assert.That(
                changedBody.RootElement.GetProperty("reasonCode").GetString(),
                Is.EqualTo("checkout_operation_in_progress"));
            Assert.That(
                changedBody.RootElement.GetProperty("billingOperationId").GetGuid(),
                Is.EqualTo(operationId));
            Assert.That(
                changedBody.RootElement.GetProperty("cadence").GetString(),
                Is.EqualTo("monthly"));
            Assert.That(changedBody.RootElement.GetProperty("seatQuantity").GetInt32(),
                Is.EqualTo(1));
            Assert.That(
                changedBody.RootElement.GetProperty("expiresAt").GetDateTimeOffset(),
                Is.EqualTo(CheckoutTime.Add(BillingOperation.CheckoutLifetime)));
            Assert.That(provider.Requests, Has.Count.EqualTo(1));
        });
    }

    [Test]
    public async Task CapacityGrowthBlocksAReplacementUntilTheLiveCheckoutExpires()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(
            database,
            "checkout-api-capacity-growth",
            "checkout-api-capacity-growth@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Checkout capacity growth",
            SignupTime.AddDays(1));
        var provider = new ApiCheckoutProvider { FailNextRequest = true };
        using var factory = CreateFactory(database, provider);
        using var client = factory.CreateApiClient(owner.UserId);

        using var unavailable = await PostCheckoutAsync(
            client,
            organization.BillingAccountId,
            "monthly",
            1);
        using var unavailableBody = JsonDocument.Parse(
            await unavailable.Content.ReadAsStringAsync());
        var operationId = unavailableBody.RootElement
            .GetProperty("billingOperationId")
            .GetGuid();
        await CreateInvitationAsync(
            database,
            owner.UserId,
            organization.OrganizationId,
            "checkout-api-capacity-growth-invite@example.com",
            CheckoutTime.AddMinutes(-1));

        using var retry = await PostCheckoutAsync(
            client,
            organization.BillingAccountId,
            "monthly",
            1,
            operationId.ToString());
        using var retryBody = JsonDocument.Parse(await retry.Content.ReadAsStringAsync());

        Assert.Multiple(() =>
        {
            Assert.That(retry.StatusCode, Is.EqualTo(HttpStatusCode.Conflict));
            Assert.That(
                retryBody.RootElement.GetProperty("reasonCode").GetString(),
                Is.EqualTo("checkout_operation_capacity_changed"));
            Assert.That(
                retryBody.RootElement.GetProperty("billingOperationId").GetGuid(),
                Is.EqualTo(operationId));
            Assert.That(retryBody.RootElement.GetProperty("seatQuantity").GetInt32(),
                Is.EqualTo(1));
            Assert.That(
                retryBody.RootElement.GetProperty("requiredSeatQuantity").GetInt32(),
                Is.EqualTo(2));
            Assert.That(
                retryBody.RootElement.GetProperty("expiresAt").GetDateTimeOffset(),
                Is.EqualTo(CheckoutTime.Add(BillingOperation.CheckoutLifetime)));
            Assert.That(provider.Requests, Has.Count.EqualTo(1));
        });
    }

    private static LicensingWebApplicationFactory CreateFactory(
        PostgresTestDatabase database,
        IBillingCheckoutProvider provider)
    {
        return new(
            database,
            CheckoutTime,
            billingCheckoutProvider: provider);
    }

    private static async Task<HttpResponseMessage> PostCheckoutAsync(
        HttpClient client,
        Guid billingAccountId,
        string cadence,
        int seatQuantity,
        string? billingOperationId = null)
    {
        return await PostCheckoutAsync(
            client,
            billingAccountId.ToString(),
            cadence,
            seatQuantity,
            billingOperationId);
    }

    private static async Task<HttpResponseMessage> PostCheckoutAsync(
        HttpClient client,
        string billingAccountId,
        string cadence,
        int seatQuantity,
        string? billingOperationId = null)
    {
        var token = await AntiforgeryTestClient.GetTokenAsync(client);
        using var request = new HttpRequestMessage(HttpMethod.Post, Path(billingAccountId))
        {
            Content = JsonContent.Create(
                new { cadence, seatQuantity, billingOperationId }),
        };
        AntiforgeryTestClient.AddToken(request, token);
        return await client.SendAsync(request);
    }

    private static string Path(Guid billingAccountId)
    {
        return Path(billingAccountId.ToString());
    }

    private static string Path(string billingAccountId)
    {
        return $"/api/billing-accounts/{billingAccountId}/checkout-sessions";
    }

    private sealed class ApiCheckoutProvider : IBillingCheckoutProvider
    {
        public List<BillingCheckoutProviderRequest> Requests { get; } = [];

        public bool FailNextRequest { get; set; }

        public Task<BillingCheckoutProviderSession> CreateSessionAsync(
            BillingCheckoutProviderRequest request,
            CancellationToken cancellationToken)
        {
            cancellationToken.ThrowIfCancellationRequested();
            Requests.Add(request);
            if (FailNextRequest)
            {
                FailNextRequest = false;
                throw new BillingCheckoutProviderUnavailableException(
                    "Simulated Stripe outage.");
            }

            return Task.FromResult(
                new BillingCheckoutProviderSession(
                    "cs_api_checkout",
                    new Uri("https://checkout.stripe.test/api-session")));
        }

        public Task<BillingCheckoutProviderSessionState> GetSessionStateAsync(
            string externalSessionId,
            CancellationToken cancellationToken)
        {
            return Task.FromResult(BillingCheckoutProviderSessionState.Open);
        }
    }
}
