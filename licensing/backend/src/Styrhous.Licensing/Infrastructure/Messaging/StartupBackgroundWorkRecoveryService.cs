using Styrhous.Licensing.Application.Messaging;

namespace Styrhous.Licensing.Infrastructure.Messaging;

internal sealed partial class StartupBackgroundWorkRecoveryService(
    IServiceScopeFactory scopeFactory,
    ILogger<StartupBackgroundWorkRecoveryService> logger)
    : IHostedService
{
    public async Task StartAsync(CancellationToken cancellationToken)
    {
        await using var scope = scopeFactory.CreateAsyncScope();
        var result = await scope.ServiceProvider
            .GetRequiredService<BackgroundWorkRecoveryService>()
            .RecoverAsync(cancellationToken);
        LogRecoveryCompleted(logger, result.OutboxDrained);
    }

    public Task StopAsync(CancellationToken cancellationToken)
    {
        return Task.CompletedTask;
    }

    [LoggerMessage(
        EventId = 3205,
        Level = LogLevel.Information,
        Message = "Startup background-work recovery completed; native outbox drained: {OutboxDrained}.")]
    private static partial void LogRecoveryCompleted(
        ILogger logger,
        bool outboxDrained);
}
