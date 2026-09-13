using System.Data.Common;
using Microsoft.EntityFrameworkCore;
using Microsoft.EntityFrameworkCore.Diagnostics;

namespace Styrhous.Licensing.Tests.Persistence;

internal sealed class OneTimeCommitFailureInterceptor : DbTransactionInterceptor
{
    private int _failNextCommit;

    public void FailNextCommit()
    {
        Interlocked.Exchange(ref _failNextCommit, 1);
    }

    public override ValueTask<InterceptionResult> TransactionCommittingAsync(
        DbTransaction transaction,
        TransactionEventData eventData,
        InterceptionResult result,
        CancellationToken cancellationToken = default)
    {
        if (Interlocked.Exchange(ref _failNextCommit, 0) == 1)
        {
            throw new DbUpdateConcurrencyException(
                "Injected transaction commit concurrency failure.");
        }

        return base.TransactionCommittingAsync(
            transaction,
            eventData,
            result,
            cancellationToken);
    }
}
