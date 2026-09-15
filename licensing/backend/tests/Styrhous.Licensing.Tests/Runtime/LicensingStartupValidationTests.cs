using Microsoft.AspNetCore.Builder;
using Microsoft.EntityFrameworkCore;
using Styrhous.Licensing.Tests.Infrastructure;
using Microsoft.Extensions.DependencyInjection;
using Microsoft.Extensions.Hosting;
using Styrhous.Licensing.Application.Messaging;
using Styrhous.Licensing.Domain.Messaging;
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

    [Test]
    public async Task ProductionStartsAfterTheMigrateCommandCreatesTheCurrentSchema()
    {
        await using var database = await PostgresTestDatabase.CreateForMigrationAsync();
        await Program.RunMigrationsAsync([
            $"--ConnectionStrings:Licensing={database.ConnectionString}",
        ]);
        await using var application = CreateApplication(database, "github-client", "github-secret");

        await application.StartAsync();
        await application.StopAsync();
    }

    [Test]
    [NonParallelizable]
    public async Task MonolithStartupRecoversAndConsumesDurableWorkAfterEachRestart()
    {
        await using var database = await PostgresTestDatabase.CreateForMigrationAsync();
        await Program.RunMigrationsAsync([
            $"--ConnectionStrings:Licensing={database.ConnectionString}",
        ]);

        var firstWorkId = await EnqueueUnprotectedInvitationDeliveryAsync(database);
        await using (var firstApplication = CreateApplication(
                         database,
                         "github-client",
                         "github-secret"))
        {
            await firstApplication.StartAsync();
            try
            {
                await AssertWorkWasConsumedAsync(database, firstWorkId);
            }
            finally
            {
                await firstApplication.StopAsync();
            }
        }

        var restartedWorkId = await EnqueueUnprotectedInvitationDeliveryAsync(database);
        await using var restartedApplication = CreateApplication(
            database,
            "github-client",
            "github-secret");
        await restartedApplication.StartAsync();
        try
        {
            await AssertWorkWasConsumedAsync(database, restartedWorkId);
        }
        finally
        {
            await restartedApplication.StopAsync();
        }
    }

    [Test]
    public async Task ProductionRejectsThePendingInitialMigration()
    {
        await using var database = await PostgresTestDatabase.CreateForMigrationAsync();
        await using var application = CreateApplication(database, "github-client", "github-secret");

        Assert.That(
            async () => await application.StartAsync(),
            Throws.TypeOf<InvalidOperationException>()
                .With.Message.Contains("pending migration"));
    }

    private static readonly Type[] ApiStartupValidatorTypes =
        [typeof(BrowserAuthenticationStartupValidation), typeof(LicensingStartupValidation)];

    private static async Task ValidateApiStartupAsync(WebApplication application)
    {
        var validators = application.Services.GetServices<IHostedService>()
            .Where(service => service is BrowserAuthenticationStartupValidation or LicensingStartupValidation)
            .ToArray();
        Assert.That(validators.Select(service => service.GetType()), Is.EqualTo(ApiStartupValidatorTypes));
        foreach (var validator in validators.OfType<BrowserAuthenticationStartupValidation>())
        {
            await validator.StartAsync(CancellationToken.None);
        }
    }

    private static async Task<Guid> EnqueueUnprotectedInvitationDeliveryAsync(
        PostgresTestDatabase database)
    {
        var observedAt = DateTimeOffset.UtcNow;
        var message = OutboxMessage.Enqueue(
            Guid.CreateVersion7(),
            Guid.CreateVersion7(),
            OutboxMessageTypes.OrganizationInvitationDelivery,
            "not-a-protected-invitation-delivery",
            observedAt,
            observedAt.AddHours(1));
        await using var context = database.CreateContext();
        context.OutboxMessages.Add(message);
        await context.SaveChangesAsync();
        return message.Id;
    }

    private static async Task AssertWorkWasConsumedAsync(
        PostgresTestDatabase database,
        Guid workId)
    {
        for (var attempt = 0; attempt < 100; attempt++)
        {
            await using var context = database.CreateContext();
            var message = await context.OutboxMessages.SingleAsync(candidate => candidate.Id == workId);
            if (message.DiscardReason == OutboxDiscardReason.UndeliverableProtectedPayload)
            {
                Assert.That(message.NativeOutboxEnqueued, Is.True);
                return;
            }

            await Task.Delay(TimeSpan.FromMilliseconds(100));
        }

        Assert.Fail($"Background work {workId} was not consumed within 10 seconds.");
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
            $"--Messaging:QueueName={database.DatabaseName}",
            $"--Messaging:ErrorQueueName={database.DatabaseName}-error",
            "--urls=http://127.0.0.1:0",
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
            "--InvitationEmail:FromAddress=invitations@example.com",
            "--InvitationEmail:AcceptanceUrl=https://localhost/invitations/accept",
            "--InvitationEmail:Smtp:Host=localhost",
            "--InvitationEmail:Smtp:Port=587",
            "--InvitationEmail:Smtp:Username=test",
            "--InvitationEmail:Smtp:Password=test",
            .. values.Select(pair => $"--{pair.Key}={pair.Value}"),
        ]);
    }
}
