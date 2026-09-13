using System.Data.Common;
using Microsoft.EntityFrameworkCore;
using Microsoft.EntityFrameworkCore.Diagnostics;

namespace Styrhous.Licensing.Tests.Persistence;

internal sealed class OrganizationMemberRemovalConflictInterceptor : DbCommandInterceptor
{
    public int SuppressedDeleteCount { get; private set; }

    public override ValueTask<InterceptionResult<int>> NonQueryExecutingAsync(
        DbCommand command,
        CommandEventData eventData,
        InterceptionResult<int> result,
        CancellationToken cancellationToken = default)
    {
        if (!command.CommandText.StartsWith(
                "DELETE FROM organization_memberships",
                StringComparison.OrdinalIgnoreCase))
        {
            return ValueTask.FromResult(result);
        }

        SuppressedDeleteCount++;
        return ValueTask.FromResult(InterceptionResult<int>.SuppressWithResult(0));
    }
}

internal sealed class OrganizationMemberRemovalSerializationFailureInterceptor
    : DbCommandInterceptor
{
    public int FailureCount { get; private set; }

    public override ValueTask<InterceptionResult<int>> NonQueryExecutingAsync(
        DbCommand command,
        CommandEventData eventData,
        InterceptionResult<int> result,
        CancellationToken cancellationToken = default)
    {
        if (!command.CommandText.StartsWith(
                "DELETE FROM organization_memberships",
                StringComparison.OrdinalIgnoreCase))
        {
            return ValueTask.FromResult(result);
        }

        FailureCount++;
        throw new SimulatedSerializationFailureException();
    }

    private sealed class SimulatedSerializationFailureException : DbUpdateConcurrencyException
    {
    }
}
