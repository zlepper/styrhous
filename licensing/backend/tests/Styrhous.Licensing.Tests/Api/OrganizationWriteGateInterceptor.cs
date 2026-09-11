using System.Data.Common;
using Microsoft.EntityFrameworkCore.Diagnostics;
using Styrhous.Licensing.Tests.Persistence;

namespace Styrhous.Licensing.Tests.Api;

internal sealed class OrganizationWriteGateInterceptor(DatabaseCommandGate gate)
    : DbCommandInterceptor
{
    private int _hasWaited;

    public override async ValueTask<DbDataReader> ReaderExecutedAsync(
        DbCommand command,
        CommandExecutedEventData eventData,
        DbDataReader result,
        CancellationToken cancellationToken = default)
    {
        if (command.CommandText.Contains(
                "INSERT INTO organizations",
                StringComparison.OrdinalIgnoreCase)
            && Interlocked.Exchange(ref _hasWaited, 1) == 0)
        {
            await gate.SignalAndWaitAsync(cancellationToken);
        }

        return result;
    }
}
