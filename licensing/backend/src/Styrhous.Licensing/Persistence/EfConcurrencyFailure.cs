using Microsoft.EntityFrameworkCore;

namespace Styrhous.Licensing.Persistence;

internal static class EfConcurrencyFailure
{
    public static bool IsRetryable(Exception exception)
    {
        return exception is DbUpdateConcurrencyException;
    }
}
