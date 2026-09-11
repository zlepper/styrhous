using Styrhous.Licensing.Domain.Identifiers;

namespace Styrhous.Licensing.Tests.Domain;

[TestFixture]
public sealed class Uuid7Tests
{
    [Test]
    public void GeneratedIdentifiersUseVersionSevenAndTheRfcVariant()
    {
        var identifier = Uuid7.Create();
        Assert.Multiple(() =>
        {
            Assert.That(identifier.Version, Is.EqualTo(7));
            Assert.That(identifier.Variant, Is.InRange(8, 11));
            Assert.That(identifier, Is.Not.EqualTo(Uuid7.Create()));
        });
    }
}
