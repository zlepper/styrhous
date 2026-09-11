using System.Data.Common;
using Microsoft.EntityFrameworkCore;

namespace Styrhous.Licensing.Persistence;

internal static class EfConcurrencyFailure
{
    private const string DeadlockDetectedSqlState = "40P01";
    private const string SerializationFailureSqlState = "40001";

    public static bool IsRetryable(Exception exception)
    {
        return exception is DbUpdateConcurrencyException
        || HasSqlState(
            exception,
            SerializationFailureSqlState,
            DeadlockDetectedSqlState);
    }

    public static bool HasSqlState(
        Exception exception,
        params string[] sqlStates)
    {
        return exception is DbException databaseException
            && sqlStates.Contains(databaseException.SqlState, StringComparer.Ordinal)
        || exception.InnerException is not null
            && HasSqlState(exception.InnerException, sqlStates);
    }
}
