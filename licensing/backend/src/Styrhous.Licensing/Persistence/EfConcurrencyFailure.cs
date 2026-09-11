using Microsoft.EntityFrameworkCore;
using Npgsql;

namespace Styrhous.Licensing.Persistence;

internal static class EfConcurrencyFailure
{
    public static bool IsRetryable(Exception exception)
    {
        return exception is DbUpdateConcurrencyException
            || exception is PostgresException
            {
                SqlState: PostgresErrorCodes.SerializationFailure
                    or PostgresErrorCodes.DeadlockDetected,
            }
            || exception is InvalidOperationException
            {
                InnerException: PostgresException
                {
                    SqlState: PostgresErrorCodes.SerializationFailure
                        or PostgresErrorCodes.DeadlockDetected,
                },
            };
    }
}
