using Microsoft.EntityFrameworkCore;
using Microsoft.Extensions.DependencyInjection;
using Microsoft.Extensions.DependencyInjection.Extensions;
using Styrhous.Licensing.Application.Billing;
using Styrhous.Licensing.Domain.Billing;
using Styrhous.Licensing.Persistence;
using static Styrhous.Licensing.Tests.Persistence.LicensingPersistenceScenario;

namespace Styrhous.Licensing.Tests.Application;

[TestFixture]
[Parallelizable(ParallelScope.All)]
public sealed class BillingWebhookProcessingServiceTests
{
    private static readonly DateTimeOffset ObservedAt = SignupTime.AddDays(2);

    [TestCase(false)]
    [TestCase(true)]
    public async Task ProcessingPersistsSnapshotAndCompletesLease(bool unmanaged)
    {
        var provider = new RecordingSubscriptionProvider();
        await using var test = await CreateAsync(provider);
        var signup = await SignUpAsync(test.Database);
        provider.Result = unmanaged ? null : Snapshot(signup.PersonalBillingAccountId);
        var id = await ReceiveAsync(test);
        provider.BeforeReturn = async () =>
        {
            await using var verification = test.Database.CreateContext();
            var claimed = await verification.BillingWebhookEvents.SingleAsync();
            Assert.That(claimed.ProcessingLeaseId!.Value.Version, Is.EqualTo(7));
            Assert.That(claimed.ProcessingLeaseExpiresAt, Is.EqualTo(ObservedAt.AddMinutes(5)));
        };

        var result = await test.Service.ProcessAsync(id);
        var duplicate = await test.Service.ProcessAsync(id);
        await using var context = test.Database.CreateContext();
        var stored = await context.BillingWebhookEvents.SingleAsync();
        Assert.Multiple(() =>
        {
            Assert.That(result.Status, Is.EqualTo(unmanaged
                ? BillingWebhookProcessingStatus.Ignored : BillingWebhookProcessingStatus.Processed));
            Assert.That(duplicate.Status, Is.EqualTo(BillingWebhookProcessingStatus.AlreadyProcessed));
            Assert.That(provider.Calls, Is.EqualTo(1));
            Assert.That(provider.ExternalEventId, Is.EqualTo("evt_process"));
            Assert.That(provider.Kind, Is.EqualTo(BillingWebhookEventKind.InvoicePaid));
            Assert.That(stored.ProcessedAt, Is.EqualTo(ObservedAt));
            Assert.That(stored.ProcessingLeaseId, Is.Null);
            Assert.That(context.CommercialSubscriptions.Count(), Is.EqualTo(unmanaged ? 0 : 1));
        });
        if (!unmanaged)
        {
            var subscription = await context.CommercialSubscriptions.SingleAsync();
            Assert.That(subscription.BillingAccountId, Is.EqualTo(signup.PersonalBillingAccountId));
            Assert.That(subscription.Status, Is.EqualTo(CommercialSubscriptionStatus.Active));
        }
    }

    [TestCase(false)]
    [TestCase(true)]
    public async Task ProcessingFailureReleasesLeaseForRetry(bool missingAccount)
    {
        var failure = new InvalidOperationException("provider failed");
        var provider = new RecordingSubscriptionProvider
        {
            Result = missingAccount ? Snapshot(Guid.NewGuid()) : null,
            Failure = missingAccount ? null : failure,
        };
        await using var test = await CreateAsync(provider);
        var id = await ReceiveAsync(test);
        var exception = Assert.ThrowsAsync<InvalidOperationException>(async () => await test.Service.ProcessAsync(id));
        if (!missingAccount)
        {
            Assert.That(exception, Is.SameAs(failure));
        }
        await using var context = test.Database.CreateContext();
        var stored = await context.BillingWebhookEvents.SingleAsync();
        Assert.Multiple(() =>
        {
            Assert.That(stored.ProcessedAt, Is.Null);
            Assert.That(stored.ProcessingLeaseId, Is.Null);
            Assert.That(stored.ProcessingAttemptCount, Is.EqualTo(1));
        });
        provider.Failure = null;
        provider.Result = null;
        var retry = await test.Service.ProcessAsync(id);
        Assert.That(retry.Status, Is.EqualTo(BillingWebhookProcessingStatus.Ignored));
    }

    [Test]
    public async Task CausallyConflictingObservationReleasesLeaseForAuthoritativeReread()
    {
        var provider = new RecordingSubscriptionProvider();
        await using var test = await CreateAsync(provider);
        var signup = await SignUpAsync(test.Database);
        var snapshot = Snapshot(signup.PersonalBillingAccountId);
        await using (var scope = test.CreateScope())
        {
            await scope.ServiceProvider.GetRequiredService<CommercialSubscriptionProjectionService>()
                .ApplyAsync(snapshot with
                {
                    ProviderReadRevision = 1,
                    ProviderSnapshotKind = CommercialSubscriptionSnapshotKind.MutationResponse,
                });
        }
        provider.Result = snapshot with
        {
            Projection = new CommercialSubscriptionProjection("cus_process", "sub_process", "price_process",
                CommercialSubscriptionStatus.Canceled, 3, true, ObservedAt.AddDays(-1), ObservedAt.AddMonths(1), ObservedAt),
            ProviderReadRevision = 2,
            ProviderSnapshotKind = CommercialSubscriptionSnapshotKind.Observation,
        };
        var id = await ReceiveAsync(test);
        var exception = Assert.ThrowsAsync<InvalidOperationException>(async () => await test.Service.ProcessAsync(id));
        Assert.That(exception!.Message, Does.Contain("resolved again"));
        await using var context = test.Database.CreateContext();
        var stored = await context.BillingWebhookEvents.SingleAsync();
        Assert.That(stored.ProcessingLeaseId, Is.Null);
        Assert.That(stored.ProcessedAt, Is.Null);
        Assert.That((await context.CommercialSubscriptions.SingleAsync()).Status,
            Is.EqualTo(CommercialSubscriptionStatus.Active));
    }

    [Test]
    public async Task LostLeaseCompletionFailsWithoutOverwritingNewOwner()
    {
        var provider = new RecordingSubscriptionProvider();
        await using var test = await CreateAsync(provider);
        var id = await ReceiveAsync(test);
        var newOwner = Guid.CreateVersion7();
        provider.BeforeReturn = async () =>
        {
            await using var context = test.Database.CreateContext();
            await context.BillingWebhookEvents.Where(item => item.Id == id)
                .ExecuteUpdateAsync(setters => setters.SetProperty(item => item.ProcessingLeaseId, newOwner));
        };
        var exception = Assert.ThrowsAsync<InvalidOperationException>(async () => await test.Service.ProcessAsync(id));
        Assert.That(exception!.Message, Does.Contain("lease was lost"));
        await using var verification = test.Database.CreateContext();
        Assert.That((await verification.BillingWebhookEvents.SingleAsync()).ProcessingLeaseId, Is.EqualTo(newOwner));
    }

    [Test]
    public async Task ReleaseFailurePreservesOriginalProviderFailure()
    {
        var failure = new InvalidOperationException("provider failed");
        var provider = new RecordingSubscriptionProvider { Failure = failure };
        await using var test = await CreateAsync(provider);
        var id = await ReceiveAsync(test);
        provider.BeforeReturn = async () =>
        {
            await test.Database.DisposeAsync();
        };
        var exception = Assert.ThrowsAsync<AggregateException>(async () => await test.Service.ProcessAsync(id));
        Assert.That(exception!.InnerExceptions[0], Is.SameAs(failure));
        Assert.That(exception.InnerExceptions, Has.Count.EqualTo(2));
    }

    [TestCase(false)]
    [TestCase(true)]
    public async Task MissingOrBusyClaimDoesNotCallProvider(bool busy)
    {
        var provider = new RecordingSubscriptionProvider();
        await using var test = await CreateAsync(provider);
        var id = busy ? await ReceiveAsync(test) : Guid.NewGuid();
        if (busy)
        {
            await test.Services.GetRequiredService<PostgresBillingWebhookProcessingStore>()
                .TryAcquireAsync(id, Guid.CreateVersion7(), ObservedAt, ObservedAt.AddMinutes(5), CancellationToken.None);
        }
        var result = await test.Service.ProcessAsync(id);
        Assert.That(result.Status, Is.EqualTo(busy ? BillingWebhookProcessingStatus.Busy : BillingWebhookProcessingStatus.NotFound));
        Assert.That(provider.Calls, Is.Zero);
    }

    private static Task<ServiceTestBase<BillingWebhookProcessingService>> CreateAsync(RecordingSubscriptionProvider provider)
    {
        return WorkerServiceTestBase.CreateAsync<BillingWebhookProcessingService>(ObservedAt, services =>
        {
            services.RemoveAll<ICommercialSubscriptionProvider>();
            services.AddSingleton<ICommercialSubscriptionProvider>(provider);
        });
    }

    private static async Task<Guid> ReceiveAsync(ServiceTestBase<BillingWebhookProcessingService> test)
    {
        await using var inbox = ServiceTestBase<PostgresBillingWebhookInboxStore>.ForDatabase(test.Database, ObservedAt);
        var result = await inbox.Service.ReceiveAsync(
            new VerifiedBillingWebhookEvent("evt_process", "invoice.paid", BillingWebhookEventKind.InvoicePaid,
                ObservedAt.AddMinutes(-1)), ObservedAt, BillingWebhookInboxDisposition.PendingProcessing, CancellationToken.None);
        return result.InboxEventId;
    }

    private static AuthoritativeCommercialSubscription Snapshot(Guid billingAccountId)
    {
        return new(billingAccountId, new CommercialSubscriptionProjection(
            "cus_process", "sub_process", "price_process", CommercialSubscriptionStatus.Active,
            3, false, ObservedAt.AddDays(-1), ObservedAt.AddMonths(1), ObservedAt));
    }

    private sealed class RecordingSubscriptionProvider : ICommercialSubscriptionProvider
    {
        public AuthoritativeCommercialSubscription? Result { get; set; }
        public Exception? Failure { get; set; }
        public Func<Task>? BeforeReturn { get; set; }
        public int Calls { get; private set; }
        public string? ExternalEventId { get; private set; }
        public BillingWebhookEventKind? Kind { get; private set; }

        public async Task<AuthoritativeCommercialSubscription?> ResolveEventAsync(
            string externalEventId, BillingWebhookEventKind kind, CancellationToken cancellationToken)
        {
            Calls++;
            ExternalEventId = externalEventId;
            Kind = kind;
            if (BeforeReturn is not null)
            {
                await BeforeReturn();
            }
            if (Failure is not null)
            {
                throw Failure;
            }
            return Result;
        }

        public Task<AuthoritativeCommercialSubscription> ResolveCheckoutSubscriptionAsync(
            string externalSubscriptionId, Guid expectedBillingOperationId, CancellationToken cancellationToken)
        {
            throw new AssertionException("Webhook processing must resolve subscriptions through their event.");
        }
    }
}
