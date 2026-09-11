namespace Styrhous.Licensing.Tests.Persistence;

internal sealed class MutableTimeProvider(DateTimeOffset utcNow) : TimeProvider
{
    public DateTimeOffset UtcNow { get; set; } = utcNow.ToUniversalTime();

    public override DateTimeOffset GetUtcNow()
    {
        return UtcNow;
    }
}
