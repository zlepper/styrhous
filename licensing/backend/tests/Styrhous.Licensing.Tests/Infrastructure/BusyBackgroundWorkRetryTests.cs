using Rebus.Handlers;
using Microsoft.Extensions.DependencyInjection;
using Microsoft.Extensions.DependencyInjection.Extensions;
using Microsoft.EntityFrameworkCore;
using Rebus.Activation;
using Rebus.Bus;
using Rebus.Config;
using Rebus.PostgreSql;
using Rebus.Serialization.Json;
using Styrhous.Licensing.Application.Messaging;
using Styrhous.Licensing.Infrastructure.Messaging;
using Styrhous.Licensing.Persistence;
using Styrhous.Licensing.Tests.Persistence;

namespace Styrhous.Licensing.Tests.Infrastructure;

[TestFixture]
[NonParallelizable]
public sealed class BusyBackgroundWorkRetryTests
{
    [TestCase(false)]
    [TestCase(true)]
    public async Task BrokerRedeliveryWaitsForAnAbandonedLeaseAndProcessesWithoutAnotherPublication(
        bool restartWhileBusy)
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var outboxMessageId = await BackgroundWorkTestScenario.CreateInvitationOutboxAsync(
            database, "abandoned-lease");
        var clock = new MutableClock(LicensingPersistenceScenario.SignupTime.AddDays(3));
        await using (var context = database.CreateContext())
        {
            var message = await context.OutboxMessages.SingleAsync(item => item.Id == outboxMessageId);
            Assert.That(message.TryAcquireProcessingLease(
                Guid.CreateVersion7(), clock.GetUtcNow(), clock.GetUtcNow().AddMinutes(5)), Is.True);
            await context.SaveChangesAsync();
        }

        var retryReached = new DatabaseCommandGate();
        var sender = new RecordingEmailSender();
        await using var test = WorkerServiceTestBase.ForDatabase<OrganizationInvitationDeliveryService>(
            database, clock.GetUtcNow(), services =>
            {
                services.RemoveAll<TimeProvider>();
                services.AddSingleton<TimeProvider>(clock);
                services.RemoveAll<IOrganizationInvitationEmailSender>();
                services.AddSingleton<IOrganizationInvitationEmailSender>(sender);
            }, interceptors: [new DatabaseCommandGateInterceptor(
                retryReached, "FROM outbox_messages", matchingOccurrence: 2)]);
        var handler = test.Services.GetRequiredService<IHandleMessages<OrganizationInvitationDeliveryMessage>>();
        var handled = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
        var queueName = $"styrhous-it-{Guid.CreateVersion7():N}";
        using var activator = CreateActivator(handler, handled);
        using var bus = StartReceiver(activator, database.ConnectionString, queueName);
        await bus.SendLocal(new OrganizationInvitationDeliveryMessage(outboxMessageId));

        var retried = retryReached.WaitUntilReachedAsync();
        try
        {
            Assert.That(await Task.WhenAny(retried, handled.Task).WaitAsync(TimeSpan.FromSeconds(10)),
                Is.SameAs(retried), "A busy redelivery must remain unacknowledged and retry acquisition.");
            await retried;
            Assert.That(sender.Attempts, Is.Zero);
            if (!restartWhileBusy)
            {
                clock.Advance(TimeSpan.FromMinutes(6));
            }
        }
        finally
        {
            retryReached.Release();
        }

        if (restartWhileBusy)
        {
            bus.Dispose();
            Assert.That(handled.Task.IsCompleted, Is.False,
                "Shutdown must cancel the busy handler without acknowledging its message.");
            clock.Advance(TimeSpan.FromMinutes(6));
            using var restartedActivator = CreateActivator(handler, handled);
            using var restarted = StartReceiver(
                restartedActivator,
                database.ConnectionString,
                queueName);
            await handled.Task.WaitAsync(TimeSpan.FromSeconds(10));
        }
        else
        {
            await handled.Task.WaitAsync(TimeSpan.FromSeconds(10));
        }

        await using var verification = database.CreateContext();
        var delivered = await verification.OutboxMessages.SingleAsync(message => message.Id == outboxMessageId);
        Assert.Multiple(() =>
        {
            Assert.That(delivered.DeliveredAt, Is.EqualTo(clock.GetUtcNow()));
            Assert.That(delivered.ProcessingAttemptCount, Is.EqualTo(2));
            Assert.That(sender.Attempts, Is.EqualTo(1));
        });
    }

    private static BuiltinHandlerActivator CreateActivator(
        IHandleMessages<OrganizationInvitationDeliveryMessage> handler, TaskCompletionSource handled)
    {
        var activator = new BuiltinHandlerActivator();
        activator.Handle<OrganizationInvitationDeliveryMessage>(async message =>
        {
            await handler.Handle(message);
            handled.TrySetResult();
        });
        return activator;
    }

    private static IBus StartReceiver(
        BuiltinHandlerActivator activator,
        string connectionString,
        string queueName)
    {
        return Configure.With(activator)
            .Transport(transport => transport.UsePostgreSql(
                connectionString,
                "RebusMessages",
                queueName))
            .Serialization(serializer => serializer.UseSystemTextJson())
            .Start();
    }

    private sealed class MutableClock(DateTimeOffset initialTime) : TimeProvider
    {
        private long _utcTicks = initialTime.UtcTicks;

        public override DateTimeOffset GetUtcNow()
        {
            return new DateTimeOffset(Interlocked.Read(ref _utcTicks), TimeSpan.Zero);
        }

        public void Advance(TimeSpan elapsed)
        {
            Interlocked.Add(ref _utcTicks, elapsed.Ticks);
        }
    }

    private sealed class RecordingEmailSender : IOrganizationInvitationEmailSender
    {
        private int _attempts;
        public int Attempts => Volatile.Read(ref _attempts);

        public Task SendAsync(Guid outboxMessageId, OrganizationInvitationDelivery delivery,
            CancellationToken cancellationToken)
        {
            cancellationToken.ThrowIfCancellationRequested();
            Interlocked.Increment(ref _attempts);
            return Task.CompletedTask;
        }
    }
}
