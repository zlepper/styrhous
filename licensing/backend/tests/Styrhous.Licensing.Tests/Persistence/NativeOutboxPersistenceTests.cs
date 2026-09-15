using Styrhous.Licensing.Domain.Billing;
using System.Text.Json;
using Microsoft.EntityFrameworkCore;
using Microsoft.Extensions.DependencyInjection;
using Microsoft.Extensions.DependencyInjection.Extensions;
using Rebus.Messages;
using Styrhous.Licensing.Application.Messaging;
using Styrhous.Licensing.Domain.Messaging;
using Styrhous.Licensing.Infrastructure.Messaging;
using Styrhous.Licensing.Persistence;

namespace Styrhous.Licensing.Tests.Persistence;

[TestFixture]
[Parallelizable(ParallelScope.All)]
public sealed class NativeOutboxPersistenceTests
{
    private static readonly DateTimeOffset ObservedAt = new(2026, 9, 11, 12, 0, 0, TimeSpan.Zero);

    [TestCase(BackgroundWorkKind.OrganizationInvitationDelivery, typeof(OrganizationInvitationDeliveryMessage), true)]
    [TestCase(BackgroundWorkKind.OrganizationInvitationDelivery, typeof(OrganizationInvitationDeliveryMessage), false)]
    [TestCase(BackgroundWorkKind.InfrastructureSmokeProbe, typeof(InfrastructureSmokeProbeMessage), true)]
    [TestCase(BackgroundWorkKind.InfrastructureSmokeProbe, typeof(InfrastructureSmokeProbeMessage), false)]
    [TestCase(BackgroundWorkKind.BillingWebhook, typeof(BillingWebhookProcessingMessage), true)]
    [TestCase(BackgroundWorkKind.BillingWebhook, typeof(BillingWebhookProcessingMessage), false)]
    public async Task NativeMessageAndBusinessWorkCommitOrRollBackTogether(
        BackgroundWorkKind kind, Type expectedContract, bool commit)
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        await using var fixture = ServiceTestBase<PostgresBackgroundWorkOutbox>.ForDatabaseWithBackgroundQueue(database, ObservedAt, "outbox-test");
        Guid workId;
        await using (var context = database.CreateContext())
        {
            await using var transaction = await context.Database.BeginTransactionAsync();
            BackgroundWorkReference work;
            if (kind == BackgroundWorkKind.BillingWebhook)
            {
                var webhook = BillingWebhookEvent.Receive("evt_native_contract", "invoice.paid",
                    BillingWebhookEventKind.InvoicePaid, ObservedAt, ObservedAt);
                context.BillingWebhookEvents.Add(webhook);
                workId = webhook.Id;
                work = BackgroundWorkReference.BillingWebhook(workId, ObservedAt);
            }
            else
            {
                var message = kind == BackgroundWorkKind.InfrastructureSmokeProbe
                    ? OutboxMessage.Enqueue(Guid.CreateVersion7(), Guid.CreateVersion7(),
                        OutboxMessageTypes.InfrastructureSmokeProbe, "probe", ObservedAt)
                    : PendingDelivery(ObservedAt);
                context.OutboxMessages.Add(message);
                workId = message.Id;
                work = kind == BackgroundWorkKind.InfrastructureSmokeProbe
                    ? BackgroundWorkReference.InfrastructureSmokeProbe(workId, ObservedAt)
                    : BackgroundWorkReference.OrganizationInvitationDelivery(workId, ObservedAt);
            }
            await fixture.Service.EnqueueAsync(context, work, default);
            await context.SaveChangesAsync();
            if (commit)
            {
                await transaction.CommitAsync();
            }
        }

        await using var verification = database.CreateContext();
        var workCount = kind == BackgroundWorkKind.BillingWebhook
            ? await verification.BillingWebhookEvents.CountAsync()
            : await verification.OutboxMessages.CountAsync();
        Assert.That(workCount, Is.EqualTo(commit ? 1 : 0));
        var nativeMessages = await verification.Set<RebusOutboxMessage>().ToArrayAsync();
        Assert.That(nativeMessages, Has.Length.EqualTo(commit ? 1 : 0));
        if (commit)
        {
            var native = nativeMessages.Single();
            var headers = JsonSerializer.Deserialize<Dictionary<string, string>>(native.Headers!)!;
            using var payload = JsonDocument.Parse(native.Body!);
            var enqueued = kind == BackgroundWorkKind.BillingWebhook
                ? (await verification.BillingWebhookEvents.SingleAsync()).NativeOutboxEnqueued
                : (await verification.OutboxMessages.SingleAsync()).NativeOutboxEnqueued;
            Assert.Multiple(() =>
            {
                Assert.That(native.DestinationAddress, Is.EqualTo("outbox-test"));
                Assert.That(headers[Headers.ContentType], Is.EqualTo("application/json;charset=utf-8"));
                Assert.That(headers[Headers.Type], Is.EqualTo(expectedContract.AssemblyQualifiedName));
                Assert.That(payload.RootElement.GetProperty("WorkId").GetGuid(), Is.EqualTo(workId));
                Assert.That(enqueued, Is.True);
            });
        }
    }

    [Test]
    public async Task CancelledEnqueueCannotLeaveACommittedBusinessRecord()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        await using var fixture = ServiceTestBase<PostgresBackgroundWorkOutbox>.ForDatabaseWithBackgroundQueue(database, ObservedAt, "outbox-test");
        await using (var context = database.CreateContext())
        {
            await using var transaction = await context.Database.BeginTransactionAsync();
            var message = PendingDelivery(ObservedAt);
            context.OutboxMessages.Add(message);
            using var cancelled = new CancellationTokenSource();
            cancelled.Cancel();
            Assert.That(async () => await fixture.Service.EnqueueAsync(
                context, BackgroundWorkReference.OrganizationInvitationDelivery(message.Id, ObservedAt), cancelled.Token),
                Throws.InstanceOf<OperationCanceledException>());
        }
        await using var verification = database.CreateContext();
        Assert.Multiple(() =>
        {
            Assert.That(verification.OutboxMessages.Count(), Is.Zero);
            Assert.That(verification.Set<RebusOutboxMessage>().Count(), Is.Zero);
        });
    }

    [Test]
    public async Task LegacyTransitionIsIdempotentAndExcludesExpiredOrSupersededInvitations()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var pending = PendingDelivery(ObservedAt);
        var expired = PendingDelivery(ObservedAt.AddDays(-1));
        var discarded = PendingDelivery(ObservedAt);
        discarded.TryDiscard(OutboxDiscardReason.Superseded, ObservedAt);
        await using (var context = database.CreateContext())
        {
            context.OutboxMessages.AddRange(pending, expired, discarded);
            await context.SaveChangesAsync();
        }
        await using var fixture = ServiceTestBase<NativeOutboxUpgrade>.ForDatabaseWithBackgroundQueue(database, ObservedAt, "outbox-test");
        await fixture.Service.EnqueueLegacyWorkAsync(default);
        await fixture.Service.EnqueueLegacyWorkAsync(default);

        await using var verification = database.CreateContext();
        Assert.That(await verification.Set<RebusOutboxMessage>().CountAsync(), Is.EqualTo(1));
        var work = await verification.OutboxMessages.ToDictionaryAsync(message => message.Id);
        Assert.Multiple(() =>
        {
            Assert.That(work[pending.Id].NativeOutboxEnqueued, Is.True);
            Assert.That(work[expired.Id].NativeOutboxEnqueued, Is.False);
            Assert.That(work[discarded.Id].NativeOutboxEnqueued, Is.False);
        });
    }

    [Test]
    public async Task StoppedForwarderLeavesWorkDurableWhenMaintenanceIsCancelled()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        await using var fixture = ServiceTestBase<BackgroundWorkRecoveryService>.ForDatabaseWithBackgroundQueue(database, ObservedAt, "outbox-test");
        var message = PendingDelivery(ObservedAt);
        await using (var context = database.CreateContext())
        {
            context.OutboxMessages.Add(message);
            await context.SaveChangesAsync();
        }
        // Complete the upgrade before cancelling so this checks durable publication state.
        await fixture.Services.GetRequiredService<NativeOutboxUpgrade>().EnqueueLegacyWorkAsync(default);
        using var cancelled = new CancellationTokenSource();
        cancelled.Cancel();
        Assert.That(async () => await fixture.Service
            .RecoverAsync(cancelled.Token), Throws.InstanceOf<OperationCanceledException>());
        await using var verification = database.CreateContext();
        Assert.That(await verification.Set<RebusOutboxMessage>().CountAsync(), Is.EqualTo(1));
    }

    private static OutboxMessage PendingDelivery(DateTimeOffset occurredAt)
    {
        return OutboxMessage.Enqueue(Guid.CreateVersion7(), Guid.CreateVersion7(),
            OutboxMessageTypes.OrganizationInvitationDelivery, "protected-payload",
            occurredAt, occurredAt.AddHours(1));
    }
}
