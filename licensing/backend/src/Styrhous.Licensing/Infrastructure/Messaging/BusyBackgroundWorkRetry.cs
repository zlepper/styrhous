namespace Styrhous.Licensing.Infrastructure.Messaging;

internal static class BusyBackgroundWorkRetry
{
    private static readonly TimeSpan RetryInterval = TimeSpan.FromSeconds(1);

    public static async Task RunAsync(
        Func<CancellationToken, Task<bool>> processUntilTerminal,
        CancellationToken cancellationToken)
    {
        while (true)
        {
            cancellationToken.ThrowIfCancellationRequested();
            if (await processUntilTerminal(cancellationToken))
            {
                return;
            }

            // Keep the broker delivery unacknowledged while an existing lease is active.
            // A crashed worker cannot otherwise arrange a later delivery after lease expiry.
            await Task.Delay(RetryInterval, cancellationToken);
        }
    }
}
