using Microsoft.Extensions.Hosting;
using Styrhous.Licensing.Tests.Infrastructure;
using Microsoft.EntityFrameworkCore.Diagnostics;
using Microsoft.Extensions.Configuration;
using Microsoft.Extensions.DependencyInjection;
using Microsoft.Extensions.DependencyInjection.Extensions;
using Styrhous.Licensing.Application.Messaging;
using Styrhous.Licensing.Tests.Persistence;

namespace Styrhous.Licensing.Tests;

internal static class WorkerServiceTestBase
{
    public static async Task<ServiceTestBase<T>> CreateAsync<T>(DateTimeOffset observedAt,
        Action<IServiceCollection>? configureServices = null, params IInterceptor[] interceptors) where T : notnull
    {
        var database = await PostgresTestDatabase.CreateAsync();
        try
        {
            return ServiceTestBase<T>.FromHost(database,
                BuildHost(database, observedAt, configureServices, interceptors), ownsDatabase: true);
        }
        catch
        {
            await database.DisposeAsync();
            throw;
        }
    }

    public static ServiceTestBase<T> ForDatabase<T>(PostgresTestDatabase database, DateTimeOffset observedAt,
        Action<IServiceCollection>? configureServices = null, params IInterceptor[] interceptors) where T : notnull
    {
        return ServiceTestBase<T>.FromHost(database, BuildHost(database, observedAt, configureServices, interceptors));
    }

    internal static IHost BuildHost(PostgresTestDatabase database, DateTimeOffset observedAt,
        Action<IServiceCollection>? configureServices = null, params IInterceptor[] interceptors)
    {
        return Program.BuildWorker([
            "--environment=Production",
            "--Authentication:GitHub:ClientId=",
            "--Authentication:GitHub:ClientSecret=",
            "--Authentication:Google:ClientId=",
            "--Authentication:Google:ClientSecret=",
            "--Authentication:Microsoft:ClientId=",
            "--Authentication:Microsoft:ClientSecret=",
            $"--ConnectionStrings:Licensing={database.ConnectionString}",
            "--Messaging:Transport=RabbitMq",
            $"--Messaging:QueueName={database.DatabaseName}",
            $"--Messaging:RabbitMq:ConnectionString={RabbitMqTestConnection.Value}",
            $"--DataProtection:Certificate={TestDataProtectionCertificate.EncodedCertificate}",
            $"--DataProtection:CertificatePassword={TestDataProtectionCertificate.Password}",
            "--InfrastructureSmoke:ProcessingDelaySeconds=0",
            "--InvitationEmail:FromAddress=invitations@example.com",
            "--InvitationEmail:AcceptanceUrl=https://localhost/invitations/accept",
            "--InvitationEmail:AmazonSes:Region=eu-west-1",
            "--Stripe:SecretKey=sk_test_worker",
            "--Stripe:MonthlyPriceId=price_monthly",
            "--Stripe:AnnualPriceId=price_annual",
        ], applicationConfiguration: null, configureBuilder: builder =>
        {
            builder.ConfigureContainer(new DefaultServiceProviderFactory(new ServiceProviderOptions
            {
                ValidateScopes = true,
                ValidateOnBuild = true,
            }));
            builder.Services.RemoveAll<TimeProvider>();
            builder.Services.AddSingleton<TimeProvider>(new FixedTimeProvider(observedAt));
            foreach (var interceptor in interceptors)
            {
                builder.Services.AddSingleton(interceptor);
            }
            builder.Services.RemoveAll<IOrganizationInvitationEmailSender>();
            builder.Services.AddSingleton<IOrganizationInvitationEmailSender, UnexpectedEmailSender>();
            configureServices?.Invoke(builder.Services);
        });
    }

    private sealed class UnexpectedEmailSender : IOrganizationInvitationEmailSender
    {
        public Task SendAsync(Guid outboxMessageId, OrganizationInvitationDelivery delivery,
            CancellationToken cancellationToken)
        {
            throw new AssertionException("Configure an email sender for tests that submit email.");
        }
    }
}
