using Microsoft.EntityFrameworkCore.Diagnostics;
using Microsoft.Extensions.DependencyInjection;
using Microsoft.Extensions.DependencyInjection.Extensions;
using Styrhous.Licensing.Application.Messaging;
using Styrhous.Licensing.Tests.Persistence;

namespace Styrhous.Licensing.Tests;

internal static class WorkerServiceTestBase
{
    public static Task<ServiceTestBase<T>> CreateAsync<T>(
        DateTimeOffset observedAt,
        Action<IServiceCollection>? configureServices = null,
        params IInterceptor[] interceptors)
        where T : notnull
    {
        return ServiceTestBase<T>.CreateAsync(
            observedAt,
            services => ConfigureServices(services, configureServices),
            interceptors: interceptors);
    }

    public static ServiceTestBase<T> ForDatabase<T>(
        PostgresTestDatabase database,
        DateTimeOffset observedAt,
        Action<IServiceCollection>? configureServices = null,
        params IInterceptor[] interceptors)
        where T : notnull
    {
        return ServiceTestBase<T>.ForDatabase(
            database,
            observedAt,
            services => ConfigureServices(services, configureServices),
            interceptors: interceptors);
    }

    private static void ConfigureServices(
        IServiceCollection services,
        Action<IServiceCollection>? configureServices)
    {
        services.RemoveAll<IOrganizationInvitationEmailSender>();
        services.AddSingleton<IOrganizationInvitationEmailSender, UnexpectedEmailSender>();
        configureServices?.Invoke(services);
    }

    private sealed class UnexpectedEmailSender : IOrganizationInvitationEmailSender
    {
        public Task SendAsync(
            Guid outboxMessageId,
            OrganizationInvitationDelivery delivery,
            CancellationToken cancellationToken)
        {
            throw new AssertionException(
                "Configure an email sender for tests that submit email.");
        }
    }
}
