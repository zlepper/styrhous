using Rebus.Bus;
using Styrhous.Licensing.Infrastructure.Messaging;
using Styrhous.Licensing.Persistence;
using Microsoft.Extensions.DependencyInjection;
using Microsoft.Extensions.Options;
using Styrhous.Licensing.Application.Messaging;
using Styrhous.Licensing.Tests.Infrastructure;
using Styrhous.Licensing.Tests.Persistence;

namespace Styrhous.Licensing.Tests.Runtime;

[TestFixture]
[Parallelizable(ParallelScope.All)]
public sealed class ProgramRuntimeModeTests
{
    [Test]
    public void ApiBuildsTheNativeOutboxProducerAndForwarderGraph()
    {
        using var application = Program.BuildApplication(
        [
            .. CommonArguments(),
            .. DataProtectionArguments(),
            "--Stripe:WebhookSecret=whsec_test_api",
        ]);

        var registeredServices = application.Services.GetRequiredService<
            IServiceProviderIsService>();
        Assert.Multiple(() =>
        {
            Assert.That(
                registeredServices.IsService(typeof(IBus)),
                Is.True);
            Assert.That(
                registeredServices.IsService(typeof(PostgresBackgroundWorkOutbox)),
                Is.True);
            Assert.That(
                registeredServices.IsService(
                    typeof(PostgresOrganizationInvitationDeliveryStore)),
                Is.False);
        });
    }

    [TestCase("")]
    [TestCase("invalid/queue")]
    public void DeferredApiForwardingStillRequiresAValidDurableDestination(string queueName)
    {
        using var application = Program.BuildApplication(
        [
            "--ConnectionStrings:Licensing=Host=localhost;Database=licensing;Username=test;Password=test",
            "--Messaging:ApiOutboxForwardingEnabled=false",
            $"--Messaging:QueueName={queueName}",
            .. DataProtectionArguments(),
            "--Stripe:WebhookSecret=whsec_test_api",
        ]);
        Assert.That(() => application.Services.GetRequiredService<PostgresBackgroundWorkOutbox>(),
            Throws.InvalidOperationException);
    }

    [Test]
    public void ApiCanDeferForwardingToMaintenanceWithDurableQueueDestination()
    {
        using var application = Program.BuildApplication(
        [
            "--ConnectionStrings:Licensing=Host=localhost;Database=licensing;Username=test;Password=test",
            "--Messaging:ApiOutboxForwardingEnabled=false",
            "--Messaging:QueueName=styrhous-licensing",
            .. DataProtectionArguments(),
            "--Stripe:WebhookSecret=whsec_test_api",
        ]);

        var registeredServices = application.Services.GetRequiredService<
            IServiceProviderIsService>();
        Assert.Multiple(() =>
        {
            Assert.That(
                registeredServices.IsService(typeof(IBus)),
                Is.False);
            Assert.That(
                registeredServices.IsService(typeof(PostgresBackgroundWorkOutbox)),
                Is.True);
        });
    }

    [Test]
    public void WorkerBuildsTheReceiverAndInvitationEmailGraph()
    {
        using var host = Program.BuildWorker(WorkerArguments());

        Assert.That(
            host.Services.GetService<IOrganizationInvitationEmailSender>(),
            Is.Not.Null);
        Assert.That(
            host.Services.GetService<BackgroundWorkRecoveryService>(),
            Is.Null);
    }

    [Test]
    public void MaintenanceBuildsWithoutEmailOrDataProtectionSecrets()
    {
        using var host = Program.BuildMaintenanceHost(CommonArguments());

        Assert.Multiple(() =>
        {
            Assert.That(
                host.Services.GetService<IOrganizationInvitationEmailSender>(),
                Is.Null);
            Assert.That(
                host.Services.GetService<DataProtectionOrganizationInvitationDeliveryProtector>(),
                Is.Null);
        });
    }

    [Test]
    public void WorkerFailsFastWhenSesRegionIsMissing()
    {
        Assert.That(
            () => Program.BuildWorker(
                [
                    .. CommonArguments(),
                    .. DataProtectionArguments(),
                    .. StripeWorkerArguments(),
                    "--InvitationEmail:FromAddress=noreply@example.com",
                    "--InvitationEmail:AcceptanceUrl=https://licenses.example.com/invitations/accept",
                ]),
            Throws.InvalidOperationException.With.Message.Contains(
                "InvitationEmail:AmazonSes:Region"));
    }

    [Test]
    public void WorkerFailsStartupWhenStripeSettingsAreMissing()
    {
        using var host = Program.BuildWorker(
            [
                .. CommonArguments(),
                .. DataProtectionArguments(),
                .. InvitationEmailArguments(),
            ]);

        var exception = Assert.ThrowsAsync<OptionsValidationException>(
            async () => await host.StartAsync());

        Assert.Multiple(() =>
        {
            Assert.That(exception!.Message, Does.Contain("Stripe:SecretKey"));
            Assert.That(exception.Message, Does.Contain("Stripe:MonthlyPriceId"));
            Assert.That(exception.Message, Does.Contain("Stripe:AnnualPriceId"));
        });
    }

    [Test]
    public void WorkerFailsStartupWhenStripePriceIdsMatch()
    {
        using var host = Program.BuildWorker(
            [
                .. CommonArguments(),
                .. DataProtectionArguments(),
                .. InvitationEmailArguments(),
                "--Stripe:SecretKey=sk_test_worker",
                "--Stripe:MonthlyPriceId=price_same",
                "--Stripe:AnnualPriceId=price_same",
            ]);

        var exception = Assert.ThrowsAsync<OptionsValidationException>(
            async () => await host.StartAsync());

        Assert.That(exception!.Message, Does.Contain("must differ"));
    }

    [Test]
    public void WorkerFailsStartupWhenStripePriceIdsContainSurroundingWhitespace()
    {
        using var host = Program.BuildWorker(
            [
                .. CommonArguments(),
                .. DataProtectionArguments(),
                .. InvitationEmailArguments(),
                "--Stripe:SecretKey=sk_test_worker",
                "--Stripe:MonthlyPriceId= price_monthly",
                "--Stripe:AnnualPriceId=price_annual",
            ]);

        var exception = Assert.ThrowsAsync<OptionsValidationException>(
            async () => await host.StartAsync());

        Assert.That(exception!.Message, Does.Contain("surrounding whitespace"));
    }

    [Test]
    public async Task ApiFailsStartupWhenBrowserBillingDestinationsAreMissing()
    {
        await using var application = Program.BuildApplication(
        [
            "--ConnectionStrings:Licensing=Host=localhost;Database=licensing;Username=test;Password=test",
            "--Messaging:ApiOutboxForwardingEnabled=false",
            "--Messaging:QueueName=styrhous-licensing",
            "--urls=http://127.0.0.1:0",
            .. DataProtectionArguments(),
            "--Stripe:SecretKey=sk_test_api",
            "--Stripe:WebhookSecret=whsec_test_api",
            "--Stripe:MonthlyPriceId=price_monthly",
            "--Stripe:AnnualPriceId=price_annual",
        ]);

        var exception = Assert.ThrowsAsync<OptionsValidationException>(
            async () => await application.StartAsync());

        Assert.Multiple(() =>
        {
            Assert.That(exception!.Message, Does.Contain("Stripe:CheckoutSuccessUrl"));
            Assert.That(exception.Message, Does.Contain("Stripe:CheckoutCancelUrl"));
            Assert.That(
                exception.Message,
                Does.Contain("Stripe:CustomerPortalConfigurationId"));
            Assert.That(
                exception.Message,
                Does.Contain("Stripe:CustomerPortalReturnUrl"));
        });
    }

    [TestCase(
        "--Stripe:CustomerPortalConfigurationId= bpc_test",
        "Stripe:CustomerPortalConfigurationId")]
    [TestCase(
        "--Stripe:CustomerPortalReturnUrl=http://licenses.example.com/billing",
        "Stripe:CustomerPortalReturnUrl")]
    public async Task ApiFailsStartupWhenCustomerPortalConfigurationIsUnsafe(
        string overrideArgument,
        string expectedConfigurationName)
    {
        await using var application = Program.BuildApplication(
        [
            .. CommonArguments(),
            "--Messaging:ApiOutboxForwardingEnabled=false",
            "--Messaging:QueueName=styrhous-licensing",
            "--urls=http://127.0.0.1:0",
            .. DataProtectionArguments(),
            .. StripeApiArguments(),
            overrideArgument,
        ]);

        var exception = Assert.ThrowsAsync<OptionsValidationException>(
            async () => await application.StartAsync());

        Assert.That(exception!.Message, Does.Contain(expectedConfigurationName));
    }

    [Test]
    public async Task MaintenanceRunsOneRecoveryBatchAndExits()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var queueName = $"styrhous-it-{Guid.CreateVersion7():N}";

        var result = await Program.RunMaintenanceAsync(
            CommonArguments(
                database.ConnectionString,
                "amqp://styrhous:local-development-only@127.0.0.1:55672",
                queueName));

        Assert.That(
            result,
            Is.EqualTo(new BackgroundWorkRecoveryResult(OutboxDrained: true)));
    }

    private static string[] CommonArguments(
        string connectionString = "Host=localhost;Database=licensing;Username=test;Password=test",
        string rabbitMqConnectionString = "amqp://localhost",
        string queueName = "styrhous-licensing")
    {
        return [
        $"--ConnectionStrings:Licensing={connectionString}",
        "--Messaging:Transport=RabbitMq",
        $"--Messaging:QueueName={queueName}",
        $"--Messaging:RabbitMq:ConnectionString={rabbitMqConnectionString}",
    ];
    }

    private static string[] DataProtectionArguments()
    {
        return [
        $"--DataProtection:Certificate={TestDataProtectionCertificate.EncodedCertificate}",
        $"--DataProtection:CertificatePassword={TestDataProtectionCertificate.Password}",
        $"--DesktopProtocol:Certificate={TestDataProtectionCertificate.EncodedDesktopCertificate}",
        $"--DesktopProtocol:CertificatePassword={TestDataProtectionCertificate.Password}",
        "--DesktopProtocol:Issuer=https://licenses.example.com/",
    ];
    }

    private static string[] InvitationEmailArguments()
    {
        return [
        "--InvitationEmail:FromAddress=noreply@example.com",
        "--InvitationEmail:AcceptanceUrl=https://licenses.example.com/invitations/accept",
        "--InvitationEmail:AmazonSes:Region=eu-west-1",
    ];
    }

    private static string[] StripeWorkerArguments()
    {
        return [
        "--Stripe:SecretKey=sk_test_worker",
        "--Stripe:MonthlyPriceId=price_monthly",
        "--Stripe:AnnualPriceId=price_annual",
    ];
    }

    private static string[] StripeApiArguments()
    {
        return [
        "--Stripe:SecretKey=sk_test_api",
        "--Stripe:WebhookSecret=whsec_test_api",
        "--Stripe:MonthlyPriceId=price_monthly",
        "--Stripe:AnnualPriceId=price_annual",
        "--Stripe:CheckoutSuccessUrl=https://licenses.example.com/billing?checkout=success",
        "--Stripe:CheckoutCancelUrl=https://licenses.example.com/billing?checkout=cancelled",
        "--Stripe:CustomerPortalConfigurationId=bpc_test",
        "--Stripe:CustomerPortalReturnUrl=https://licenses.example.com/billing",
    ];
    }

    private static string[] WorkerArguments()
    {
        return [
        .. CommonArguments(),
        .. DataProtectionArguments(),
        .. InvitationEmailArguments(),
        .. StripeWorkerArguments(),
    ];
    }
}
