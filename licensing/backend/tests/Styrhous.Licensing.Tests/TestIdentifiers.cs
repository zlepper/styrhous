namespace Styrhous.Licensing.Tests;

internal static class TestIdentifiers
{
    public const string MalformedText = "not-a-uuid";

    public const string Version7WithNonRfcVariantText =
        "018f0000-0000-7000-0000-000000000001";

    public static Guid Version7WithNonRfcVariant { get; } =
        Guid.Parse(Version7WithNonRfcVariantText);

    public static IReadOnlyList<Guid> ExistingIdentifierFormats { get; } =
    [
        Guid.Empty,
        Guid.Parse("36c1e76c-8841-44f9-8864-6e7ffe632ef1"),
        Version7WithNonRfcVariant,
    ];
}
