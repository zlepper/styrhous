using Styrhous.Licensing.Domain.Devices;

namespace Styrhous.Licensing.Tests.Domain;

[TestFixture]
public sealed class DesktopInstallationTests
{
    [Test]
    public void InstallationPreservesIdentifierAndNormalizesMetadata()
    {
        var installationId = Guid.CreateVersion7();

        var installation = DesktopInstallation.Create(
            installationId,
            "  Work laptop  ",
            "  linux  ",
            "  x86_64  ",
            "  1.2.3  ");

        Assert.Multiple(() =>
        {
            Assert.That(installation.InstallationId, Is.EqualTo(installationId));
            Assert.That(installation.DisplayName, Is.EqualTo("Work laptop"));
            Assert.That(installation.Platform, Is.EqualTo("linux"));
            Assert.That(installation.Architecture, Is.EqualTo("x86_64"));
            Assert.That(installation.StyrhousVersion, Is.EqualTo("1.2.3"));
        });
    }

    [TestCaseSource(nameof(ExistingInstallationIds))]
    public void InstallationAcceptsExistingIdentifierVersions(Guid installationId)
    {
        Assert.That(
            () => DesktopInstallation.Create(
                installationId,
                "Work laptop",
                "linux",
                "x86_64",
                "1.2.3"),
            Throws.Nothing);
    }

    [TestCase("displayName")]
    [TestCase("platform")]
    [TestCase("architecture")]
    [TestCase("version")]
    public void InstallationRequiresAllMetadata(string missingField)
    {
        Assert.That(
            () => DesktopInstallation.Create(
                Guid.CreateVersion7(),
                missingField == "displayName" ? " " : "Work laptop",
                missingField == "platform" ? " " : "linux",
                missingField == "architecture" ? " " : "x86_64",
                missingField == "version" ? " " : "1.2.3"),
            Throws.TypeOf<ArgumentException>());
    }

    [TestCase("displayName")]
    [TestCase("platform")]
    [TestCase("architecture")]
    [TestCase("version")]
    public void InstallationRejectsMetadataBeyondPersistenceLimits(string oversizedField)
    {
        Assert.That(
            () => DesktopInstallation.Create(
                Guid.CreateVersion7(),
                oversizedField == "displayName"
                    ? new string('d', DesktopInstallation.MaximumDisplayNameLength + 1)
                    : "Work laptop",
                oversizedField == "platform"
                    ? new string('p', DesktopInstallation.MaximumPlatformLength + 1)
                    : "linux",
                oversizedField == "architecture"
                    ? new string('a', DesktopInstallation.MaximumArchitectureLength + 1)
                    : "x86_64",
                oversizedField == "version"
                    ? new string('v', DesktopInstallation.MaximumVersionLength + 1)
                    : "1.2.3"),
            Throws.TypeOf<ArgumentException>());
    }

    private static IEnumerable<Guid> ExistingInstallationIds()
    {
        yield return Guid.NewGuid();
        yield return TestIdentifiers.Version7WithNonRfcVariant;
    }
}
