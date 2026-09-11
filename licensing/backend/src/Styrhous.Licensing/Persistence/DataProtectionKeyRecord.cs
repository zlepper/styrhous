using Styrhous.Licensing.Domain.Identifiers;

namespace Styrhous.Licensing.Persistence;

public sealed class DataProtectionKeyRecord
{
    public const int MaximumFriendlyNameLength = 256;

    public const int MaximumXmlLength = 65_536;

    private DataProtectionKeyRecord()
    {
    }

    private DataProtectionKeyRecord(Guid id, string friendlyName, string xml)
    {
        Id = id;
        FriendlyName = friendlyName;
        Xml = xml;
    }

    public Guid Id { get; private set; }

    public string FriendlyName { get; private set; } = string.Empty;

    public string Xml { get; private set; } = string.Empty;

    internal static DataProtectionKeyRecord Create(string friendlyName, string xml)
    {
        return new(Uuid7.Create(), friendlyName, xml);
    }
}
