using System.Data.Common;
using Microsoft.EntityFrameworkCore;
using Microsoft.EntityFrameworkCore.Diagnostics;

namespace Styrhous.Licensing.Tests.Persistence;

internal sealed class OneTimeCommandConcurrencyFailureInterceptor(
    string commandFragment) : DbCommandInterceptor
{
    private readonly string _commandFragment =
        string.IsNullOrWhiteSpace(commandFragment)
            ? throw new ArgumentException(
                "A database command fragment is required.",
                nameof(commandFragment))
            : commandFragment;
    private int _matchingCommandCount;

    public int MatchingCommandCount => Volatile.Read(ref _matchingCommandCount);

    public override ValueTask<InterceptionResult<int>> NonQueryExecutingAsync(
        DbCommand command,
        CommandEventData eventData,
        InterceptionResult<int> result,
        CancellationToken cancellationToken = default)
    {
        if (command.CommandText.Contains(
                _commandFragment,
                StringComparison.OrdinalIgnoreCase)
            && Interlocked.Increment(ref _matchingCommandCount) == 1)
        {
            throw new DbUpdateConcurrencyException(
                "Injected one-time command concurrency failure.");
        }

        return base.NonQueryExecutingAsync(command, eventData, result, cancellationToken);
    }
}
