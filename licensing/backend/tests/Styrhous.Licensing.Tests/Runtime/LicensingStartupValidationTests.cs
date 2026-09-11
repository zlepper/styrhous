using Microsoft.AspNetCore.Builder;
using Styrhous.Licensing.Tests.Infrastructure;
using Microsoft.Extensions.DependencyInjection;
using Microsoft.Extensions.Hosting;
using Styrhous.Licensing.Runtime;
using Styrhous.Licensing.Tests.Persistence;

namespace Styrhous.Licensing.Tests.Runtime;

[TestFixture]
[Parallelizable(ParallelScope.All)]
public sealed class LicensingStartupValidationTests
{
    [Test]
    public async Task ProductionRequiresAtLeastOneAuthenticationProvider()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        await using var application = CreateApplication(database, clientId: null, clientSecret: null);

        Assert.That(
            async () => await ValidateApiStartupAsync(application),
            Throws.TypeOf<InvalidOperationException>()
                .With.Message.Contains("at least one configured Authentication provider"));
    }

    [TestCase("github-client", null)]
    [TestCase(null, "github-secret")]
    public async Task ProductionRejectsAPartialAuthenticationProvider(
        string? clientId,
        string? clientSecret)
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        await using var application = CreateApplication(database, clientId, clientSecret);

        Assert.That(
            async () => await ValidateApiStartupAsync(application),
            Throws.TypeOf<InvalidOperationException>()
                .With.Message.Contains("requires both ClientId and ClientSecret"));
    }

    [Test]
    public async Task CompleteProviderDoesNotMaskAnotherPartialProvider()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        await using var application = CreateApplication(
            database,
            "github-client",
            "github-secret",
            new Dictionary<string, string?>
            {
                ["Authentication:Google:ClientId"] = "google-client",
            });

        Assert.That(
            async () => await ValidateApiStartupAsync(application),
            Throws.TypeOf<InvalidOperationException>()
                .With.Message.Contains("Authentication:Google requires both"));
    }

    [TestCase("GitHub")]
    [TestCase("Google")]
    [TestCase("Microsoft")]
    public async Task ProductionAcceptsEachConfiguredProviderAndCurrentDatabase(string provider)
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        await using var application = CreateApplication(
            database,
            clientId: null,
            clientSecret: null,
            additionalValues: new Dictionary<string, string?>
            {
                [$"Authentication:{provider}:ClientId"] = $"{provider}-client",
                [$"Authentication:{provider}:ClientSecret"] = $"{provider}-secret",
            });

        await ValidateApiStartupAsync(application);
    }

    private static readonly Type[] ApiStartupValidatorTypes =
        [typeof(BrowserAuthenticationStartupValidation), typeof(LicensingStartupValidation)];

    private static async Task ValidateApiStartupAsync(WebApplication application)
    {
        var validators = application.Services.GetServices<IHostedService>()
            .Where(service => service is BrowserAuthenticationStartupValidation or LicensingStartupValidation)
            .ToArray();
        Assert.That(validators.Select(service => service.GetType()), Is.EqualTo(ApiStartupValidatorTypes));
        foreach (var validator in validators)
        {
            await validator.StartAsync(CancellationToken.None);
        }
    }

    private static WebApplication CreateApplication(
        PostgresTestDatabase database,
        string? clientId,
        string? clientSecret,
        IReadOnlyDictionary<string, string?>? additionalValues = null)
    {
        var values = new Dictionary<string, string?>(additionalValues ?? new Dictionary<string, string?>());
        values.TryAdd("Authentication:GitHub:ClientId", clientId);
        values.TryAdd("Authentication:GitHub:ClientSecret", clientSecret);
        values.TryAdd("Authentication:Google:ClientId", null);
        values.TryAdd("Authentication:Google:ClientSecret", null);
        values.TryAdd("Authentication:Microsoft:ClientId", null);
        values.TryAdd("Authentication:Microsoft:ClientSecret", null);
        return Program.BuildApplication([
            "--environment=Production",
            $"--ConnectionStrings:Licensing={database.ConnectionString}",
            "--Messaging:ApiOutboxForwardingEnabled=false",
            $"--Messaging:QueueName={database.DatabaseName}",
            $"--DataProtection:Certificate={TestDataProtectionCertificate.EncodedCertificate}",
            $"--DataProtection:CertificatePassword={TestDataProtectionCertificate.Password}",
            $"--DesktopProtocol:Certificate={TestDataProtectionCertificate.EncodedDesktopCertificate}",
            $"--DesktopProtocol:CertificatePassword={TestDataProtectionCertificate.Password}",
            "--DesktopProtocol:Issuer=https://localhost/",
            "--Stripe:SecretKey=sk_test_startup",
            "--Stripe:WebhookSecret=whsec_test_startup",
            "--Stripe:MonthlyPriceId=price_monthly",
            "--Stripe:AnnualPriceId=price_annual",
            "--Stripe:CheckoutSuccessUrl=https://localhost/billing?checkout=success",
            "--Stripe:CheckoutCancelUrl=https://localhost/billing?checkout=cancelled",
            "--Stripe:CustomerPortalConfigurationId=bpc_test_startup",
            "--Stripe:CustomerPortalReturnUrl=https://localhost/billing",
            .. values.Select(pair => $"--{pair.Key}={pair.Value}"),
        ], applicationConfiguration: null);
    }
}
