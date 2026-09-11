using Microsoft.EntityFrameworkCore;
using Microsoft.Extensions.DependencyInjection;
using System.Text.Json;
using Styrhous.Licensing.Persistence;
using Styrhous.Licensing.Runtime;

namespace Styrhous.Licensing.Tests.Runtime;

[TestFixture]
[Parallelizable(ParallelScope.All)]
public sealed class ApplicationConfigurationLoaderTests
{
    [Test]
    public async Task MissingRuntimeConfigurationUsesOrdinaryHostSources()
    {
        var factoryCalled = false;

        var result = await ApplicationConfigurationLoader.LoadAsync(
            _ => null,
            () =>
            {
                factoryCalled = true;
                return new RecordingSecretReader("unused");
            });

        Assert.Multiple(() =>
        {
            Assert.That(result, Is.Null);
            Assert.That(factoryCalled, Is.False);
        });
    }

    [Test]
    public async Task InlineConfigurationAvoidsAnExternalSecretRead()
    {
        const string configuration =
            "{\"ConnectionStrings\":{\"Licensing\":\"inline\"}}";
        var values = EnvironmentValues(
            (ApplicationConfigurationLoader.InlineConfigurationVariable,
                configuration));
        var factoryCalled = false;

        var result = await ApplicationConfigurationLoader.LoadAsync(
            values.GetValueOrDefault,
            () =>
            {
                factoryCalled = true;
                return new RecordingSecretReader("unused");
            });

        Assert.Multiple(() =>
        {
            Assert.That(result, Is.EqualTo(configuration));
            Assert.That(factoryCalled, Is.False);
        });
    }

    [Test]
    public async Task SecretArnLoadsAndDisposesTheExternalReader()
    {
        var values = EnvironmentValues(
            (ApplicationConfigurationLoader.SecretArnVariable,
                "  arn:aws:secretsmanager:eu-west-1:123456789012:secret:licensing  "),
            (ApplicationConfigurationLoader.SecretVersionVariable, " version-7 "));
        var reader = new RecordingSecretReader(
            "{\"ConnectionStrings\":{\"Licensing\":\"secret\"}}");

        var result = await ApplicationConfigurationLoader.LoadAsync(
            values.GetValueOrDefault,
            () => reader);

        Assert.Multiple(() =>
        {
            Assert.That(result, Does.Contain("secret"));
            Assert.That(reader.SecretIds, Has.Count.EqualTo(1));
            Assert.That(
                reader.SecretIds[0],
                Is.EqualTo(
                    "arn:aws:secretsmanager:eu-west-1:123456789012:secret:licensing"));
            Assert.That(reader.VersionIds, Has.Count.EqualTo(1));
            Assert.That(reader.VersionIds[0], Is.EqualTo("version-7"));
            Assert.That(reader.Disposed, Is.True);
        });
    }

    [Test]
    public void InlineAndSecretConfigurationCannotCompeteForPrecedence()
    {
        var values = EnvironmentValues(
            (ApplicationConfigurationLoader.InlineConfigurationVariable, "{}"),
            (ApplicationConfigurationLoader.SecretArnVariable, "secret-id"));
        var factoryCalled = false;

        Assert.That(
            async () => await ApplicationConfigurationLoader.LoadAsync(
                values.GetValueOrDefault,
                () =>
                {
                    factoryCalled = true;
                    return new RecordingSecretReader("unused");
                }),
            Throws.InvalidOperationException.With.Message.Contains(
                "not both"));
        Assert.That(factoryCalled, Is.False);
    }

    [Test]
    public void SecretVersionCannotBeConfiguredWithoutASecretArn()
    {
        var values = EnvironmentValues(
            (ApplicationConfigurationLoader.SecretVersionVariable, "version-7"));

        Assert.That(
            async () => await ApplicationConfigurationLoader.LoadAsync(
                values.GetValueOrDefault,
                () => new RecordingSecretReader("unused")),
            Throws.InvalidOperationException.With.Message.Contains(
                ApplicationConfigurationLoader.SecretArnVariable));
    }

    [TestCase(null)]
    [TestCase("")]
    [TestCase("   ")]
    public void SecretArnRequiresAStringSecret(string? secret)
    {
        var values = EnvironmentValues(
            (ApplicationConfigurationLoader.SecretArnVariable, "secret-id"));
        var reader = new RecordingSecretReader(secret);

        Assert.That(
            async () => await ApplicationConfigurationLoader.LoadAsync(
                values.GetValueOrDefault,
                () => reader),
            Throws.InvalidOperationException.With.Message.Contains(
                "non-empty SecretString"));
        Assert.That(reader.Disposed, Is.True);
    }

    [Test]
    public void CancelledSecretReadStillDisposesTheExternalReader()
    {
        var values = EnvironmentValues(
            (ApplicationConfigurationLoader.SecretArnVariable, "secret-id"));
        var reader = new RecordingSecretReader("unused");
        using var cancellation = new CancellationTokenSource();
        cancellation.Cancel();

        Assert.That(
            async () => await ApplicationConfigurationLoader.LoadAsync(
                values.GetValueOrDefault,
                () => reader,
                cancellation.Token),
            Throws.InstanceOf<OperationCanceledException>());
        Assert.That(reader.Disposed, Is.True);
    }

    [Test]
    public void LoadedJsonOverridesOrdinaryHostConfiguration()
    {
        const string connectionString =
            "Host=proxy.example;Database=licensing;Username=test;Password=secret";
        var configuration = JsonSerializer.Serialize(new
        {
            ConnectionStrings = new { Licensing = connectionString },
        });

        using var host = Program.BuildMigrationHost(
            ["--ConnectionStrings:Licensing=command-line"],
            configuration);
        using var scope = host.Services.CreateScope();
        var database = scope.ServiceProvider.GetRequiredService<LicensingDbContext>();

        Assert.That(
            database.Database.GetConnectionString(),
            Is.EqualTo(connectionString));
    }

    private static Dictionary<string, string?> EnvironmentValues(
        params (string Key, string? Value)[] values)
    {
        return values.ToDictionary(value => value.Key, value => value.Value);
    }

    private sealed class RecordingSecretReader(string? secret) :
        IApplicationSecretReader
    {
        public List<string> SecretIds { get; } = [];

        public List<string?> VersionIds { get; } = [];

        public bool Disposed { get; private set; }

        public Task<string?> ReadSecretStringAsync(
            string secretId,
            string? versionId,
            CancellationToken cancellationToken)
        {
            cancellationToken.ThrowIfCancellationRequested();
            SecretIds.Add(secretId);
            VersionIds.Add(versionId);
            return Task.FromResult(secret);
        }

        public void Dispose()
        {
            Disposed = true;
        }
    }
}
