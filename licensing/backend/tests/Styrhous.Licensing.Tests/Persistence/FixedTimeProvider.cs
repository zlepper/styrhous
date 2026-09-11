namespace Styrhous.Licensing.Tests.Persistence;

internal sealed class FixedTimeProvider(DateTimeOffset utcNow) : TimeProvider
{
    private readonly DateTimeOffset _utcNow = utcNow.ToUniversalTime();

    public override DateTimeOffset GetUtcNow()
    {
        return _utcNow;
    }
}
