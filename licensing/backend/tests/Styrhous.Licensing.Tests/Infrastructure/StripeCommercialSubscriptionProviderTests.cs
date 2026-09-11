using Microsoft.Extensions.DependencyInjection;
using Microsoft.Extensions.DependencyInjection.Extensions;
using System.Data.Common;
using Microsoft.EntityFrameworkCore.Diagnostics;
using System.Net;
using Microsoft.Extensions.Options;
using Stripe;
using Styrhous.Licensing.Application.Billing;
using Styrhous.Licensing.Domain.Billing;
using Styrhous.Licensing.Infrastructure.Billing;
using Styrhous.Licensing.Persistence;
using Styrhous.Licensing.Tests.Persistence;
using static Styrhous.Licensing.Tests.Persistence.LicensingPersistenceScenario;

namespace Styrhous.Licensing.Tests.Infrastructure;

[TestFixture]
[FixtureLifeCycle(LifeCycle.InstancePerTestCase)]
[Parallelizable(ParallelScope.All)]
public sealed class StripeCommercialSubscriptionProviderTests : IDisposable
{
    private PostgresTestDatabase _database = null!;
    private readonly TestLogging _logging = new();

    [SetUp]
    public async Task SetUpAsync()
    {
        _database = await PostgresTestDatabase.CreateAsync();
    }

    [TearDown]
    public async Task TearDownAsync()
    {
        await _database.DisposeAsync();
    }

    public void Dispose()
    {
        _logging.Dispose();
    }

    private static readonly DateTimeOffset ObservedAt =
        new(2026, 8, 31, 15, 0, 0, TimeSpan.Zero);

    private static readonly string[] AuthoritativeLookupPaths =
    [
        "/v1/events/evt_authoritative",
        "/v1/subscriptions/sub_current",
    ];

    private static readonly string[] DirectSubscriptionLookupPaths =
        ["/v1/subscriptions/sub_current"];

    private static readonly int[] ExpectedMutationFenceRequestCounts = [1, 2, 3];

    [TestCase(3, 7, "always_invoice", "allow_incomplete")]
    [TestCase(3, 1, "none", null)]
    public async Task ChangesOnlyTheManagedItemQuantityWithDirectionSpecificProration(
        int previousQuantity,
        int targetQuantity,
        string expectedProrationBehavior,
        string? expectedPaymentBehavior)
    {
        var billingAccountId = Guid.CreateVersion7();
        var operationId = Guid.CreateVersion7();
        var httpClient = new RecordingStripeHttpClient(
            SubscriptionJson(
                billingAccountId,
                status: "active",
                quantity: previousQuantity),
            SubscriptionJson(
                billingAccountId,
                status: "active",
                quantity: previousQuantity),
            SubscriptionJson(
                billingAccountId,
                status: "active",
                quantity: targetQuantity));
        var provider = CreateProvider(httpClient);
        var request = new BillingSeatQuantityProviderRequest(
            operationId,
            billingAccountId,
            "sub_current",
            previousQuantity,
            targetQuantity);

        var observation = await provider.ObserveAsync(request, CancellationToken.None);
        var result = await provider.ApplyAsync(
            request,
            ObservedAt.AddHours(23),
            CancellationToken.None);

        var update = httpClient.Requests[2];
        Assert.Multiple(() =>
        {
            Assert.That(
                observation.Status,
                Is.EqualTo(BillingSeatQuantityProviderStatus.MutationRequired));
            Assert.That(result.Status, Is.EqualTo(BillingSeatQuantityProviderStatus.Applied));
            Assert.That(observation.Subscription.ProviderReadRevision, Is.GreaterThan(0));
            Assert.That(
                result.Subscription.ProviderReadRevision,
                Is.GreaterThan(observation.Subscription.ProviderReadRevision));
            Assert.That(result.Subscription.Projection.SeatQuantity, Is.EqualTo(targetQuantity));
            Assert.That(httpClient.Requests, Has.Count.EqualTo(3));
            Assert.That(httpClient.Requests[0].Method, Is.EqualTo(HttpMethod.Get));
            Assert.That(update.Method, Is.EqualTo(HttpMethod.Post));
            Assert.That(update.Path, Is.EqualTo("/v1/subscriptions/sub_current"));
            Assert.That(update.Form["items[0][id]"], Is.EqualTo("si_current"));
            Assert.That(
                update.Form["items[0][quantity]"],
                Is.EqualTo(targetQuantity.ToString(System.Globalization.CultureInfo.InvariantCulture)));
            Assert.That(
                update.Form["proration_behavior"],
                Is.EqualTo(expectedProrationBehavior));
            Assert.That(
                update.Form.GetValueOrDefault("payment_behavior"),
                Is.EqualTo(expectedPaymentBehavior));
            Assert.That(update.Form.ContainsKey("items[0][price]"), Is.False);
            Assert.That(update.Form.ContainsKey("billing_cycle_anchor"), Is.False);
            Assert.That(update.Form.Keys.Any(key => key.StartsWith("metadata", StringComparison.Ordinal)), Is.False);
            Assert.That(
                update.IdempotencyKey,
                Is.EqualTo($"styrhous-seat-quantity-{operationId:N}"));
        });
    }

    [Test]
    public async Task SuccessfulMutationResponseIsFencedAfterTheStripePostCompletes()
    {
        var billingAccountId = Guid.CreateVersion7();
        var httpClient = new RecordingStripeHttpClient(
            SubscriptionJson(billingAccountId, status: "active", quantity: 3),
            SubscriptionJson(billingAccountId, status: "active", quantity: 3),
            SubscriptionJson(billingAccountId, status: "active", quantity: 5));
        var reservations = new RevisionReservationObserver(() => httpClient.Requests.Count);
        var revisions = new PostgresBillingProviderReadRevisionSource(
            _database.CreateContextFactory(reservations));
        var provider = CreateProvider(httpClient, revisions);
        var request = new BillingSeatQuantityProviderRequest(
            Guid.CreateVersion7(),
            billingAccountId,
            "sub_current",
            PreviousSeatQuantity: 3,
            SeatQuantity: 5);

        _ = await provider.ObserveAsync(request, CancellationToken.None);
        var applied = await provider.ApplyAsync(
            request,
            ObservedAt.AddHours(23),
            CancellationToken.None);

        Assert.Multiple(() =>
        {
            Assert.That(
                reservations.RequestCountsAtReservation,
                Is.EqualTo(ExpectedMutationFenceRequestCounts));
            Assert.That(applied.Subscription.ProviderReadRevision, Is.EqualTo(3));
            Assert.That(httpClient.Requests[2].Method, Is.EqualTo(HttpMethod.Post));
        });
    }

    [Test]
    public async Task AppliedMutationUsesTheStripePostResponseTimeAndSnapshotKind()
    {
        var billingAccountId = Guid.CreateVersion7();
        var postObservedAt = ObservedAt.AddSeconds(1);
        var httpClient = new RecordingStripeHttpClient(
            (
                HttpStatusCode.OK,
                SubscriptionJson(billingAccountId, status: "active", quantity: 3),
                (DateTimeOffset?)ObservedAt),
            (
                HttpStatusCode.OK,
                SubscriptionJson(billingAccountId, status: "active", quantity: 5),
                (DateTimeOffset?)postObservedAt));
        var provider = CreateProvider(httpClient);
        var request = new BillingSeatQuantityProviderRequest(
            Guid.CreateVersion7(),
            billingAccountId,
            "sub_current",
            PreviousSeatQuantity: 3,
            SeatQuantity: 5);

        var result = await provider.ApplyAsync(
            request,
            ObservedAt.AddHours(23),
            CancellationToken.None);

        Assert.Multiple(() =>
        {
            Assert.That(result.Status, Is.EqualTo(BillingSeatQuantityProviderStatus.Applied));
            Assert.That(result.Subscription.Projection.ProjectedAt, Is.EqualTo(postObservedAt));
            Assert.That(
                result.Subscription.ProviderSnapshotKind,
                Is.EqualTo(CommercialSubscriptionSnapshotKind.MutationResponse));
            Assert.That(httpClient.Requests[1].Method, Is.EqualTo(HttpMethod.Post));
        });
    }

    [Test]
    public void MissingStripeResponseTimeFailsClosed()
    {
        var billingAccountId = Guid.CreateVersion7();
        var httpClient = new RecordingStripeHttpClient(
            (
                HttpStatusCode.OK,
                SubscriptionJson(billingAccountId, status: "active", quantity: 3),
                (DateTimeOffset?)null));
        var provider = CreateProvider(httpClient);

        Assert.That(
            async () => await provider.ObserveAsync(
                new BillingSeatQuantityProviderRequest(
                    Guid.CreateVersion7(),
                    billingAccountId,
                    "sub_current",
                    PreviousSeatQuantity: 3,
                    SeatQuantity: 5),
                CancellationToken.None),
            Throws.TypeOf<InvalidOperationException>());
    }

    [Test]
    public void MissingMutationResponseTimeFailsClosedAfterTheStripePost()
    {
        var billingAccountId = Guid.CreateVersion7();
        var httpClient = new RecordingStripeHttpClient(
            (
                HttpStatusCode.OK,
                SubscriptionJson(billingAccountId, status: "active", quantity: 3),
                (DateTimeOffset?)ObservedAt),
            (
                HttpStatusCode.OK,
                SubscriptionJson(billingAccountId, status: "active", quantity: 5),
                (DateTimeOffset?)null));
        var provider = CreateProvider(httpClient);

        Assert.That(
            async () => await provider.ApplyAsync(
                new BillingSeatQuantityProviderRequest(
                    Guid.CreateVersion7(),
                    billingAccountId,
                    "sub_current",
                    PreviousSeatQuantity: 3,
                    SeatQuantity: 5),
                ObservedAt.AddHours(23),
                CancellationToken.None),
            Throws.TypeOf<InvalidOperationException>());
        Assert.That(httpClient.Requests[1].Method, Is.EqualTo(HttpMethod.Post));
    }

    [Test]
    public async Task FreshProviderCheckAtReplayCutoffDoesNotPost()
    {
        var billingAccountId = Guid.CreateVersion7();
        var replayEndsAt = ObservedAt.AddHours(23);
        var httpClient = new RecordingStripeHttpClient(
            (
                HttpStatusCode.OK,
                SubscriptionJson(billingAccountId, status: "active", quantity: 3),
                (DateTimeOffset?)replayEndsAt));
        var provider = CreateProvider(httpClient);

        var result = await provider.ApplyAsync(
            new BillingSeatQuantityProviderRequest(
                Guid.CreateVersion7(),
                billingAccountId,
                "sub_current",
                PreviousSeatQuantity: 3,
                SeatQuantity: 5),
            replayEndsAt,
            CancellationToken.None);

        Assert.Multiple(() =>
        {
            Assert.That(
                result.Status,
                Is.EqualTo(BillingSeatQuantityProviderStatus.MutationRequired));
            Assert.That(result.Subscription.Projection.ProjectedAt, Is.EqualTo(replayEndsAt));
            Assert.That(httpClient.Requests, Has.Count.EqualTo(1));
            Assert.That(httpClient.Requests[0].Method, Is.EqualTo(HttpMethod.Get));
        });
    }

    [Test]
    public async Task DelayedEqualTimeObservationCannotOverwriteMutationResponse()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(
            database,
            "stripe-equal-time-mutation-priority",
            "stripe-equal-time-mutation-priority@example.com");
        var billingAccountId = signup.PersonalBillingAccountId;
        var httpClient = new RecordingStripeHttpClient(
            SubscriptionJson(billingAccountId, status: "active", quantity: 3),
            SubscriptionJson(billingAccountId, status: "active", quantity: 3),
            SubscriptionJson(billingAccountId, status: "active", quantity: 5));
        var reservationGate = new DatabaseCommandGate();
        var revisions = new PostgresBillingProviderReadRevisionSource(
            _database.CreateContextFactory(new DatabaseCommandGateInterceptor(
                reservationGate,
                "FROM billing_provider_read_cursors")));
        var provider = CreateProvider(httpClient, revisions);
        var request = new BillingSeatQuantityProviderRequest(
            Guid.CreateVersion7(),
            billingAccountId,
            "sub_current",
            PreviousSeatQuantity: 3,
            SeatQuantity: 5);

        var delayedObservationTask = provider.ObserveAsync(
            request,
            CancellationToken.None);
        await reservationGate.WaitUntilReachedAsync(CancellationToken.None);
        var applied = await provider.ApplyAsync(
            request,
            ObservedAt.AddHours(23),
            CancellationToken.None);
        reservationGate.Release();
        var delayedObservation = await delayedObservationTask;

        await using (var projectionTest1 = ServiceTestBase<CommercialSubscriptionProjectionService>.ForDatabase(database, ObservedAt))
        {
            await projectionTest1.Service
                .ApplyAsync(applied.Subscription);
        }

        await using (var projectionTest2 = ServiceTestBase<CommercialSubscriptionProjectionService>.ForDatabase(database, ObservedAt))
        {
            var delayedResult = await projectionTest2.Service
                .ApplyAsync(delayedObservation.Subscription);
            Assert.That(
                delayedResult.Status,
                Is.EqualTo(CommercialSubscriptionProjectionStatus.CausalConflict));
        }

        await using var verification = database.CreateContext();
        var persisted = verification.CommercialSubscriptions.Single();
        Assert.Multiple(() =>
        {
            Assert.That(applied.Subscription.ProviderReadRevision, Is.EqualTo(2));
            Assert.That(delayedObservation.Subscription.ProviderReadRevision, Is.EqualTo(3));
            Assert.That(
                applied.Subscription.Projection.ProjectedAt,
                Is.EqualTo(delayedObservation.Subscription.Projection.ProjectedAt));
            Assert.That(persisted.SeatQuantity, Is.EqualTo(5));
            Assert.That(
                persisted.ProviderSnapshotKind,
                Is.EqualTo(CommercialSubscriptionSnapshotKind.MutationResponse));
        });
    }

    [Test]
    public async Task IndeterminateMutationPollsUntilStripeShowsTheTargetWithoutPostingAgain()
    {
        var billingAccountId = Guid.CreateVersion7();
        var operationId = Guid.CreateVersion7();
        var previous = SubscriptionJson(
            billingAccountId,
            status: "active",
            quantity: 3);
        var client = new RecordingStripeHttpClient(
            (HttpStatusCode.OK, previous),
            (HttpStatusCode.OK, previous),
            (HttpStatusCode.InternalServerError, "provider response must not leak"),
            (HttpStatusCode.OK, previous),
            (HttpStatusCode.OK, SubscriptionJson(
                billingAccountId,
                status: "active",
                quantity: 5)));
        var provider = CreateProvider(client);
        var initial = new BillingSeatQuantityProviderRequest(
            operationId,
            billingAccountId,
            "sub_current",
            PreviousSeatQuantity: 3,
            SeatQuantity: 5);

        var initialObservation = await provider.ObserveAsync(initial, CancellationToken.None);
        Assert.That(
            async () => await provider.ApplyAsync(
                initial,
                ObservedAt.AddHours(23),
                CancellationToken.None),
            Throws.TypeOf<BillingSeatQuantityProviderIndeterminateException>());
        var waiting = await provider.ObserveAsync(initial, CancellationToken.None);
        var recovered = await provider.ObserveAsync(initial, CancellationToken.None);

        Assert.Multiple(() =>
        {
            Assert.That(
                initialObservation.Status,
                Is.EqualTo(BillingSeatQuantityProviderStatus.MutationRequired));
            Assert.That(
                waiting.Status,
                Is.EqualTo(BillingSeatQuantityProviderStatus.MutationRequired));
            Assert.That(recovered.Status, Is.EqualTo(BillingSeatQuantityProviderStatus.Applied));
            Assert.That(client.Requests, Has.Count.EqualTo(5));
            Assert.That(
                client.Requests[2].IdempotencyKey,
                Is.EqualTo($"styrhous-seat-quantity-{operationId:N}"));
            Assert.That(
                client.Requests.Count(request => request.Method == HttpMethod.Post),
                Is.EqualTo(1));
        });
    }

    [TestCase(
        5,
        "active",
        BillingSeatQuantityStatus.Changed,
        BillingOperationStatus.Completed,
        CommercialSubscriptionStatus.Active)]
    [TestCase(
        4,
        "active",
        BillingSeatQuantityStatus.SubscriptionQuantityChanged,
        BillingOperationStatus.Failed,
        CommercialSubscriptionStatus.Active)]
    [TestCase(
        3,
        "canceled",
        BillingSeatQuantityStatus.SubscriptionQuantityChanged,
        BillingOperationStatus.Failed,
        CommercialSubscriptionStatus.Canceled)]
    public async Task DurableServiceRetryReusesOneIdempotencyKeyUntilStripeReconciles(
        int reconciledQuantity,
        string reconciledProviderStatus,
        BillingSeatQuantityStatus expectedResultStatus,
        BillingOperationStatus expectedOperationStatus,
        CommercialSubscriptionStatus expectedSubscriptionStatus)
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(
            database,
            "stripe-indeterminate-seat-owner",
            "stripe-indeterminate-seat-owner@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Stripe indeterminate seats",
            SignupTime.AddDays(1));
        await using (var projectionTest3 = ServiceTestBase<CommercialSubscriptionProjectionService>.ForDatabase(database, ObservedAt))
        {
            await projectionTest3.Service
                .ApplyAsync(
                    organization.BillingAccountId,
                    new CommercialSubscriptionProjection(
                        "cus_current",
                        "sub_current",
                        "price_monthly",
                        CommercialSubscriptionStatus.Active,
                        seatQuantity: 3,
                        cancelAtPeriodEnd: false,
                        ObservedAt.AddDays(-1),
                        ObservedAt.AddMonths(1),
                        ObservedAt.AddMinutes(-1)),
                    CancellationToken.None);
        }

        var previous = SubscriptionJson(
            organization.BillingAccountId,
            status: "active",
            quantity: 3);
        var client = new RecordingStripeHttpClient(
            (HttpStatusCode.OK, previous),
            (HttpStatusCode.OK, previous),
            (HttpStatusCode.InternalServerError, "provider response must not leak"),
            (HttpStatusCode.OK, previous),
            (HttpStatusCode.OK, previous),
            (HttpStatusCode.InternalServerError, "provider response must not leak"),
            (HttpStatusCode.OK, SubscriptionJson(
                organization.BillingAccountId,
                status: reconciledProviderStatus,
                quantity: reconciledQuantity)));
        var provider = CreateProvider(client);

        BillingSeatQuantityResult initial;
        await using (var initialContext = CreateSeatQuantityTest(database, provider))
        {
            initial = await initialContext.Service
                .ChangeAsync(
                    owner.UserId,
                    organization.BillingAccountId,
                    seatQuantity: 5,
                    retryOperationId: null,
                    CancellationToken.None);
        }

        var operationId = initial.BillingOperationId!.Value;
        BillingSeatQuantityResult waiting;
        await using (var retryContext = CreateSeatQuantityTest(database, provider))
        {
            waiting = await retryContext.Service
                .ChangeAsync(
                    owner.UserId,
                    organization.BillingAccountId,
                    seatQuantity: 5,
                    operationId,
                    CancellationToken.None);
        }

        await using (var waitingVerification = database.CreateContext())
        {
            var operation = waitingVerification.BillingOperations.Single();
            Assert.Multiple(() =>
            {
                Assert.That(initial.Status, Is.EqualTo(BillingSeatQuantityStatus.ProviderUnavailable));
                Assert.That(waiting.Status, Is.EqualTo(BillingSeatQuantityStatus.ProviderUnavailable));
                Assert.That(operation.Status, Is.EqualTo(BillingOperationStatus.Pending));
                Assert.That(
                    waitingVerification.CommercialSubscriptions.Single().SeatQuantity,
                    Is.EqualTo(3));
            });
        }

        BillingSeatQuantityResult recovered;
        await using (var recoveryContext = CreateSeatQuantityTest(database, provider))
        {
            recovered = await recoveryContext.Service
                .ChangeAsync(
                    owner.UserId,
                    organization.BillingAccountId,
                    seatQuantity: 5,
                    operationId,
                    CancellationToken.None);
        }

        await using var verification = database.CreateContext();
        Assert.Multiple(() =>
        {
            Assert.That(recovered.Status, Is.EqualTo(expectedResultStatus));
            Assert.That(
                verification.BillingOperations.Single().Status,
                Is.EqualTo(expectedOperationStatus));
            Assert.That(
                verification.CommercialSubscriptions.Single().SeatQuantity,
                Is.EqualTo(reconciledQuantity));
            Assert.That(
                verification.CommercialSubscriptions.Single().Status,
                Is.EqualTo(expectedSubscriptionStatus));
            Assert.That(
                client.Requests.Count(request => request.Method == HttpMethod.Post),
                Is.EqualTo(2));
            Assert.That(
                client.Requests
                    .Where(request => request.Method == HttpMethod.Post)
                    .Select(request => request.IdempotencyKey),
                Has.All.EqualTo($"styrhous-seat-quantity-{operationId:N}"));
        });
    }

    [TestCase("subscription")]
    [TestCase("account")]
    [TestCase("quantity")]
    public void UpdatedSubscriptionResponseMustMatchTheRequestedMutation(string mismatch)
    {
        var billingAccountId = Guid.CreateVersion7();
        var updated = mismatch switch
        {
            "subscription" => SubscriptionJson(
                    billingAccountId,
                    status: "active",
                    quantity: 5)
                .Replace("\"id\": \"sub_current\"", "\"id\": \"sub_other\"", StringComparison.Ordinal),
            "account" => SubscriptionJson(
                Guid.CreateVersion7(),
                status: "active",
                quantity: 5),
            "quantity" => SubscriptionJson(
                billingAccountId,
                status: "active",
                quantity: 6),
            _ => throw new ArgumentOutOfRangeException(nameof(mismatch)),
        };
        var previous = SubscriptionJson(
            billingAccountId,
            status: "active",
            quantity: 3);
        var client = new RecordingStripeHttpClient(
            previous,
            previous,
            updated);
        var provider = CreateProvider(client);

        Assert.That(
            async () => await ObserveAndApplyAsync(
                provider,
                new BillingSeatQuantityProviderRequest(
                    Guid.CreateVersion7(),
                    billingAccountId,
                    "sub_current",
                    PreviousSeatQuantity: 3,
                    SeatQuantity: 5),
                CancellationToken.None),
            Throws.TypeOf<InvalidOperationException>());
        Assert.That(client.Requests, Has.Count.EqualTo(3));
    }

    [TestCase(5, 3, 3, BillingSeatQuantityProviderStatus.Applied)]
    [TestCase(3, 4, 5, BillingSeatQuantityProviderStatus.Superseded)]
    public async Task ExistingTargetIsIdempotentAndUnexpectedCurrentQuantityIsSuperseded(
        int previousQuantity,
        int currentQuantity,
        int targetQuantity,
        BillingSeatQuantityProviderStatus expectedStatus)
    {
        var billingAccountId = Guid.CreateVersion7();
        var providerClient = new RecordingStripeHttpClient(
            SubscriptionJson(
                billingAccountId,
                status: "active",
                quantity: currentQuantity));

        var result = await CreateProvider(providerClient).ObserveAsync(
            new BillingSeatQuantityProviderRequest(
                Guid.CreateVersion7(),
                billingAccountId,
                "sub_current",
                previousQuantity,
                SeatQuantity: targetQuantity),
            CancellationToken.None);

        Assert.Multiple(() =>
        {
            Assert.That(result.Status, Is.EqualTo(expectedStatus));
            Assert.That(result.Subscription.Projection.SeatQuantity, Is.EqualTo(currentQuantity));
            Assert.That(providerClient.Requests, Has.Count.EqualTo(1));
            Assert.That(providerClient.Requests[0].Method, Is.EqualTo(HttpMethod.Get));
        });
    }

    [Test]
    public void SeatQuantityChangeValidatesManagedAccountAndProviderFailures()
    {
        var expectedAccountId = Guid.CreateVersion7();
        var wrongAccount = CreateProvider(
            new RecordingStripeHttpClient(
                SubscriptionJson(
                    Guid.CreateVersion7(),
                    status: "active",
                    quantity: 3)));
        var transient = CreateProvider(
            new RecordingStripeHttpClient(
                HttpStatusCode.TooManyRequests,
                "provider response must not leak"));
        var permanent = CreateProvider(
            new RecordingStripeHttpClient(
                HttpStatusCode.BadRequest,
                "provider response must not leak"));
        var request = new BillingSeatQuantityProviderRequest(
            Guid.CreateVersion7(),
            expectedAccountId,
            "sub_current",
            PreviousSeatQuantity: 3,
            SeatQuantity: 4);

        Assert.Multiple(() =>
        {
            Assert.That(
                async () => await wrongAccount.ObserveAsync(request, CancellationToken.None),
                Throws.TypeOf<InvalidOperationException>());
            Assert.That(
                async () => await transient.ObserveAsync(request, CancellationToken.None),
                Throws.TypeOf<BillingSeatQuantityProviderUnavailableException>());
            Assert.That(
                async () => await permanent.ObserveAsync(request, CancellationToken.None),
                Throws.TypeOf<BillingSeatQuantityProviderRejectedException>());
        });
    }

    [TestCase(
        StripeBillingWebhookEventTypes.CheckoutSessionCompleted,
        BillingWebhookEventKind.CheckoutCompleted,
        "{\"object\":\"checkout.session\",\"subscription\":\"sub_current\"}")]
    [TestCase(
        StripeBillingWebhookEventTypes.CustomerSubscriptionUpdated,
        BillingWebhookEventKind.SubscriptionChanged,
        "{\"object\":\"subscription\",\"id\":\"sub_current\"}")]
    [TestCase(
        StripeBillingWebhookEventTypes.InvoicePaid,
        BillingWebhookEventKind.InvoicePaid,
        "{\"object\":\"invoice\",\"parent\":{\"type\":\"subscription_details\",\"subscription_details\":{\"subscription\":\"sub_current\"}}}")]
    [TestCase(
        StripeBillingWebhookEventTypes.InvoicePaymentFailed,
        BillingWebhookEventKind.PaymentFailed,
        "{\"object\":\"invoice\",\"parent\":{\"type\":\"subscription_details\",\"subscription_details\":{\"subscription\":\"sub_current\"}}}")]
    public async Task ResolveEventRetrievesAuthoritativeCurrentSubscription(
        string eventType,
        BillingWebhookEventKind kind,
        string eventObject)
    {
        var billingAccountId = Guid.CreateVersion7();
        var httpClient = new RecordingStripeHttpClient(
            EventJson("evt_authoritative", eventType, eventObject),
            SubscriptionJson(billingAccountId, status: "active"));
        var provider = CreateProvider(httpClient);

        var result = await provider.ResolveEventAsync(
            "evt_authoritative",
            kind,
            CancellationToken.None);

        Assert.Multiple(() =>
        {
            Assert.That(result, Is.Not.Null);
            Assert.That(result!.BillingAccountId, Is.EqualTo(billingAccountId));
            Assert.That(result.Projection.ExternalCustomerId, Is.EqualTo("cus_current"));
            Assert.That(result.Projection.ExternalSubscriptionId, Is.EqualTo("sub_current"));
            Assert.That(result.Projection.ExternalPriceId, Is.EqualTo("price_monthly"));
            Assert.That(result.Projection.Status, Is.EqualTo(CommercialSubscriptionStatus.Active));
            Assert.That(result.Projection.SeatQuantity, Is.EqualTo(3));
            Assert.That(result.Projection.CancelAtPeriodEnd, Is.True);
            Assert.That(
                result.Projection.CurrentPeriodStartedAt,
                Is.EqualTo(DateTimeOffset.FromUnixTimeSeconds(1788177600)));
            Assert.That(
                result.Projection.CurrentPeriodEndsAt,
                Is.EqualTo(DateTimeOffset.FromUnixTimeSeconds(1790856000)));
            Assert.That(result.Projection.ProjectedAt, Is.EqualTo(ObservedAt));
            Assert.That(
                httpClient.Paths,
                Is.EqualTo(AuthoritativeLookupPaths));
        });
    }

    [Test]
    public async Task ResolvesCompletedCheckoutSubscriptionWithoutAWebhookEvent()
    {
        var billingAccountId = Guid.CreateVersion7();
        var billingOperationId = Guid.CreateVersion7();
        var httpClient = new RecordingStripeHttpClient(
            SubscriptionJson(
                billingAccountId,
                status: "active",
                billingOperationId));
        var provider = CreateProvider(httpClient);

        var result = await provider.ResolveCheckoutSubscriptionAsync(
            "sub_current",
            billingOperationId,
            CancellationToken.None);

        Assert.Multiple(() =>
        {
            Assert.That(result.BillingAccountId, Is.EqualTo(billingAccountId));
            Assert.That(result.Projection.ExternalSubscriptionId, Is.EqualTo("sub_current"));
            Assert.That(httpClient.Paths, Is.EqualTo(DirectSubscriptionLookupPaths));
        });
    }

    [TestCase(false)]
    [TestCase(true)]
    public void CompletedCheckoutSubscriptionRequiresMatchingOperationMetadata(
        bool includeMismatchedOperation)
    {
        var expectedBillingOperationId = Guid.CreateVersion7();
        var returnedBillingOperationId = includeMismatchedOperation
            ? Guid.CreateVersion7()
            : (Guid?)null;
        var provider = CreateProvider(
            new RecordingStripeHttpClient(
                SubscriptionJson(
                    Guid.CreateVersion7(),
                    status: "active",
                    returnedBillingOperationId)));

        Assert.That(
            async () => await provider.ResolveCheckoutSubscriptionAsync(
                "sub_current",
                expectedBillingOperationId,
                CancellationToken.None),
            Throws.TypeOf<InvalidOperationException>());
    }

    [Test]
    public void CompletedCheckoutSubscriptionLookupDistinguishesOutageAndCancellation()
    {
        var billingOperationId = Guid.CreateVersion7();
        var unavailable = CreateProvider(
            new RecordingStripeHttpClient(
                HttpStatusCode.ServiceUnavailable,
                "provider response must not leak"));
        using var cancellation = new CancellationTokenSource();
        cancellation.Cancel();
        var cancelled = CreateProvider(
            new RecordingStripeHttpClient(
                SubscriptionJson(
                    Guid.CreateVersion7(),
                    status: "active",
                    billingOperationId)));

        Assert.Multiple(() =>
        {
            Assert.That(
                async () => await unavailable.ResolveCheckoutSubscriptionAsync(
                    "sub_current",
                    billingOperationId,
                    CancellationToken.None),
                Throws.TypeOf<BillingCheckoutProviderUnavailableException>());
            Assert.That(
                async () => await cancelled.ResolveCheckoutSubscriptionAsync(
                    "sub_current",
                    billingOperationId,
                    cancellation.Token),
                Throws.InstanceOf<OperationCanceledException>());
        });
    }

    [TestCase("active", CommercialSubscriptionStatus.Active)]
    [TestCase("past_due", CommercialSubscriptionStatus.PastDue)]
    [TestCase("unpaid", CommercialSubscriptionStatus.Unpaid)]
    [TestCase("paused", CommercialSubscriptionStatus.Paused)]
    [TestCase("incomplete", CommercialSubscriptionStatus.Incomplete)]
    [TestCase("incomplete_expired", CommercialSubscriptionStatus.IncompleteExpired)]
    [TestCase("trialing", CommercialSubscriptionStatus.Trialing)]
    [TestCase("canceled", CommercialSubscriptionStatus.Canceled)]
    public async Task ResolveEventMapsProviderStatus(
        string providerStatus,
        CommercialSubscriptionStatus expectedStatus)
    {
        var provider = CreateProvider(
            new RecordingStripeHttpClient(
                EventJson(
                    "evt_status",
                    StripeBillingWebhookEventTypes.CustomerSubscriptionUpdated,
                    "{\"object\":\"subscription\",\"id\":\"sub_current\"}"),
                SubscriptionJson(Guid.CreateVersion7(), providerStatus)));

        var result = await provider.ResolveEventAsync(
            "evt_status",
            BillingWebhookEventKind.SubscriptionChanged,
            CancellationToken.None);

        Assert.That(result!.Projection.Status, Is.EqualTo(expectedStatus));
    }

    [Test]
    public async Task EventWithoutSubscriptionIsIgnored()
    {
        var httpClient = new RecordingStripeHttpClient(
            EventJson(
                "evt_no_subscription",
                StripeBillingWebhookEventTypes.CheckoutSessionCompleted,
                "{\"object\":\"checkout.session\",\"subscription\":null}"));
        var provider = CreateProvider(httpClient);

        var result = await provider.ResolveEventAsync(
            "evt_no_subscription",
            BillingWebhookEventKind.CheckoutCompleted,
            CancellationToken.None);

        Assert.Multiple(() =>
        {
            Assert.That(result, Is.Null);
            Assert.That(httpClient.Paths, Has.Count.EqualTo(1));
        });
    }

    [Test]
    public async Task SubscriptionWithoutStyrhousMetadataIsIgnored()
    {
        var provider = CreateProvider(
            new RecordingStripeHttpClient(
                EventJson(
                    "evt_unmanaged",
                    StripeBillingWebhookEventTypes.CustomerSubscriptionCreated,
                    "{\"object\":\"subscription\",\"id\":\"sub_current\"}"),
                SubscriptionJson(billingAccountId: (string?)null, status: "active")));

        var result = await provider.ResolveEventAsync(
            "evt_unmanaged",
            BillingWebhookEventKind.SubscriptionChanged,
            CancellationToken.None);

        Assert.That(result, Is.Null);
    }

    [TestCase("not-a-uuid")]
    public void InvalidManagedSubscriptionFailsClosed(string billingAccountId)
    {
        var provider = CreateProvider(
            new RecordingStripeHttpClient(
                EventJson(
                    "evt_invalid_metadata",
                    StripeBillingWebhookEventTypes.CustomerSubscriptionCreated,
                    "{\"object\":\"subscription\",\"id\":\"sub_current\"}"),
                SubscriptionJson(billingAccountId, status: "active")));

        Assert.That(
            async () => await provider.ResolveEventAsync(
                "evt_invalid_metadata",
                BillingWebhookEventKind.SubscriptionChanged,
                CancellationToken.None),
            Throws.TypeOf<InvalidOperationException>());
    }

    [Test]
    public async Task ParseableMetadataIdentifierDoesNotRequireASpecificUuidVersion()
    {
        var accountId = Guid.NewGuid();
        var provider = CreateProvider(new RecordingStripeHttpClient(
            EventJson("evt_existing_metadata", StripeBillingWebhookEventTypes.CustomerSubscriptionCreated,
                "{\"object\":\"subscription\",\"id\":\"sub_current\"}"),
            SubscriptionJson(accountId, status: "active")));
        var result = await provider.ResolveEventAsync("evt_existing_metadata",
            BillingWebhookEventKind.SubscriptionChanged, CancellationToken.None);
        Assert.That(result!.BillingAccountId, Is.EqualTo(accountId));
    }

    [TestCase(0)]
    [TestCase(2)]
    public void ManagedSubscriptionRequiresExactlyOneItem(int itemCount)
    {
        var provider = CreateProvider(
            new RecordingStripeHttpClient(
                EventJson(
                    "evt_item_count",
                    StripeBillingWebhookEventTypes.CustomerSubscriptionUpdated,
                    "{\"object\":\"subscription\",\"id\":\"sub_current\"}"),
                SubscriptionJsonWithItemCount(
                    Guid.CreateVersion7(),
                    status: "active",
                    itemCount)));

        Assert.That(
            async () => await provider.ResolveEventAsync(
                "evt_item_count",
                BillingWebhookEventKind.SubscriptionChanged,
                CancellationToken.None),
            Throws.TypeOf<InvalidOperationException>());
    }

    [Test]
    public void EventKindMismatchFailsClosedBeforeSubscriptionLookup()
    {
        var httpClient = new RecordingStripeHttpClient(
            EventJson(
                "evt_mismatch",
                StripeBillingWebhookEventTypes.InvoicePaid,
                "{\"object\":\"invoice\",\"parent\":null}"));
        var provider = CreateProvider(httpClient);

        Assert.That(
            async () => await provider.ResolveEventAsync(
                "evt_mismatch",
                BillingWebhookEventKind.PaymentFailed,
                CancellationToken.None),
            Throws.TypeOf<InvalidOperationException>());
        Assert.That(httpClient.Paths, Has.Count.EqualTo(1));
    }

    [Test]
    public void EventObjectMismatchFailsClosedBeforeSubscriptionLookup()
    {
        var httpClient = new RecordingStripeHttpClient(
            EventJson(
                "evt_object_mismatch",
                StripeBillingWebhookEventTypes.InvoicePaid,
                "{\"object\":\"subscription\",\"id\":\"sub_current\"}"));
        var provider = CreateProvider(httpClient);

        Assert.That(
            async () => await provider.ResolveEventAsync(
                "evt_object_mismatch",
                BillingWebhookEventKind.InvoicePaid,
                CancellationToken.None),
            Throws.TypeOf<InvalidOperationException>());
        Assert.That(httpClient.Paths, Has.Count.EqualTo(1));
    }

    [TestCase("unknown")]
    [TestCase("")]
    public void UnknownOrMissingProviderStatusFailsClosed(string status)
    {
        var provider = CreateProvider(
            new RecordingStripeHttpClient(
                EventJson(
                    "evt_bad_status",
                    StripeBillingWebhookEventTypes.CustomerSubscriptionUpdated,
                    "{\"object\":\"subscription\",\"id\":\"sub_current\"}"),
                SubscriptionJson(Guid.CreateVersion7(), status)));

        Assert.That(
            async () => await provider.ResolveEventAsync(
                "evt_bad_status",
                BillingWebhookEventKind.SubscriptionChanged,
                CancellationToken.None),
            Throws.TypeOf<InvalidOperationException>());
    }

    [Test]
    public void SubscriptionUsingUnconfiguredPriceFailsClosed()
    {
        var subscription = SubscriptionJson(Guid.CreateVersion7(), status: "active")
            .Replace("price_monthly", "price_untrusted", StringComparison.Ordinal);
        var provider = CreateProvider(
            new RecordingStripeHttpClient(
                EventJson(
                    "evt_untrusted_price",
                    StripeBillingWebhookEventTypes.CustomerSubscriptionUpdated,
                    "{\"object\":\"subscription\",\"id\":\"sub_current\"}"),
                subscription));

        Assert.That(
            async () => await provider.ResolveEventAsync(
                "evt_untrusted_price",
                BillingWebhookEventKind.SubscriptionChanged,
                CancellationToken.None),
            Throws.TypeOf<InvalidOperationException>());
    }

    [Test]
    public async Task ConfiguredAnnualPriceIsAccepted()
    {
        var subscription = SubscriptionJson(Guid.CreateVersion7(), status: "active")
            .Replace("price_monthly", "price_annual", StringComparison.Ordinal);
        var provider = CreateProvider(
            new RecordingStripeHttpClient(
                EventJson(
                    "evt_annual_price",
                    StripeBillingWebhookEventTypes.CustomerSubscriptionUpdated,
                    "{\"object\":\"subscription\",\"id\":\"sub_current\"}"),
                subscription));

        var result = await provider.ResolveEventAsync(
            "evt_annual_price",
            BillingWebhookEventKind.SubscriptionChanged,
            CancellationToken.None);

        Assert.That(result!.Projection.ExternalPriceId, Is.EqualTo("price_annual"));
    }

    [Test]
    public void MissingOrNonCanonicalConfiguredPricesFailClosed()
    {
        var configurations = new[]
        {
            new StripeBillingOptions(),
            new StripeBillingOptions
            {
                MonthlyPriceId = " price_monthly",
                AnnualPriceId = "price_annual",
            },
            new StripeBillingOptions
            {
                MonthlyPriceId = "price_monthly",
                AnnualPriceId = "price_annual ",
            },
        };

        foreach (var configuration in configurations)
        {
            Assert.That(
                () => new StripeCommercialSubscriptionProvider(
                    new StripeClient(
                        "sk_test_subscription_provider",
                        httpClient: new RecordingStripeHttpClient()),
                    Options.Create(configuration),
                    new PostgresBillingProviderReadRevisionSource(_database.CreateContextFactory()),
                    _logging.GetLogger<StripeCommercialSubscriptionProvider>()),
                Throws.TypeOf<InvalidOperationException>());
        }
    }

    private StripeCommercialSubscriptionProvider CreateProvider(
        RecordingStripeHttpClient httpClient,
        PostgresBillingProviderReadRevisionSource? revisions = null)
    {
        return new(
            new StripeClient("sk_test_subscription_provider", httpClient: httpClient),
            Options.Create(
                new StripeBillingOptions
                {
                    MonthlyPriceId = "price_monthly",
                    AnnualPriceId = "price_annual",
                }),
            revisions ?? new PostgresBillingProviderReadRevisionSource(_database.CreateContextFactory()),
            _logging.GetLogger<StripeCommercialSubscriptionProvider>());
    }

    private sealed class RevisionReservationObserver(Func<int> requestCount) : DbCommandInterceptor
    {
        public List<int> RequestCountsAtReservation { get; } = [];

        public override ValueTask<InterceptionResult<DbDataReader>> ReaderExecutingAsync(
            DbCommand command,
            CommandEventData eventData,
            InterceptionResult<DbDataReader> result,
            CancellationToken cancellationToken = default)
        {
            if (command.CommandText.Contains("FROM billing_provider_read_cursors", StringComparison.OrdinalIgnoreCase))
            {
                RequestCountsAtReservation.Add(requestCount());
            }

            return ValueTask.FromResult(result);
        }
    }

    private static async Task<BillingSeatQuantityProviderResult> ObserveAndApplyAsync(
        StripeCommercialSubscriptionProvider provider,
        BillingSeatQuantityProviderRequest request,
        CancellationToken cancellationToken)
    {
        var observation = await provider.ObserveAsync(request, cancellationToken);
        return observation.Status == BillingSeatQuantityProviderStatus.MutationRequired
            ? await provider.ApplyAsync(
                request,
                ObservedAt.AddHours(23),
                cancellationToken)
            : observation;
    }

    private static ServiceTestBase<BillingSeatQuantityService> CreateSeatQuantityTest(
        PostgresTestDatabase database,
        IBillingSeatQuantityProvider provider)
    {
        return ServiceTestBase<BillingSeatQuantityService>.ForDatabase(database, ObservedAt.AddMinutes(1), services =>
        {
            services.RemoveAll<IBillingSeatQuantityProvider>();
            services.AddSingleton(provider);
        });
    }

    private static string EventJson(string eventId, string eventType, string eventObject)
    {
        return $$"""
        {
          "id": "{{eventId}}",
          "object": "event",
          "api_version": "2026-08-27.basil",
          "created": 1788177540,
          "livemode": false,
          "pending_webhooks": 1,
          "type": "{{eventType}}",
          "data": { "object": {{eventObject}} }
        }
        """;
    }

    private static string SubscriptionJson(
        Guid? billingAccountId,
        string status,
        Guid? billingOperationId = null,
        int quantity = 3)
    {
        return SubscriptionJson(
            billingAccountId?.ToString(),
            status,
            billingOperationId?.ToString(),
            quantity);
    }

    private static string SubscriptionJson(
        string? billingAccountId,
        string status,
        string? billingOperationId = null,
        int quantity = 3)
    {
        return SubscriptionJsonWithItems(
            billingAccountId,
            status,
            $$"""
            {
              "id": "si_current",
              "object": "subscription_item",
              "current_period_start": 1788177600,
              "current_period_end": 1790856000,
              "price": { "id": "price_monthly", "object": "price" },
              "quantity": {{quantity}}
            }
            """,
            billingOperationId);
    }

    private static string SubscriptionJsonWithItemCount(
        Guid billingAccountId,
        string status,
        int itemCount)
    {
        var items = string.Join(
            ',',
            Enumerable.Range(0, itemCount).Select(index => $$"""
            {
              "id": "si_{{index}}",
              "object": "subscription_item",
              "current_period_start": 1788177600,
              "current_period_end": 1790856000,
              "price": { "id": "price_{{index}}", "object": "price" },
              "quantity": 3
            }
            """));
        return SubscriptionJsonWithItems(billingAccountId.ToString(), status, items);
    }

    private static string SubscriptionJsonWithItems(
        string? billingAccountId,
        string status,
        string items,
        string? billingOperationId = null)
    {
        var metadataEntries = new List<string>();
        if (billingAccountId is not null)
        {
            metadataEntries.Add(
                $"\"styrhous_billing_account_id\": \"{billingAccountId}\"");
        }

        if (billingOperationId is not null)
        {
            metadataEntries.Add(
                $"\"styrhous_billing_operation_id\": \"{billingOperationId}\"");
        }

        var metadata = $"{{ {string.Join(", ", metadataEntries)} }}";
        return $$"""
        {
          "id": "sub_current",
          "object": "subscription",
          "customer": "cus_current",
          "status": "{{status}}",
          "cancel_at_period_end": true,
          "metadata": {{metadata}},
          "items": {
            "object": "list",
            "data": [{{items}}],
            "has_more": false,
            "url": "/v1/subscription_items?subscription=sub_current"
          }
        }
        """;
    }

    private sealed class RecordingStripeHttpClient : IHttpClient
    {
        private readonly Queue<(
            HttpStatusCode StatusCode,
            string Body,
            DateTimeOffset? ObservedAt)> _responses;

        public RecordingStripeHttpClient(params string[] responses)
        {
            _responses = new Queue<(HttpStatusCode, string, DateTimeOffset?)>(
                responses.Select(response =>
                    (HttpStatusCode.OK, response, (DateTimeOffset?)ObservedAt)));
        }

        public RecordingStripeHttpClient(HttpStatusCode statusCode, string response)
        {
            _responses = new Queue<(HttpStatusCode, string, DateTimeOffset?)>(
                [(statusCode, response, ObservedAt)]);
        }

        public RecordingStripeHttpClient(
            (HttpStatusCode StatusCode, string Body) first,
            params (HttpStatusCode StatusCode, string Body)[] remaining)
        {
            _responses = new Queue<(HttpStatusCode, string, DateTimeOffset?)>(
                remaining.Prepend(first)
                    .Select(response =>
                        (response.StatusCode, response.Body, (DateTimeOffset?)ObservedAt)));
        }

        public RecordingStripeHttpClient(
            (HttpStatusCode StatusCode, string Body, DateTimeOffset? ObservedAt) first,
            params (
                HttpStatusCode StatusCode,
                string Body,
                DateTimeOffset? ObservedAt)[] remaining)
        {
            _responses = new Queue<(HttpStatusCode, string, DateTimeOffset?)>(
                [first, .. remaining]);
        }

        public List<string> Paths { get; } = [];

        public List<RecordedStripeRequest> Requests { get; } = [];

        public async Task<StripeResponse> MakeRequestAsync(
            StripeRequest request,
            CancellationToken cancellationToken)
        {
            cancellationToken.ThrowIfCancellationRequested();
            Paths.Add(request.Uri.AbsolutePath);
            var content = request.Content is null
                ? string.Empty
                : await request.Content.ReadAsStringAsync(cancellationToken);
            request.StripeHeaders.TryGetValue("Idempotency-Key", out var idempotencyKey);
            Requests.Add(
                new RecordedStripeRequest(
                    request.Method,
                    request.Uri.AbsolutePath,
                    ParseForm(content),
                    idempotencyKey));
            if (!_responses.TryDequeue(out var response))
            {
                throw new InvalidOperationException("No Stripe response was configured.");
            }

            using var responseMessage = new HttpResponseMessage(response.StatusCode);
            if (response.ObservedAt is not null)
            {
                responseMessage.Headers.Date = response.ObservedAt;
            }
            return new StripeResponse(
                response.StatusCode,
                responseMessage.Headers,
                response.Body);
        }

        public Task<StripeStreamedResponse> MakeStreamingRequestAsync(
            StripeRequest request,
            CancellationToken cancellationToken)
        {
            throw new NotSupportedException("Streaming is not used by billing tests.");
        }

        private static Dictionary<string, string> ParseForm(string content)
        {
            return content.Split('&', StringSplitOptions.RemoveEmptyEntries)
                .Select(pair => pair.Split('=', 2))
                .ToDictionary(
                    pair => Decode(pair[0]),
                    pair => pair.Length == 2 ? Decode(pair[1]) : string.Empty,
                    StringComparer.Ordinal);
        }

        private static string Decode(string value)
        {
            return Uri.UnescapeDataString(value.Replace('+', ' '));
        }
    }

    private sealed record RecordedStripeRequest(
        HttpMethod Method,
        string Path,
        IReadOnlyDictionary<string, string> Form,
        string? IdempotencyKey);
}
