using System.Collections.Concurrent;
using System.Collections.Immutable;
using System.Text.Json;
using Pulumi.Testing;

namespace Styrhous.Licensing.Infrastructure.Tests;

[TestFixture]
[NonParallelizable]
public sealed partial class LicensingStackTests
{
    private static readonly string[] AvailabilityZoneNames = ["eu-west-1a", "eu-west-1b"];
    private static readonly string[] AvailabilityZoneIds = ["euw1-az1", "euw1-az2"];
    private static readonly string[] DatabaseCredentialPropertyNames = ["username", "password"];
    private static readonly string[] ExpectedQueueSendActions =
        ["sqs:GetQueueUrl", "sqs:SendMessageBatch"];
    private static readonly string[] DnsRecordTypes = ["A", "AAAA"];
    private static readonly string[] ApiPathPatterns =
        ["/api/*", "/auth/*", "/desktop/*", "/health"];
    private static readonly string[] ApiConfigurationSections =
    [
        "ConnectionStrings",
        "DesktopProtocol",
        "DataProtection",
        "Stripe",
        "Authentication",
    ];
    private static readonly string[] WorkerConfigurationSections =
    [
        "ConnectionStrings",
        "DataProtection",
        "Stripe",
        "InvitationEmail",
        "InfrastructureSmoke",
    ];
    private static readonly string[] ConnectionConfigurationSections = ["ConnectionStrings"];
    private static readonly string[] ProvisioningConfigurationSections =
        ["ConnectionStrings", "DatabaseRuntimeRole"];
    private static readonly string[] ForbiddenRuntimeConfigurationVariables =
    [
        "STYRHOUS_APPLICATION_CONFIGURATION",
        "ConnectionStrings__Licensing",
    ];
    private static readonly string[] SensitiveFixtureValues =
    [
        "test;database=password",
        "test;runtime=password",
        "desktop-certificate",
        "desktop-password",
        "data-protection-certificate",
        "data-protection-password",
        "stripe-secret",
        "stripe-webhook-secret",
        "github-secret",
        "google-secret",
        "microsoft-secret",
    ];
    private string? _originalConfig;
    private Dictionary<string, string> _configuration = null!;

    [SetUp]
    public void SetUp()
    {
        _originalConfig = Environment.GetEnvironmentVariable("PULUMI_CONFIG");
        _configuration = new Dictionary<string, string>
        {
            ["styrhous-licensing:apiImageUri"] = "registry.example/api@sha256:api",
            ["styrhous-licensing:workerImageUri"] = "registry.example/worker@sha256:worker",
            ["styrhous-licensing:databasePassword"] = "test;database=password",
            ["styrhous-licensing:databaseRuntimePassword"] = "test;runtime=password",
            ["styrhous-licensing:hostedOrigin"] = "https://license.example.com",
            ["styrhous-licensing:desktopCertificate"] = "desktop-certificate",
            ["styrhous-licensing:desktopCertificatePassword"] = "desktop-password",
            ["styrhous-licensing:dataProtectionCertificate"] = "data-protection-certificate",
            ["styrhous-licensing:dataProtectionCertificatePassword"] = "data-protection-password",
            ["styrhous-licensing:stripeSecretKey"] = "stripe-secret",
            ["styrhous-licensing:stripeWebhookSecret"] = "stripe-webhook-secret",
            ["styrhous-licensing:stripeMonthlyPriceId"] = "price_monthly",
            ["styrhous-licensing:stripeAnnualPriceId"] = "price_annual",
            ["styrhous-licensing:stripeCustomerPortalConfigurationId"] = "bpc_test",
            ["styrhous-licensing:invitationFromAddress"] = "licensing@example.com",
            ["styrhous-licensing:alertEmailAddress"] = "alerts@example.com",
            ["styrhous-licensing:githubClientId"] = "github-client",
            ["styrhous-licensing:githubClientSecret"] = "github-secret",
            ["styrhous-licensing:googleClientId"] = "google-client",
            ["styrhous-licensing:googleClientSecret"] = "google-secret",
            ["styrhous-licensing:microsoftClientId"] = "microsoft-client",
            ["styrhous-licensing:microsoftClientSecret"] = "microsoft-secret",
        };
        ApplyConfiguration();
    }

    [TearDown]
    public void TearDown()
    {
        Environment.SetEnvironmentVariable("PULUMI_CONFIG", _originalConfig);
    }

    private void ApplyConfiguration()
    {
        Environment.SetEnvironmentVariable(
            "PULUMI_CONFIG",
            JsonSerializer.Serialize(_configuration));
    }

    private void AddCustomDomainConfiguration(string domainName)
    {
        _configuration["styrhous-licensing:domainName"] = domainName;
        _configuration["styrhous-licensing:hostedZoneId"] = "hosted-zone";
        _configuration["styrhous-licensing:certificateArn"] =
            "arn:aws:acm:::certificate/test";
    }

    private static JsonElement JsonInput(MockResourceArgs resource, string name)
    {
        return JsonSerializer.SerializeToElement(resource.Inputs[name]);
    }

    private static void AssertPropertyNames(
        JsonElement value,
        params string[] expected)
    {
        Assert.That(
            value.EnumerateObject().Select(property => property.Name),
            Is.EquivalentTo(expected));
    }

    private static string Policy(RecordingMocks mocks, string name)
    {
        return JsonInput(
            mocks.Named("aws:iam/rolePolicy:RolePolicy", name),
            "policy").GetString()!;
    }

    private static Dictionary<string, string> LambdaEnvironment(
        MockResourceArgs function)
    {
        return JsonInput(function, "environment")
            .GetProperty("variables")
            .EnumerateObject()
            .ToDictionary(
                property => property.Name,
                property => property.Value.GetString()!);
    }

    private static Dictionary<string, string> TaskEnvironment(
        MockResourceArgs task)
    {
        using var definitions = JsonDocument.Parse(
            JsonInput(task, "containerDefinitions").GetString()!);
        return definitions.RootElement[0]
            .GetProperty("environment")
            .EnumerateArray()
            .ToDictionary(
                variable => variable.GetProperty("name").GetString()!,
                variable => variable.GetProperty("value").GetString()!);
    }

    private static void AssertSafeRuntimeEnvironment(
        IReadOnlyDictionary<string, string> environment,
        string secretName)
    {
        Assert.Multiple(() =>
        {
            Assert.That(
                environment["LICENSING_SECRET_ARN"],
                Is.EqualTo($"arn:aws:test:::{secretName}"));
            Assert.That(
                environment["LICENSING_SECRET_VERSION"],
                Is.EqualTo($"{secretName}-version"));
            Assert.That(
                environment.Keys.Intersect(ForbiddenRuntimeConfigurationVariables),
                Is.Empty);
            var serialized = string.Join(
                '\n',
                environment.Select(item => $"{item.Key}={item.Value}"));
            foreach (var sensitiveValue in SensitiveFixtureValues)
            {
                Assert.That(serialized, Does.Not.Contain(sensitiveValue));
            }
        });
    }

    private static TestOptions Options(bool isPreview)
    {
        return new()
        {
            IsPreview = isPreview,
            ProjectName = "styrhous-licensing",
            StackName = "test",
        };
    }

    private sealed class RecordingMocks : IMocks
    {
        private readonly ConcurrentBag<MockResourceArgs> _resources = [];

        public MockResourceArgs Single(string type)
        {
            return _resources.Single(item => item.Type == type);
        }

        public MockResourceArgs Named(string type, string name)
        {
            return _resources.Single(item => item.Type == type && item.Name == name);
        }

        public IEnumerable<MockResourceArgs> All(string type)
        {
            return _resources.Where(item => item.Type == type);
        }

        public Task<(string? id, object state)> NewResourceAsync(MockResourceArgs args)
        {
            _resources.Add(args);
            var name = args.Name ?? "resource";
            var outputs = ImmutableDictionary.CreateBuilder<string, object>();
            outputs.AddRange(args.Inputs);
            outputs.TryAdd("name", name);
            outputs.TryAdd("id", args.Id ?? $"{name}-id");
            outputs.TryAdd("arn", $"arn:aws:test:::{name}");
            outputs.TryAdd("domainName", $"{name}.example.com");
            outputs.TryAdd("apiEndpoint", "https://api.example.com");
            outputs.TryAdd("endpoint", "proxy.example.com");
            outputs.TryAdd("repositoryUrl", $"registry.example/{name}");
            outputs.TryAdd("url", $"https://sqs.example.com/{name}");
            outputs.TryAdd("bucketRegionalDomainName", $"{name}.s3.example.com");
            outputs.TryAdd("executionArn", $"arn:aws:execute-api:::{name}");
            outputs.TryAdd("identifier", name);
            outputs.TryAdd("versionId", $"{name}-version");
            return Task.FromResult<(string? id, object state)>((args.Id ?? $"{name}-id", outputs));
        }

        public Task<object> CallAsync(MockCallArgs args)
        {
            if (args.Token == "aws:index/getAvailabilityZones:getAvailabilityZones")
            {
                return Task.FromResult<object>(new Dictionary<string, object>
                {
                    ["names"] = AvailabilityZoneNames,
                    ["zoneIds"] = AvailabilityZoneIds,
                });
            }

            return Task.FromResult<object>(args.Args);
        }
    }
}
