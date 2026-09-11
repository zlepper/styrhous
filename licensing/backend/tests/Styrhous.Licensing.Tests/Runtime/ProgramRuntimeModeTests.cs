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
    public void WorkerBuildsTheReceiverAndInvitationEmailGraph()
    {
        using var host = Program.BuildWorker([.. WorkerArguments(), "--environment=Development"]);

        using var scope = host.Services.CreateScope();
        Assert.That(scope.ServiceProvider.GetRequiredService<Styrhous.Licensing.Application.Billing.BillingWebhookProcessingService>(), Is.Not.Null);
        Assert.That(scope.ServiceProvider.GetRequiredService<OrganizationInvitationDeliveryService>(), Is.Not.Null);

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
