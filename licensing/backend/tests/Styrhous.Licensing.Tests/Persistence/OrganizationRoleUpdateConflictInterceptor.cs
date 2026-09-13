using System.Data.Common;
using Microsoft.EntityFrameworkCore.Diagnostics;

namespace Styrhous.Licensing.Tests.Persistence;

internal sealed class OrganizationRoleUpdateConflictInterceptor(
    bool suppressEveryUpdate = true) : DbCommandInterceptor
{

    public int UpdateCount { get; private set; }

    public int SuppressedUpdateCount { get; private set; }

    public override ValueTask<InterceptionResult<int>> NonQueryExecutingAsync(
        DbCommand command,
        CommandEventData eventData,
        InterceptionResult<int> result,
        CancellationToken cancellationToken = default)
    {
        if (!command.CommandText.StartsWith(
                "UPDATE organization_memberships",
                StringComparison.OrdinalIgnoreCase))
        {
            return ValueTask.FromResult(result);
        }

        UpdateCount++;
        if (suppressEveryUpdate || UpdateCount % 2 == 0)
        {
            SuppressedUpdateCount++;
            return ValueTask.FromResult(InterceptionResult<int>.SuppressWithResult(0));
        }

        return ValueTask.FromResult(result);
    }
}
