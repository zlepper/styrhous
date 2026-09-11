using System.Data.Common;
using Microsoft.EntityFrameworkCore.Diagnostics;

namespace Styrhous.Licensing.Tests.Persistence;

internal sealed class DatabaseCommandGateInterceptor(
    DatabaseCommandGate gate,
    string commandFragment,
    DatabaseCommandInterceptionPhase phase = DatabaseCommandInterceptionPhase.BeforeExecution,
    int matchingOccurrence = 1)
    : DbCommandInterceptor
{
    private readonly string _commandFragment =
        string.IsNullOrWhiteSpace(commandFragment)
            ? throw new ArgumentException(
                "A database command fragment is required.",
                nameof(commandFragment))
            : commandFragment;
    private readonly int _matchingOccurrence = matchingOccurrence > 0
        ? matchingOccurrence
        : throw new ArgumentOutOfRangeException(
            nameof(matchingOccurrence),
            "The matching occurrence must be positive.");
    private readonly TaskCompletionSource _cancellationObserved =
        new(TaskCreationOptions.RunContinuationsAsynchronously);
    private int _matchingCommandCount;

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

    public async Task WaitUntilCancellationObservedAsync(
        CancellationToken cancellationToken = default)
    {
        try
        {
            await _cancellationObserved.Task.WaitAsync(
                TimeSpan.FromSeconds(10),
                cancellationToken);
        }
        catch (TimeoutException exception)
        {
            throw new TimeoutException(
                "The expected database command did not observe cancellation.",
                exception);
        }
    }

    private async Task WaitIfMatchingAsync(
        DbCommand command,
        CancellationToken cancellationToken)
    {
        if (!command.CommandText.Contains(
                _commandFragment,
                StringComparison.OrdinalIgnoreCase)
            || Interlocked.Increment(ref _matchingCommandCount) != _matchingOccurrence)
        {
            return;
        }

        try
        {
            await gate.SignalAndWaitAsync(cancellationToken);
        }
        catch (OperationCanceledException) when (cancellationToken.IsCancellationRequested)
        {
            _cancellationObserved.TrySetResult();
            throw;
        }
    }
}
