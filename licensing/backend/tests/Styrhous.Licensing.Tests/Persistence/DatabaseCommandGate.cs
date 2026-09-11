namespace Styrhous.Licensing.Tests.Persistence;

internal sealed class DatabaseCommandGate
{
    private readonly TaskCompletionSource _commandReached =
        new(TaskCreationOptions.RunContinuationsAsynchronously);
    private readonly TaskCompletionSource _release =
        new(TaskCreationOptions.RunContinuationsAsynchronously);

    public async Task SignalAndWaitAsync(CancellationToken cancellationToken)
    {
        _commandReached.TrySetResult();
        try
        {
            await _release.Task.WaitAsync(TimeSpan.FromSeconds(10), cancellationToken);
        }
        catch (TimeoutException exception)
        {
            throw new TimeoutException(
                "The database command gate was not released by the test.",
                exception);
        }
    }

    public async Task WaitUntilReachedAsync(CancellationToken cancellationToken = default)
    {
        try
        {
            await _commandReached.Task.WaitAsync(TimeSpan.FromSeconds(10), cancellationToken);
        }
        catch (TimeoutException exception)
        {
            throw new TimeoutException(
                "The expected database command did not reach the test gate.",
                exception);
        }
    }

    public void Release()
    {
        _release.TrySetResult();
    }
}
