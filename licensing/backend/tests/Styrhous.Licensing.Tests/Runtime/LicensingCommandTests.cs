using Styrhous.Licensing.Runtime;

namespace Styrhous.Licensing.Tests.Runtime;

[TestFixture]
[Parallelizable(ParallelScope.All)]
public sealed class LicensingCommandTests
{
    private static readonly string[] ApiHostArguments =
        ["--urls", "http://localhost:5000"];

    private static readonly string[] EnvironmentHostArguments =
        ["--environment", "Development"];

    private static readonly string[] UnknownModeArguments = ["serve"];

    [TestCase(null)]
    [TestCase("api")]
    public void ApiIsTheDefaultMode(string? explicitMode)
    {
        string[] arguments = explicitMode is null
            ? ApiHostArguments
            : [explicitMode, .. ApiHostArguments];

        var command = LicensingCommand.Parse(arguments);

        Assert.Multiple(() =>
        {
            Assert.That(command.Mode, Is.EqualTo(LicensingRuntimeMode.Api));
            Assert.That(
                command.HostArguments,
                Is.EqualTo(ApiHostArguments));
        });
    }

    [TestCase("worker", LicensingRuntimeMode.Worker)]
    [TestCase("maintenance", LicensingRuntimeMode.Maintenance)]
    [TestCase("maintenance-lambda", LicensingRuntimeMode.MaintenanceLambda)]
    [TestCase("migrate", LicensingRuntimeMode.Migrate)]
    public void ExplicitModeIsRemovedBeforeHostConfiguration(
        string mode,
        LicensingRuntimeMode expected)
    {
        var command = LicensingCommand.Parse([mode, .. EnvironmentHostArguments]);

        Assert.Multiple(() =>
        {
            Assert.That(command.Mode, Is.EqualTo(expected));
            Assert.That(
                command.HostArguments,
                Is.EqualTo(EnvironmentHostArguments));
        });
    }

    [Test]
    public void UnknownPositionalModeIsRejected()
    {
        var exception = Assert.Throws<ArgumentException>(
            () => LicensingCommand.Parse(UnknownModeArguments));

        Assert.That(
            exception!.Message,
            Does.Contain("api, worker, maintenance, maintenance-lambda, or migrate"));
    }
}
