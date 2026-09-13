using Microsoft.EntityFrameworkCore;
using Npgsql;

namespace Styrhous.Licensing.Persistence;

internal static class EfConcurrencyFailure
{
    public static bool IsRetryable(Exception exception)
    {
        for (Exception? current = exception; current is not null; current = current.InnerException)
        {
            if (current is DbUpdateConcurrencyException
                or PostgresException
                {
                    SqlState: PostgresErrorCodes.SerializationFailure
                        or PostgresErrorCodes.DeadlockDetected,
                })
            {
                return true;
            }
        }

        return false;
    }
}
