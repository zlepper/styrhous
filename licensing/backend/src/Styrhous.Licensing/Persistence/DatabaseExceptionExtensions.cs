using Npgsql;

namespace Styrhous.Licensing.Persistence;

internal static class DatabaseExceptionExtensions
{
    public static bool IsUniqueViolation(this Exception exception, string? constraintName = null)
    {
        for (Exception? current = exception; current is not null; current = current.InnerException)
        {
            if (current is PostgresException { SqlState: PostgresErrorCodes.UniqueViolation } postgres)
            {
                return constraintName is null
                    || string.Equals(postgres.ConstraintName, constraintName, StringComparison.Ordinal);
            }
        }

        return false;
    }
}
