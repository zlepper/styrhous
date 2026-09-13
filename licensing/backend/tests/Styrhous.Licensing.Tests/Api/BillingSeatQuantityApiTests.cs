using System.Net;
using System.Net.Http.Json;
using System.Text.Json;
using Microsoft.EntityFrameworkCore;
using Styrhous.Licensing.Application.Billing;
using Styrhous.Licensing.Domain.Billing;
using Styrhous.Licensing.Domain.Organizations;
using Styrhous.Licensing.Persistence;
using Styrhous.Licensing.Tests.Persistence;
using static Styrhous.Licensing.Tests.Persistence.LicensingPersistenceScenario;

namespace Styrhous.Licensing.Tests.Api;

[TestFixture]
[Parallelizable(ParallelScope.All)]
public sealed class BillingSeatQuantityApiTests
{
    private static readonly DateTimeOffset ChangeTime = SignupTime.AddDays(4);
    private static readonly string[] SuccessProperties =
        ["reasonCode", "billingOperationId", "seatQuantity"];

    [Test]
    public async Task OrganizationOwnerChangesUnboundedQuantityAndReceivesProviderSafeResponse()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(
            database,
            "seat-quantity-api-owner",
            "seat-quantity-api-owner@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Seat quantity API",
            SignupTime.AddDays(1));
        await ProjectSubscriptionAsync(database, organization.BillingAccountId, 2);
        var provider = new ApiSeatQuantityProvider();
        using var factory = CreateFactory(database, provider);
        using var client = factory.CreateApiClient(owner.UserId);

        using var response = await PatchAsync(
            client,
            organization.BillingAccountId,
            int.MaxValue);
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
                Is.EqualTo("seat_quantity_changed"));
            Assert.That(
                body.RootElement.GetProperty("seatQuantity").GetInt32(),
                Is.EqualTo(int.MaxValue));
            Assert.That(provider.Requests.Single().PreviousSeatQuantity, Is.EqualTo(2));
            Assert.That(provider.Requests.Single().SeatQuantity, Is.EqualTo(int.MaxValue));
            Assert.That(provider.Mutations, Has.Count.EqualTo(1));
        });

        await using var verification = database.CreateContext();
        Assert.Multiple(() =>
        {
            Assert.That(
                verification.CommercialSubscriptions.Single().SeatQuantity,
                Is.EqualTo(int.MaxValue));
            Assert.That(
                verification.BillingOperations.Single().Status,
                Is.EqualTo(BillingOperationStatus.Completed));
        });
    }

    [Test]
    public async Task ProviderOutageReturnsRetryIdentityAndSameOperationCompletes()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(
            database,
            "seat-quantity-api-retry-owner",
            "seat-quantity-api-retry-owner@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Seat quantity API retry",
            SignupTime.AddDays(1));
        await ProjectSubscriptionAsync(database, organization.BillingAccountId, 2);
        var provider = new ApiSeatQuantityProvider { TransientFailuresRemaining = 1 };
        using var factory = CreateFactory(database, provider);
        using var client = factory.CreateApiClient(owner.UserId);

        using var failed = await PatchAsync(client, organization.BillingAccountId, 5);
        using var failedBody = JsonDocument.Parse(await failed.Content.ReadAsStringAsync());
        var operationId = failedBody.RootElement
            .GetProperty("billingOperationId")
            .GetGuid();
        using var retried = await PatchAsync(
            client,
            organization.BillingAccountId,
            5,
            operationId);
        using var retryBody = JsonDocument.Parse(await retried.Content.ReadAsStringAsync());

        Assert.Multiple(() =>
        {
            Assert.That(failed.StatusCode, Is.EqualTo(HttpStatusCode.ServiceUnavailable));
            Assert.That(
                failedBody.RootElement.GetProperty("reasonCode").GetString(),
                Is.EqualTo("seat_quantity_provider_unavailable"));
            Assert.That(retried.StatusCode, Is.EqualTo(HttpStatusCode.OK));
            Assert.That(
                retryBody.RootElement.GetProperty("billingOperationId").GetGuid(),
                Is.EqualTo(operationId));
            Assert.That(provider.Requests.Select(request => request.BillingOperationId),
                Is.All.EqualTo(operationId));
        });
    }

    [Test]
    public async Task ChangedRequestReturnsTheLiveOperationNeededForRecovery()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(
            database,
            "seat-quantity-api-in-progress-owner",
            "seat-quantity-api-in-progress-owner@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Seat quantity API in progress",
            SignupTime.AddDays(1));
        await ProjectSubscriptionAsync(database, organization.BillingAccountId, 2);
        var provider = new ApiSeatQuantityProvider { TransientFailuresRemaining = 1 };
        using var factory = CreateFactory(database, provider);
        using var client = factory.CreateApiClient(owner.UserId);

        using var failed = await PatchAsync(client, organization.BillingAccountId, 5);
        using var changed = await PatchAsync(client, organization.BillingAccountId, 6);
        using var body = JsonDocument.Parse(await changed.Content.ReadAsStringAsync());

        Assert.Multiple(() =>
        {
            Assert.That(failed.StatusCode, Is.EqualTo(HttpStatusCode.ServiceUnavailable));
            Assert.That(changed.StatusCode, Is.EqualTo(HttpStatusCode.Conflict));
            Assert.That(
                body.RootElement.GetProperty("reasonCode").GetString(),
                Is.EqualTo("seat_quantity_operation_in_progress"));
            Assert.That(body.RootElement.GetProperty("previousSeatQuantity").GetInt32(), Is.EqualTo(2));
            Assert.That(body.RootElement.GetProperty("seatQuantity").GetInt32(), Is.EqualTo(5));
            Assert.That(provider.Requests, Has.Count.EqualTo(1));
        });
    }

    [Test]
    public async Task ProviderSupersessionProjectsCurrentQuantityBeforeReturningConflict()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(
            database,
            "seat-quantity-api-superseded-owner",
            "seat-quantity-api-superseded-owner@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Seat quantity API superseded",
            SignupTime.AddDays(1));
        await ProjectSubscriptionAsync(database, organization.BillingAccountId, 2);
        var provider = new ApiSeatQuantityProvider { SupersededQuantity = 4 };
        using var factory = CreateFactory(database, provider);
        using var client = factory.CreateApiClient(owner.UserId);

        using var response = await PatchAsync(client, organization.BillingAccountId, 5);
        using var body = JsonDocument.Parse(await response.Content.ReadAsStringAsync());

        Assert.Multiple(() =>
        {
            Assert.That(response.StatusCode, Is.EqualTo(HttpStatusCode.Conflict));
            Assert.That(
                body.RootElement.GetProperty("reasonCode").GetString(),
                Is.EqualTo("subscription_quantity_changed"));
            Assert.That(body.RootElement.GetProperty("seatQuantity").GetInt32(), Is.EqualTo(4));
        });
        await using var verification = database.CreateContext();
        Assert.Multiple(() =>
        {
            Assert.That(verification.CommercialSubscriptions.Single().SeatQuantity, Is.EqualTo(4));
            Assert.That(
                verification.BillingOperations.Single().Status,
                Is.EqualTo(BillingOperationStatus.Failed));
        });
    }

    [Test]
    public async Task AgedProviderReplayRequiresReconciliationWithoutAnotherMutation()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(
            database,
            "seat-quantity-api-reconciliation",
            "seat-quantity-api-reconciliation@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Expired seat replay API",
            SignupTime.AddDays(1));
        await ProjectSubscriptionAsync(database, organization.BillingAccountId, 2);
        Guid operationId;
        await using (var context = database.CreateContext())
        {
            var store = new PostgresBillingSeatQuantityStore(context.CreateContextFactory());
            var prepared = await store.PrepareAsync(
                owner.UserId,
                organization.BillingAccountId,
                5,
                retryOperationId: null,
                ChangeTime,
                CancellationToken.None);
            operationId = prepared.Operation!.Id;
            _ = await store.ApplyObservationAndPrepareProviderMutationAsync(
                operationId,
                new AuthoritativeCommercialSubscription(
                    organization.BillingAccountId,
                        Projection(
                            organization.BillingAccountId,
                            seatQuantity: 2,
                            ChangeTime.AddMinutes(1))),
                CancellationToken.None);
        }

        var provider = new ApiSeatQuantityProvider
        {
            ProviderObservedAt = ChangeTime.AddHours(23).AddMinutes(1),
        };
        using var factory = CreateFactory(database, provider);
        using var client = factory.CreateApiClient(owner.UserId);
        using var response = await PatchAsync(
            client,
            organization.BillingAccountId,
            5,
            operationId);
        using var body = JsonDocument.Parse(await response.Content.ReadAsStringAsync());

        Assert.Multiple(() =>
        {
            Assert.That(response.StatusCode, Is.EqualTo(HttpStatusCode.Conflict));
            Assert.That(
                body.RootElement.GetProperty("reasonCode").GetString(),
                Is.EqualTo("seat_quantity_reconciliation_required"));
            Assert.That(
                body.RootElement.GetProperty("billingOperationId").GetGuid(),
                Is.EqualTo(operationId));
            Assert.That(
                body.RootElement.GetProperty("previousSeatQuantity").GetInt32(),
                Is.EqualTo(2));
            Assert.That(body.RootElement.GetProperty("seatQuantity").GetInt32(), Is.EqualTo(5));
            Assert.That(provider.Requests, Has.Count.EqualTo(1));
            Assert.That(provider.Mutations, Is.Empty);
        });

        await using var verification = database.CreateContext();
        Assert.That(
            verification.BillingOperations.Single().Status,
            Is.EqualTo(BillingOperationStatus.Pending));
    }

    [Test]
    public async Task CapacityAuthorizationAndSubscriptionErrorsAreStable()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(
            database,
            "seat-quantity-api-auth-owner",
            "seat-quantity-api-auth-owner@example.com");
        var administrator = await SignUpAsync(
            database,
            "seat-quantity-api-auth-admin",
            "seat-quantity-api-auth-admin@example.com");
        var outsider = await SignUpAsync(
            database,
            "seat-quantity-api-auth-outsider",
            "seat-quantity-api-auth-outsider@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Seat quantity API authorization",
            SignupTime.AddDays(1));
        await AddOrganizationMemberAsync(
            database,
            organization,
            administrator.UserId,
            OrganizationRole.Admin);
        await ProjectSubscriptionAsync(database, organization.BillingAccountId, 4);
        var provider = new ApiSeatQuantityProvider();
        using var factory = CreateFactory(database, provider);
        using var ownerClient = factory.CreateApiClient(owner.UserId);
        using var administratorClient = factory.CreateApiClient(administrator.UserId);
        using var outsiderClient = factory.CreateApiClient(outsider.UserId);

        using var tooSmall = await PatchAsync(ownerClient, organization.BillingAccountId, 1);
        using var forbidden = await PatchAsync(administratorClient, organization.BillingAccountId, 5);
        using var hidden = await PatchAsync(outsiderClient, organization.BillingAccountId, 5);
        using var missingSubscription = await PatchAsync(ownerClient, owner.PersonalBillingAccountId, 1);

        await AssertErrorAsync(tooSmall, HttpStatusCode.Conflict, "seat_quantity_too_small");
        await AssertErrorAsync(forbidden, HttpStatusCode.Forbidden, "insufficient_permission");
        await AssertErrorAsync(hidden, HttpStatusCode.NotFound, "billing_account_not_found");
        await AssertErrorAsync(missingSubscription, HttpStatusCode.Conflict, "subscription_not_found");
        Assert.That(provider.Requests, Is.Empty);
    }

    [Test]
    public async Task AuthenticationAntiforgeryAndRequestValidationPrecedeProviderCall()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(
            database,
            "seat-quantity-api-security",
            "seat-quantity-api-security@example.com");
        await ProjectSubscriptionAsync(database, owner.PersonalBillingAccountId, 1);
        var provider = new ApiSeatQuantityProvider();
        using var factory = CreateFactory(database, provider);
        using var anonymous = factory.CreateApiClient();
        using var authenticated = factory.CreateApiClient(owner.UserId);

        using var anonymousResponse = await anonymous.PatchAsJsonAsync(
            Path(owner.PersonalBillingAccountId),
            new { seatQuantity = 1 });
        using var noToken = await authenticated.PatchAsJsonAsync(
            Path(owner.PersonalBillingAccountId),
            new { seatQuantity = 1 });
        using var invalidQuantity = await PatchAsync(
            authenticated,
            owner.PersonalBillingAccountId,
            0);
        using var invalidOperation = await PatchRawAsync(
            authenticated,
            owner.PersonalBillingAccountId,
            new { seatQuantity = 1, billingOperationId = "not-a-uuid" });

        Assert.Multiple(() =>
        {
            Assert.That(anonymousResponse.StatusCode, Is.EqualTo(HttpStatusCode.Unauthorized));
            Assert.That(noToken.StatusCode, Is.EqualTo(HttpStatusCode.BadRequest));
            Assert.That(invalidQuantity.StatusCode, Is.EqualTo(HttpStatusCode.BadRequest));
            Assert.That(invalidOperation.StatusCode, Is.EqualTo(HttpStatusCode.BadRequest));
            Assert.That(provider.Requests, Is.Empty);
        });
    }

    private static LicensingWebApplicationFactory CreateFactory(
        PostgresTestDatabase database,
        IBillingSeatQuantityProvider provider)
    {
        return new(
            database,
            ChangeTime,
            billingSeatQuantityProvider: provider);
    }

    private static Task<HttpResponseMessage> PatchAsync(
        HttpClient client,
        Guid billingAccountId,
        int seatQuantity,
        Guid? billingOperationId = null)
    {
        return PatchRawAsync(
            client,
            billingAccountId,
            new
            {
                seatQuantity,
                billingOperationId = billingOperationId?.ToString(),
            });
    }

    private static async Task<HttpResponseMessage> PatchRawAsync(
        HttpClient client,
        Guid billingAccountId,
        object body)
    {
        var token = await AntiforgeryTestClient.GetTokenAsync(client);
        using var request = new HttpRequestMessage(
            HttpMethod.Patch,
            Path(billingAccountId))
        {
            Content = JsonContent.Create(body),
        };
        AntiforgeryTestClient.AddToken(request, token);
        return await client.SendAsync(request);
    }

    private static string Path(Guid billingAccountId)
    {
        return $"/api/billing-accounts/{billingAccountId}/seat-quantity";
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
        int seatQuantity)
    {
        await using var context = database.CreateContext();
        await using var serviceTest1 = ServiceTestBase<CommercialSubscriptionProjectionService>.ForDatabase(database, SignupTime);
        await serviceTest1.Service
            .ApplyAsync(
                billingAccountId,
                Projection(billingAccountId, seatQuantity, ChangeTime.AddHours(-1)),
                CancellationToken.None);
    }

    private static CommercialSubscriptionProjection Projection(
        Guid billingAccountId,
        int seatQuantity,
        DateTimeOffset projectedAt)
    {
        return new(
            $"cus_{billingAccountId:N}",
            $"sub_{billingAccountId:N}",
            "price_test_monthly",
            CommercialSubscriptionStatus.Active,
            seatQuantity,
            cancelAtPeriodEnd: false,
            SignupTime,
            SignupTime.AddYears(1),
            projectedAt);
    }

    private sealed class ApiSeatQuantityProvider : IBillingSeatQuantityProvider
    {
        public List<BillingSeatQuantityProviderRequest> Requests { get; } = [];

        public List<BillingSeatQuantityProviderRequest> Mutations { get; } = [];

        public int TransientFailuresRemaining { get; set; }

        public int? SupersededQuantity { get; init; }

        public DateTimeOffset ProviderObservedAt { get; init; } =
            ChangeTime.AddMinutes(1);

        public Task<BillingSeatQuantityProviderResult> ObserveAsync(
            BillingSeatQuantityProviderRequest request,
            CancellationToken cancellationToken)
        {
            cancellationToken.ThrowIfCancellationRequested();
            Requests.Add(request);
            if (TransientFailuresRemaining > 0)
            {
                TransientFailuresRemaining--;
                throw new BillingSeatQuantityProviderUnavailableException(
                    "Simulated Stripe outage.",
                    new HttpRequestException());
            }

            var quantity = SupersededQuantity ?? request.PreviousSeatQuantity;
            return Task.FromResult(
                new BillingSeatQuantityProviderResult(
                    SupersededQuantity is null
                        ? BillingSeatQuantityProviderStatus.MutationRequired
                        : BillingSeatQuantityProviderStatus.Superseded,
                    new AuthoritativeCommercialSubscription(
                        request.BillingAccountId,
                        Projection(
                            request.BillingAccountId,
                            quantity,
                            ProviderObservedAt))));
        }

        public Task<BillingSeatQuantityProviderResult> ApplyAsync(
            BillingSeatQuantityProviderRequest request,
            DateTimeOffset automaticReplayEndsAt,
            CancellationToken cancellationToken)
        {
            cancellationToken.ThrowIfCancellationRequested();
            Mutations.Add(request);
            return Task.FromResult(
                new BillingSeatQuantityProviderResult(
                    BillingSeatQuantityProviderStatus.Applied,
                    new AuthoritativeCommercialSubscription(
                        request.BillingAccountId,
                        Projection(
                            request.BillingAccountId,
                            request.SeatQuantity,
                            ProviderObservedAt.AddMinutes(1)))));
        }
    }
}
