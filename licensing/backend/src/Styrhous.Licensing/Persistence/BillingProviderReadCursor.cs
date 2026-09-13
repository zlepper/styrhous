namespace Styrhous.Licensing.Persistence;

internal sealed class BillingProviderReadCursor
{
    public static readonly Guid SingletonId =
        Guid.Parse("01a05c8d-9f83-74b3-9197-d3087eab0559");

    private BillingProviderReadCursor()
    {
    }

    public Guid Id { get; private set; }

    public long Revision { get; private set; }

    public uint Version { get; private set; }

    public long Advance()
    {
        Revision = checked(Revision + 1);
        return Revision;
    }
}
