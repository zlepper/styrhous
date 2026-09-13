using Microsoft.EntityFrameworkCore;
using Styrhous.Licensing.Application.Billing;
using Styrhous.Licensing.Domain.Billing;
using Styrhous.Licensing.Infrastructure.Billing;
using Styrhous.Licensing.Persistence;

namespace Styrhous.Licensing.Tests.Persistence;

[TestFixture]
[Parallelizable(ParallelScope.All)]
public sealed class BillingWebhookInboxPersistenceTests
{
    private static readonly DateTimeOffset OccurredAt =
        new(2026, 8, 31, 11, 59, 0, TimeSpan.Zero);

    private static readonly DateTimeOffset ReceivedAt =
        new(2026, 8, 31, 12, 0, 0, TimeSpan.Zero);

    [Test]
    public async Task ReceivePersistsProviderMetadataBehindEf()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var store = new PostgresBillingWebhookInboxStore(
            database.CreateContextFactory(), new PostgresBackgroundWorkOutbox("test-queue"));

        var result = await store.ReceiveAsync(
            Event("evt_persisted"),
            ReceivedAt,
            BillingWebhookInboxDisposition.PendingProcessing,
            CancellationToken.None);

        await using var context = database.CreateContextWithCopenhagenTimeZone();
        var stored = await context.BillingWebhookEvents.AsNoTracking().SingleAsync();
        Assert.Multiple(() =>
        {
            Assert.That(result.Status, Is.EqualTo(BillingWebhookInboxStoreStatus.Received));
            Assert.That(result.InboxEventId.Version, Is.EqualTo(7));
            Assert.That(stored.Id, Is.EqualTo(result.InboxEventId));
            Assert.That(stored.ExternalEventId, Is.EqualTo("evt_persisted"));
            Assert.That(
                stored.EventType,
                Is.EqualTo(StripeBillingWebhookEventTypes.CustomerSubscriptionUpdated));
            Assert.That(stored.Kind, Is.EqualTo(BillingWebhookEventKind.SubscriptionChanged));
            Assert.That(stored.OccurredAt, Is.EqualTo(OccurredAt));
            Assert.That(stored.ReceivedAt, Is.EqualTo(ReceivedAt));
            Assert.That(stored.ProcessedAt, Is.Null);
        });
    }

    [Test]
    public async Task SequentialDuplicateReturnsOriginalInboxIdentity()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var store = new PostgresBillingWebhookInboxStore(
            database.CreateContextFactory(), new PostgresBackgroundWorkOutbox("test-queue"));
        var first = await store.ReceiveAsync(
            Event("evt_duplicate"),
            ReceivedAt,
            BillingWebhookInboxDisposition.PendingProcessing,
            CancellationToken.None);

        var duplicate = await store.ReceiveAsync(
            Event("evt_duplicate"),
            ReceivedAt.AddMinutes(1),
            BillingWebhookInboxDisposition.PendingProcessing,
            CancellationToken.None);
        var inboxEventCount = await CountInboxEventsAsync(database);

        Assert.Multiple(() =>
        {
            Assert.That(first.Status, Is.EqualTo(BillingWebhookInboxStoreStatus.Received));
            Assert.That(
                duplicate.Status,
                Is.EqualTo(BillingWebhookInboxStoreStatus.Duplicate));
            Assert.That(duplicate.InboxEventId, Is.EqualTo(first.InboxEventId));
            Assert.That(inboxEventCount, Is.EqualTo(1));
        });
    }

    [Test]
    public async Task ConcurrentDuplicateCreatesExactlyOneInboxRecord()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var barrier = new DatabaseCommandBarrier(participantCount: 2);
        var stores = new[]
        {
            new PostgresBillingWebhookInboxStore(
                database.CreateContextFactory(
                    new DatabaseCommandBarrierInterceptor(
                        barrier,
                        "FROM billing_webhook_events",
                        DatabaseCommandInterceptionPhase.AfterReaderExecution)), new PostgresBackgroundWorkOutbox("test-queue")),
            new PostgresBillingWebhookInboxStore(
                database.CreateContextFactory(
                    new DatabaseCommandBarrierInterceptor(
                        barrier,
                        "FROM billing_webhook_events",
                        DatabaseCommandInterceptionPhase.AfterReaderExecution)), new PostgresBackgroundWorkOutbox("test-queue")),
        };
        var results = await Task.WhenAll(
            stores.Select(store => store.ReceiveAsync(
                Event("evt_concurrent"),
                ReceivedAt,
                BillingWebhookInboxDisposition.PendingProcessing,
                CancellationToken.None)));

        await using var verificationContext = database.CreateContext();
        Assert.Multiple(() =>
        {
            Assert.That(
                results.Count(result =>
                    result.Status == BillingWebhookInboxStoreStatus.Received),
                Is.EqualTo(1));
            Assert.That(
                results.Count(result =>
                    result.Status == BillingWebhookInboxStoreStatus.Duplicate),
                Is.EqualTo(1));
            Assert.That(
                results.Select(result => result.InboxEventId).Distinct(),
                Has.Exactly(1).Items);
            Assert.That(verificationContext.BillingWebhookEvents.Count(), Is.EqualTo(1));
            Assert.That(barrier.ArrivedCount, Is.EqualTo(2));
        });
    }

    [Test]
    public async Task ProcessingLeaseCompletesEventAndPersistsAttempt()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var inboxEventId = await ReceivePendingAsync(database, "evt_process_complete");
        var store = new PostgresBillingWebhookProcessingStore(
            database.CreateContextFactory());
        var leaseId = Guid.CreateVersion7();

        var claim = await store.TryAcquireAsync(
            inboxEventId,
            leaseId,
            ReceivedAt,
            ReceivedAt.AddMinutes(5),
            CancellationToken.None);
        var completed = await store.CompleteAsync(
            inboxEventId,
            leaseId,
            ReceivedAt.AddMinutes(1),
            CancellationToken.None);

        await using var context = database.CreateContextWithCopenhagenTimeZone();
        var stored = await context.BillingWebhookEvents.AsNoTracking().SingleAsync();
        Assert.Multiple(() =>
        {
            Assert.That(claim.Status, Is.EqualTo(BillingWebhookProcessingClaimStatus.Acquired));
            Assert.That(claim.Claim!.InboxEventId, Is.EqualTo(inboxEventId));
            Assert.That(claim.Claim.LeaseId, Is.EqualTo(leaseId));
            Assert.That(claim.Claim.ExternalEventId, Is.EqualTo("evt_process_complete"));
            Assert.That(claim.Claim.Kind, Is.EqualTo(BillingWebhookEventKind.SubscriptionChanged));
            Assert.That(completed, Is.True);
            Assert.That(stored.ProcessedAt, Is.EqualTo(ReceivedAt.AddMinutes(1)));
            Assert.That(stored.ProcessingLeaseId, Is.Null);
            Assert.That(stored.ProcessingLeaseExpiresAt, Is.Null);
            Assert.That(stored.ProcessingAttemptCount, Is.EqualTo(1));
        });
    }

    [Test]
    public async Task ReleasedLeaseCanBeRetried()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var inboxEventId = await ReceivePendingAsync(database, "evt_process_release");
        var store = new PostgresBillingWebhookProcessingStore(
            database.CreateContextFactory());
        var firstLeaseId = Guid.CreateVersion7();
        var secondLeaseId = Guid.CreateVersion7();
        await store.TryAcquireAsync(
            inboxEventId,
            firstLeaseId,
            ReceivedAt,
            ReceivedAt.AddMinutes(5),
            CancellationToken.None);

        var wrongRelease = await store.ReleaseAsync(
            inboxEventId,
            secondLeaseId,
            CancellationToken.None);
        var released = await store.ReleaseAsync(
            inboxEventId,
            firstLeaseId,
            CancellationToken.None);
        var retried = await store.TryAcquireAsync(
            inboxEventId,
            secondLeaseId,
            ReceivedAt.AddMinutes(1),
            ReceivedAt.AddMinutes(6),
            CancellationToken.None);

        await using var context = database.CreateContext();
        var stored = await context.BillingWebhookEvents.AsNoTracking().SingleAsync();
        Assert.Multiple(() =>
        {
            Assert.That(wrongRelease, Is.False);
            Assert.That(released, Is.True);
            Assert.That(retried.Status, Is.EqualTo(BillingWebhookProcessingClaimStatus.Acquired));
            Assert.That(stored.ProcessingLeaseId, Is.EqualTo(secondLeaseId));
            Assert.That(stored.ProcessingAttemptCount, Is.EqualTo(2));
        });
    }

    [Test]
    public async Task ActiveLeaseIsBusyAndExpiredLeaseCanBeReclaimed()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var inboxEventId = await ReceivePendingAsync(database, "evt_process_expiry");
        var store = new PostgresBillingWebhookProcessingStore(
            database.CreateContextFactory());
        await store.TryAcquireAsync(
            inboxEventId,
            Guid.CreateVersion7(),
            ReceivedAt,
            ReceivedAt.AddMinutes(5),
            CancellationToken.None);

        var busy = await store.TryAcquireAsync(
            inboxEventId,
            Guid.CreateVersion7(),
            ReceivedAt.AddMinutes(4),
            ReceivedAt.AddMinutes(9),
            CancellationToken.None);
        var reclaimedLeaseId = Guid.CreateVersion7();
        var reclaimed = await store.TryAcquireAsync(
            inboxEventId,
            reclaimedLeaseId,
            ReceivedAt.AddMinutes(5),
            ReceivedAt.AddMinutes(10),
            CancellationToken.None);

        Assert.Multiple(() =>
        {
            Assert.That(busy.Status, Is.EqualTo(BillingWebhookProcessingClaimStatus.Busy));
            Assert.That(reclaimed.Status, Is.EqualTo(BillingWebhookProcessingClaimStatus.Acquired));
            Assert.That(reclaimed.Claim!.LeaseId, Is.EqualTo(reclaimedLeaseId));
        });
    }

    [Test]
    public async Task ConcurrentClaimAcquiresExactlyOneLease()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var inboxEventId = await ReceivePendingAsync(database, "evt_process_concurrent");
        var barrier = new DatabaseCommandBarrier(participantCount: 2);
        var stores = new[]
        {
            new PostgresBillingWebhookProcessingStore(
                database.CreateContextFactory(
                    new DatabaseCommandBarrierInterceptor(
                        barrier,
                        "FROM billing_webhook_events",
                        DatabaseCommandInterceptionPhase.AfterReaderExecution))),
            new PostgresBillingWebhookProcessingStore(
                database.CreateContextFactory(
                    new DatabaseCommandBarrierInterceptor(
                        barrier,
                        "FROM billing_webhook_events",
                        DatabaseCommandInterceptionPhase.AfterReaderExecution))),
        };

        var claims = await Task.WhenAll(
            stores.Select(store => store.TryAcquireAsync(
                inboxEventId,
                Guid.CreateVersion7(),
                ReceivedAt,
                ReceivedAt.AddMinutes(5),
                CancellationToken.None)));

        Assert.Multiple(() =>
        {
            Assert.That(
                claims.Count(claim =>
                    claim.Status == BillingWebhookProcessingClaimStatus.Acquired),
                Is.EqualTo(1));
            Assert.That(
                claims.Count(claim =>
                    claim.Status == BillingWebhookProcessingClaimStatus.Busy),
                Is.EqualTo(1));
            Assert.That(barrier.ArrivedCount, Is.EqualTo(2));
        });
    }

    [TestCase(true)]
    [TestCase(false)]
    public async Task StaleLeaseCannotCompleteOrReleaseReclaimedLease(bool complete)
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var inboxEventId = await ReceivePendingAsync(database, "evt_process_reclaimed_race");
        var firstLeaseId = Guid.CreateVersion7();
        var secondLeaseId = Guid.CreateVersion7();
        var currentStore = new PostgresBillingWebhookProcessingStore(
            database.CreateContextFactory());
        await currentStore.TryAcquireAsync(
            inboxEventId,
            firstLeaseId,
            ReceivedAt,
            ReceivedAt.AddMinutes(5),
            CancellationToken.None);
        var staleReadGate = new DatabaseCommandGate();
        var staleStore = new PostgresBillingWebhookProcessingStore(
            database.CreateContextFactory(
                new DatabaseCommandGateInterceptor(
                    staleReadGate,
                    "FROM billing_webhook_events",
                    DatabaseCommandInterceptionPhase.AfterReaderExecution)));
        var staleOperation = complete
            ? staleStore.CompleteAsync(
                inboxEventId,
                firstLeaseId,
                ReceivedAt.AddMinutes(6),
                CancellationToken.None)
            : staleStore.ReleaseAsync(
                inboxEventId,
                firstLeaseId,
                CancellationToken.None);
        BillingWebhookProcessingClaimResult reclaimed;
        try
        {
            await staleReadGate.WaitUntilReachedAsync();
            reclaimed = await currentStore.TryAcquireAsync(
                inboxEventId,
                secondLeaseId,
                ReceivedAt.AddMinutes(5),
                ReceivedAt.AddMinutes(10),
                CancellationToken.None);
        }
        finally
        {
            staleReadGate.Release();
        }

        var staleResult = await staleOperation;
        await using var context = database.CreateContext();
        var stored = await context.BillingWebhookEvents.AsNoTracking().SingleAsync();
        Assert.Multiple(() =>
        {
            Assert.That(reclaimed.Status, Is.EqualTo(BillingWebhookProcessingClaimStatus.Acquired));
            Assert.That(staleResult, Is.False);
            Assert.That(stored.ProcessedAt, Is.Null);
            Assert.That(stored.ProcessingLeaseId, Is.EqualTo(secondLeaseId));
            Assert.That(
                stored.ProcessingLeaseExpiresAt,
                Is.EqualTo(ReceivedAt.AddMinutes(10)));
            Assert.That(stored.ProcessingAttemptCount, Is.EqualTo(2));
        });
    }

    [Test]
    public async Task MissingAndProcessedEventsCannotBeClaimed()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var inboxStore = new PostgresBillingWebhookInboxStore(
            database.CreateContextFactory(), new PostgresBackgroundWorkOutbox("test-queue"));
        var ignored = await inboxStore.ReceiveAsync(
            Event("evt_process_ignored"),
            ReceivedAt,
            BillingWebhookInboxDisposition.Ignored,
            CancellationToken.None);
        var store = new PostgresBillingWebhookProcessingStore(
            database.CreateContextFactory());

        var missing = await store.TryAcquireAsync(
            Guid.CreateVersion7(),
            Guid.CreateVersion7(),
            ReceivedAt,
            ReceivedAt.AddMinutes(5),
            CancellationToken.None);
        var alreadyProcessed = await store.TryAcquireAsync(
            ignored.InboxEventId,
            Guid.CreateVersion7(),
            ReceivedAt,
            ReceivedAt.AddMinutes(5),
            CancellationToken.None);

        Assert.Multiple(() =>
        {
            Assert.That(missing.Status, Is.EqualTo(BillingWebhookProcessingClaimStatus.NotFound));
            Assert.That(
                alreadyProcessed.Status,
                Is.EqualTo(BillingWebhookProcessingClaimStatus.AlreadyProcessed));
        });
    }

    private static VerifiedBillingWebhookEvent Event(string externalEventId)
    {
        return new(
            externalEventId,
            StripeBillingWebhookEventTypes.CustomerSubscriptionUpdated,
            BillingWebhookEventKind.SubscriptionChanged,
            OccurredAt);
    }

    private static async Task<int> CountInboxEventsAsync(PostgresTestDatabase database)
    {
        await using var context = database.CreateContext();
        return await context.BillingWebhookEvents.CountAsync();
    }

    private static async Task<Guid> ReceivePendingAsync(
        PostgresTestDatabase database,
        string externalEventId)
    {
        var store = new PostgresBillingWebhookInboxStore(
            database.CreateContextFactory(), new PostgresBackgroundWorkOutbox("test-queue"));
        var result = await store.ReceiveAsync(
            Event(externalEventId),
            ReceivedAt,
            BillingWebhookInboxDisposition.PendingProcessing,
            CancellationToken.None);
        return result.InboxEventId;
    }
}
