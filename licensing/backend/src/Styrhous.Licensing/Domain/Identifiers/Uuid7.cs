namespace Styrhous.Licensing.Domain.Identifiers;

public static class Uuid7
{
    public static Guid Create()
    {
        return Guid.CreateVersion7();
    }
}
