namespace Styrhous.Licensing.Persistence;

internal static class EfConcurrencyRetry
{
    private const int MaximumAttempts = 3;

    public static async Task<T> ExecuteAsync<T>(Func<Task<T>> operation)
    {
        ArgumentNullException.ThrowIfNull(operation);
        for (var attempt = 1; ; attempt++)
        {
            try
            {
                return await operation();
            }
            catch (Exception exception) when (
                attempt < MaximumAttempts
                && EfConcurrencyFailure.IsRetryable(exception))
            {
                // Retry the complete operation so it observes fresh database state.
            }
        }
    }
}
