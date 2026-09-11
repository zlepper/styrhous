using System.Data.Common;
using Microsoft.EntityFrameworkCore.Diagnostics;

namespace Styrhous.Licensing.Tests.Persistence;

internal sealed class OrganizationRoleUpdateBarrierInterceptor(DatabaseCommandBarrier barrier)
    : DbCommandInterceptor
{
    private bool _hasWaited;

    public override async ValueTask<InterceptionResult<int>> NonQueryExecutingAsync(
        DbCommand command,
        CommandEventData eventData,
        InterceptionResult<int> result,
        CancellationToken cancellationToken = default)
    {
        if (!_hasWaited
            && command.CommandText.StartsWith("UPDATE organization_memberships",
                StringComparison.OrdinalIgnoreCase))
        {
            _hasWaited = true;
            await barrier.SignalAndWaitAsync(cancellationToken);
        }

        return result;
    }
}
