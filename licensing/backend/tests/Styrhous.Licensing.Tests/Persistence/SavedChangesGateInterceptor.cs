using Microsoft.EntityFrameworkCore.Diagnostics;

namespace Styrhous.Licensing.Tests.Persistence;

internal sealed class SavedChangesGateInterceptor(DatabaseCommandGate gate)
    : SaveChangesInterceptor
{

    public bool CancellationObserved { get; private set; }

    public override async ValueTask<int> SavedChangesAsync(
        SaveChangesCompletedEventData eventData,
        int result,
        CancellationToken cancellationToken = default)
    {
        try
        {
            await gate.SignalAndWaitAsync(cancellationToken);
        }
        catch (OperationCanceledException)
        {
            CancellationObserved = true;
            throw;
        }

        return result;
    }
}
