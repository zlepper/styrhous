using Microsoft.EntityFrameworkCore;
using Styrhous.Licensing.Persistence;
using Styrhous.Licensing.Operations;

namespace Styrhous.Licensing.Application.Messaging;

public sealed record BackgroundWorkRecoveryResult(bool OutboxDrained);

public sealed partial class BackgroundWorkRecoveryService(
    IDbContextFactory<LicensingDbContext> dbContextFactory,
    NativeOutboxUpgrade upgrade,
    TimeProvider timeProvider,
    ILogger<BackgroundWorkRecoveryService> logger)
{
    private static readonly TimeSpan MaximumDuration = TimeSpan.FromSeconds(45);

    public async Task<BackgroundWorkRecoveryResult> RecoverAsync(CancellationToken cancellationToken = default)
    {
        using var deadline = CancellationTokenSource.CreateLinkedTokenSource(cancellationToken);
        deadline.CancelAfter(MaximumDuration);
        try
        {
            await EfConcurrencyRetry.ExecuteAsync(async () =>
            {
                await upgrade.EnqueueLegacyWorkAsync(deadline.Token);
                return true;
            });
            await using (var context = await dbContextFactory.CreateDbContextAsync(deadline.Token))
            {
                var now = timeProvider.GetUtcNow();
                var oldest = await context.OutboxMessages.AsNoTracking()
                    .Where(message => message.DeliveredAt == null && message.DiscardedAt == null
                        && (message.NotAfter == null || message.NotAfter > now))
                    .MinAsync(message => (DateTimeOffset?)message.OccurredAt, deadline.Token);
                LogOldestOutboxAge(logger, oldest is null ? 0 : Math.Max(0, (now - oldest.Value).TotalSeconds));
            }
            while (true)
            {
                await using var dbContext = await dbContextFactory.CreateDbContextAsync(deadline.Token);
                if (!await dbContext.Set<RebusOutboxMessage>().AsNoTracking().AnyAsync(deadline.Token))
                {
                    return new BackgroundWorkRecoveryResult(OutboxDrained: true);
                }
                await Task.Delay(TimeSpan.FromMilliseconds(100), deadline.Token);
            }
        }
        catch (OperationCanceledException) when (!cancellationToken.IsCancellationRequested)
        {
            LogForwardingTimeout(logger);
            throw new TimeoutException("Native outbox forwarding did not finish within the maintenance deadline.");
        }
    }

    [LoggerMessage(EventId = LicensingOperationalMetrics.OldestOutboxAgeEventId,
        EventName = "OldestOutboxAgeObserved", Level = LogLevel.Information,
        Message = "Oldest pending outbox message age is {" + LicensingOperationalMetrics.OldestOutboxAgeStateName + "} seconds.")]
    private static partial void LogOldestOutboxAge(ILogger logger, double oldestOutboxAgeSeconds);

    [LoggerMessage(Level = LogLevel.Warning,
        Message = "Native outbox forwarding exceeded the maintenance deadline; pending work remains durable.")]
    private static partial void LogForwardingTimeout(ILogger logger);
}
