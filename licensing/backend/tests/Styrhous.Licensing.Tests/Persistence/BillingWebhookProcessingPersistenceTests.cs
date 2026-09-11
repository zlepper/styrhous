using Microsoft.Extensions.DependencyInjection;
using Microsoft.Extensions.DependencyInjection.Extensions;
using Microsoft.EntityFrameworkCore;
using Styrhous.Licensing.Application.Billing;
using Styrhous.Licensing.Domain.Billing;
using Styrhous.Licensing.Persistence;
using static Styrhous.Licensing.Tests.Persistence.LicensingPersistenceScenario;

namespace Styrhous.Licensing.Tests.Persistence;

[TestFixture]
[Parallelizable(ParallelScope.All)]
public sealed class BillingWebhookProcessingPersistenceTests
{
    private static readonly DateTimeOffset ProcessingAt = SignupTime.AddDays(40);

    [Test]
    public async Task OutOfOrderPaymentAndCancellationEventsConvergeOnNewestSnapshot()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(database, "webhook-processing", "billing@example.com");
        var paymentFailed = await ReceiveAsync(
            database,
            "evt_payment_failed",
            "invoice.payment_failed",
            BillingWebhookEventKind.PaymentFailed);
        var cancelled = await ReceiveAsync(
            database,
            "evt_cancelled",
            "customer.subscription.deleted",
            BillingWebhookEventKind.SubscriptionChanged);
        var olderPaid = await ReceiveAsync(
            database,
            "evt_older_paid",
            "invoice.paid",
            BillingWebhookEventKind.InvoicePaid);
        var provider = new DictionarySubscriptionProvider(
            new Dictionary<string, AuthoritativeCommercialSubscription>
            {
                ["evt_payment_failed"] = Snapshot(
                    signup.PersonalBillingAccountId,
                    CommercialSubscriptionStatus.PastDue,
                    ProcessingAt.AddMinutes(2)),
                ["evt_cancelled"] = Snapshot(
                    signup.PersonalBillingAccountId,
                    CommercialSubscriptionStatus.Canceled,
                    ProcessingAt.AddMinutes(3)),
                ["evt_older_paid"] = Snapshot(
                    signup.PersonalBillingAccountId,
                    CommercialSubscriptionStatus.Active,
                    ProcessingAt.AddMinutes(1)),
            });
        await using var processingTest = WorkerServiceTestBase.ForDatabase<BillingWebhookProcessingService>(
            database, ProcessingAt, services =>
            {
                services.RemoveAll<ICommercialSubscriptionProvider>();
                services.AddSingleton<ICommercialSubscriptionProvider>(provider);
            });
        var service = processingTest.Service;

        var results = new[]
        {
            await service.ProcessAsync(paymentFailed),
            await service.ProcessAsync(cancelled),
            await service.ProcessAsync(olderPaid),
        };

        await using var verificationContext = database.CreateContext();
        var subscription = await verificationContext.CommercialSubscriptions
            .AsNoTracking()
            .SingleAsync();
        var inboxEvents = await verificationContext.BillingWebhookEvents
            .AsNoTracking()
            .OrderBy(webhookEvent => webhookEvent.ExternalEventId)
            .ToArrayAsync();
        Assert.Multiple(() =>
        {
            Assert.That(
                results.Select(result => result.Status),
                Is.All.EqualTo(BillingWebhookProcessingStatus.Processed));
            Assert.That(subscription.Status, Is.EqualTo(CommercialSubscriptionStatus.Canceled));
            Assert.That(subscription.ProjectedAt, Is.EqualTo(ProcessingAt.AddMinutes(3)));
            Assert.That(inboxEvents, Has.Length.EqualTo(3));
            Assert.That(inboxEvents, Has.All.Property("ProcessedAt").EqualTo(ProcessingAt));
            Assert.That(inboxEvents, Has.All.Property("ProcessingAttemptCount").EqualTo(1));
            Assert.That(inboxEvents, Has.All.Property("ProcessingLeaseId").Null);
        });
    }

    [Test]
    public async Task ProjectionFailureReleasesLeaseForRetry()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var inboxEventId = await ReceiveAsync(
            database,
            "evt_unknown_account",
            "invoice.paid",
            BillingWebhookEventKind.InvoicePaid);
        var provider = new DictionarySubscriptionProvider(
            new Dictionary<string, AuthoritativeCommercialSubscription>
            {
                ["evt_unknown_account"] = Snapshot(
                    Guid.CreateVersion7(),
                    CommercialSubscriptionStatus.Active,
                    ProcessingAt),
            });
        await using var processingTest = WorkerServiceTestBase.ForDatabase<BillingWebhookProcessingService>(
            database, ProcessingAt, services =>
            {
                services.RemoveAll<ICommercialSubscriptionProvider>();
                services.AddSingleton<ICommercialSubscriptionProvider>(provider);
            });
        var service = processingTest.Service;

        Assert.That(
            async () => await service.ProcessAsync(inboxEventId),
            Throws.TypeOf<InvalidOperationException>());

        await using var verificationContext = database.CreateContext();
        var inboxEvent = await verificationContext.BillingWebhookEvents
            .AsNoTracking()
            .SingleAsync();
        Assert.Multiple(() =>
        {
            Assert.That(inboxEvent.ProcessedAt, Is.Null);
            Assert.That(inboxEvent.ProcessingLeaseId, Is.Null);
            Assert.That(inboxEvent.ProcessingLeaseExpiresAt, Is.Null);
            Assert.That(inboxEvent.ProcessingAttemptCount, Is.EqualTo(1));
        });
    }

    private static async Task<Guid> ReceiveAsync(
        PostgresTestDatabase database,
        string externalEventId,
        string eventType,
        BillingWebhookEventKind kind)
    {
        var result = await new PostgresBillingWebhookInboxStore(
                database.CreateContextFactory(), new PostgresBackgroundWorkOutbox("test-queue"))
            .ReceiveAsync(
                new VerifiedBillingWebhookEvent(
                    externalEventId,
                    eventType,
                    kind,
                    ProcessingAt.AddMinutes(-1)),
                ProcessingAt,
                BillingWebhookInboxDisposition.PendingProcessing,
                CancellationToken.None);
        return result.InboxEventId;
    }

    private static AuthoritativeCommercialSubscription Snapshot(
        Guid billingAccountId,
        CommercialSubscriptionStatus status,
        DateTimeOffset projectedAt)
    {
        return new(
            billingAccountId,
            new CommercialSubscriptionProjection(
                "cus_processing",
                "sub_processing",
                "price_processing",
                status,
                3,
                status == CommercialSubscriptionStatus.Canceled,
                projectedAt.AddDays(-1),
                projectedAt.AddMonths(1),
                projectedAt));
    }

    private sealed class DictionarySubscriptionProvider(
        IReadOnlyDictionary<string, AuthoritativeCommercialSubscription> snapshots)
        : ICommercialSubscriptionProvider
    {
        public Task<AuthoritativeCommercialSubscription?> ResolveEventAsync(
            string externalEventId,
            BillingWebhookEventKind kind,
            CancellationToken cancellationToken)
        {
            cancellationToken.ThrowIfCancellationRequested();
            return Task.FromResult<AuthoritativeCommercialSubscription?>(
                snapshots[externalEventId]);
        }

        public Task<AuthoritativeCommercialSubscription> ResolveCheckoutSubscriptionAsync(
            string externalSubscriptionId,
            Guid expectedBillingOperationId,
            CancellationToken cancellationToken)
        {
            throw new AssertionException(
                "Webhook processing must resolve subscriptions through their event.");
        }
    }
}
