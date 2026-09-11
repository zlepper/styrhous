using System.Security.Cryptography;
using System.Text;
using Styrhous.Licensing.Infrastructure.Organizations;

namespace Styrhous.Licensing.Tests.Infrastructure;

[TestFixture]
[Parallelizable(ParallelScope.All)]
public sealed class CryptographicOrganizationInvitationSecretGeneratorTests
{
    [Test]
    public void GeneratorReturnsDistinctBase64UrlSecretsAndTheirSha256Hashes()
    {
        var generator = new CryptographicOrganizationInvitationSecretGenerator();

        var first = generator.Generate();
        var second = generator.Generate();
        var firstValue = first.Reveal();
        var secondValue = second.Reveal();

        Assert.Multiple(() =>
        {
            Assert.That(firstValue, Has.Length.EqualTo(43));
            Assert.That(firstValue, Does.Match("^[A-Za-z0-9_-]+$"));
            Assert.That(firstValue, Is.Not.EqualTo(secondValue));
            Assert.That(first.Hash, Is.EqualTo(Hash(firstValue)));
            Assert.That(second.Hash, Is.EqualTo(Hash(secondValue)));
            Assert.That(first.Hash, Has.Length.EqualTo(64));
            Assert.That(first.Hash, Does.Match("^[0-9a-f]+$"));
            Assert.That(first.ToString(), Is.EqualTo("[REDACTED]"));
            Assert.That(first.ToString(), Does.Not.Contain(firstValue));
        });
    }

    private static string Hash(string value)
    {
        return Convert.ToHexString(SHA256.HashData(Encoding.UTF8.GetBytes(value)))
            .ToLowerInvariant();
    }
}
