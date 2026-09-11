using Microsoft.EntityFrameworkCore.Diagnostics;

namespace Styrhous.Licensing.Tests.Persistence;

internal sealed class ThrowAfterSaveInterceptor : SaveChangesInterceptor
{
    public override ValueTask<int> SavedChangesAsync(
        SaveChangesCompletedEventData eventData,
        int result,
        CancellationToken cancellationToken = default)
    {
        throw new SimulatedPostSaveException();
    }
}

internal sealed class SimulatedPostSaveException : Exception
{
}
