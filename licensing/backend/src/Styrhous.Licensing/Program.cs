using Amazon;
using Amazon.SimpleEmailV2;
using System.Text;
using System.Text.Json;
using Microsoft.AspNetCore.Authorization;
using Microsoft.AspNetCore.Identity;
using Microsoft.AspNetCore.HttpLogging;
using Microsoft.EntityFrameworkCore;
using Microsoft.EntityFrameworkCore.Diagnostics;
using Microsoft.Extensions.Options;
using OpenIddict.EntityFrameworkCore;
using Stripe;
using Styrhous.Licensing.Application.Messaging;
using Styrhous.Licensing.Application.Organizations;
using Styrhous.Licensing.Application.Signups;
using Styrhous.Licensing.Infrastructure.Identity;
using Styrhous.Licensing.Infrastructure.Messaging;
using Styrhous.Licensing.Persistence;
using Styrhous.Licensing.Runtime;

namespace Styrhous.Licensing;

public sealed class Program
{
    private const string LicensingConnectionString = "Licensing";
    private Program()
    {
    }

    public static async Task Main(string[] args)
    {
        var command = LicensingCommand.Parse(args);
        var applicationConfiguration =
            await ApplicationConfigurationLoader.LoadAsync();
        switch (command.Mode)
        {
            case LicensingRuntimeMode.Api:
                await BuildApplication(
                    [.. command.HostArguments],
                    applicationConfiguration).RunAsync();
                return;
            case LicensingRuntimeMode.Migrate:
                await RunMigrationsAsync(
                    [.. command.HostArguments],
                    applicationConfiguration);
                return;
            default:
                throw new ArgumentOutOfRangeException(
                    nameof(args),
                    command.Mode,
                    "The licensing runtime mode is not supported.");
        }
    }

    public static WebApplication BuildApplication(string[] args)
    {
        return BuildApplication(args, ApplicationConfigurationLoader.ReadInline());
    }

    internal static WebApplication BuildApplication(
        string[] args,
        string? applicationConfiguration)
    {
        var builder = WebApplication.CreateBuilder(args);
        ConfigureStructuredLogging(builder.Logging);
        AddApplicationConfiguration(builder.Configuration, applicationConfiguration);
        ConfigurePersistence(builder.Services, builder.Configuration);
        var application = builder.Build();
        application.MapGet("/health", () => TypedResults.Ok(new { status = "healthy" }));
        return application;
    }

    internal static Task RunMigrationsAsync(string[] args)
    {
        return RunMigrationsAsync(args, ApplicationConfigurationLoader.ReadInline());
    }

    private static async Task RunMigrationsAsync(
        string[] args,
        string? applicationConfiguration)
    {
        using var host = BuildMigrationHost(args, applicationConfiguration);
        await host.StartAsync();
        try
        {
            await using var scope = host.Services.CreateAsyncScope();
            var dbContext = scope.ServiceProvider.GetRequiredService<LicensingDbContext>();
            await dbContext.Database.MigrateAsync();

        }
        finally
        {
            await host.StopAsync(CancellationToken.None);
        }
    }

    internal static IHost BuildMigrationHost(string[] args)
    {
        return BuildMigrationHost(args, ApplicationConfigurationLoader.ReadInline());
    }

    internal static IHost BuildMigrationHost(
        string[] args,
        string? applicationConfiguration)
    {
        var builder = Host.CreateApplicationBuilder(args);
        ConfigureStructuredLogging(builder.Logging);
        AddApplicationConfiguration(
            builder.Configuration,
            applicationConfiguration);
        ConfigurePersistence(builder.Services, builder.Configuration);
        return builder.Build();
    }

    private static string ConfigurePersistence(
        IServiceCollection services,
        ConfigurationManager configuration)
    {
        var connectionString = configuration.GetConnectionString(
            LicensingConnectionString);
        if (string.IsNullOrWhiteSpace(connectionString))
        {
            throw new InvalidOperationException(
                $"ConnectionStrings:{LicensingConnectionString} is required.");
        }

        services.AddDbContextFactory<LicensingDbContext>((serviceProvider, options) =>
        {
            options.UseNpgsql(connectionString);
            options.UseOpenIddict<Guid>();
            options.AddInterceptors(serviceProvider.GetServices<IInterceptor>());
        });
        services.AddSingleton(TimeProvider.System);
        return connectionString;
    }

    internal static void ConfigureStructuredLogging(ILoggingBuilder logging)
    {
        logging.ClearProviders();
        logging.AddJsonConsole();
    }

    private static void AddApplicationConfiguration(
        IConfigurationBuilder configuration,
        string? json)
    {
        if (string.IsNullOrWhiteSpace(json))
        {
            return;
        }

        configuration.AddJsonStream(new MemoryStream(Encoding.UTF8.GetBytes(json)));
    }

}
