namespace Styrhous.Licensing.Tests.Persistence;

internal sealed class DatabaseCommandBarrier(int participantCount)
{
    private readonly TaskCompletionSource _participantsReady =
        new(TaskCreationOptions.RunContinuationsAsynchronously);
    private int _arrivedParticipants;

    public int ArrivedCount =>
        Volatile.Read(ref _arrivedParticipants);

    public async Task SignalAndWaitAsync(CancellationToken cancellationToken)
    {
        if (Interlocked.Increment(ref _arrivedParticipants) == participantCount)
        {
            _participantsReady.TrySetResult();
        }

        try
        {
            await _participantsReady.Task.WaitAsync(TimeSpan.FromSeconds(10), cancellationToken);
        }
        catch (TimeoutException exception)
        {
            throw new TimeoutException(
                $"Only {ArrivedCount} of {participantCount} database commands reached the test barrier.",
                exception);
        }
    }
}
