using Rebus.Activation;
using Rebus.Config;
using Rebus.PostgreSql;
using Rebus.Routing.TypeBased;
using Styrhous.Licensing.Tests.Persistence;

namespace Styrhous.Licensing.Tests.Infrastructure;

[TestFixture]
[Parallelizable(ParallelScope.All)]
public sealed class PostgresRebusTransportIntegrationTests
{
    [Test]
    public async Task SendsAndReceivesDurableWorkThroughPostgres()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var queueName = $"rebus-{Guid.CreateVersion7():N}";
        var expected = Guid.CreateVersion7();
        var received = new TaskCompletionSource<Guid>(
            TaskCreationOptions.RunContinuationsAsynchronously);
        using var activator = new BuiltinHandlerActivator();
        activator.Handle<TransportProbe>(message =>
        {
            received.TrySetResult(message.Id);
            return Task.CompletedTask;
        });
        using var bus = Configure.With(activator)
            .Transport(transport => transport.UsePostgreSql(
                database.ConnectionString,
                "RebusMessages",
                queueName))
            .Routing(routing => routing.TypeBased().Map<TransportProbe>(queueName))
            .Start();

        await bus.Send(new TransportProbe(expected));

        using var timeout = new CancellationTokenSource(TimeSpan.FromSeconds(10));
        Assert.That(
            await received.Task.WaitAsync(timeout.Token),
            Is.EqualTo(expected));
    }

    private sealed record TransportProbe(Guid Id);
}
