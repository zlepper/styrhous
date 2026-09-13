using System.Data.Common;
using Microsoft.EntityFrameworkCore.Diagnostics;

namespace Styrhous.Licensing.Tests.Persistence;

internal enum DatabaseCommandInterceptionPhase
{
    BeforeExecution,
    AfterReaderExecution,
}

internal sealed class DatabaseCommandBarrierInterceptor(
    DatabaseCommandBarrier barrier,
    string commandFragment,
    DatabaseCommandInterceptionPhase phase = DatabaseCommandInterceptionPhase.BeforeExecution)
    : DbCommandInterceptor
{
    private readonly string _commandFragment =
        string.IsNullOrWhiteSpace(commandFragment)
            ? throw new ArgumentException(
                "A database command fragment is required.",
                nameof(commandFragment))
            : commandFragment;
    private int _hasWaited;

    public override async ValueTask<InterceptionResult<DbDataReader>> ReaderExecutingAsync(
        DbCommand command,
        CommandEventData eventData,
        InterceptionResult<DbDataReader> result,
        CancellationToken cancellationToken = default)
    {
        if (phase == DatabaseCommandInterceptionPhase.BeforeExecution)
        {
            await WaitIfMatchingAsync(command, cancellationToken);
        }

        return result;
    }

    public override async ValueTask<InterceptionResult<int>> NonQueryExecutingAsync(
        DbCommand command,
        CommandEventData eventData,
        InterceptionResult<int> result,
        CancellationToken cancellationToken = default)
    {
        if (phase == DatabaseCommandInterceptionPhase.BeforeExecution)
        {
            await WaitIfMatchingAsync(command, cancellationToken);
        }

        return result;
    }

    public override DbDataReader ReaderExecuted(
        DbCommand command,
        CommandExecutedEventData eventData,
        DbDataReader result)
    {
        if (phase == DatabaseCommandInterceptionPhase.AfterReaderExecution)
        {
            WaitIfMatchingAsync(command, CancellationToken.None).GetAwaiter().GetResult();
        }

        return result;
    }

    public override async ValueTask<DbDataReader> ReaderExecutedAsync(
        DbCommand command,
        CommandExecutedEventData eventData,
        DbDataReader result,
        CancellationToken cancellationToken = default)
    {
        if (phase == DatabaseCommandInterceptionPhase.AfterReaderExecution)
        {
            await WaitIfMatchingAsync(command, cancellationToken);
        }

        return result;
    }

    private async Task WaitIfMatchingAsync(
        DbCommand command,
        CancellationToken cancellationToken)
    {
        if (command.CommandText.Contains(
                _commandFragment,
                StringComparison.OrdinalIgnoreCase)
            && Interlocked.Exchange(ref _hasWaited, 1) == 0)
        {
            await barrier.SignalAndWaitAsync(cancellationToken);
        }
    }
}
