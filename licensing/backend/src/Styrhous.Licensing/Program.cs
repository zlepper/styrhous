using Amazon;
using Amazon.SimpleEmailV2;
using System.Text;
using System.Text.Json;
using Microsoft.AspNetCore.Authentication;
using Microsoft.AspNetCore.Authentication.Cookies;
using Microsoft.AspNetCore.Authorization;
using Microsoft.AspNetCore.HttpLogging;
using Microsoft.EntityFrameworkCore;
using Microsoft.EntityFrameworkCore.Diagnostics;
using Microsoft.Extensions.Options;
using OpenIddict.EntityFrameworkCore;
using Stripe;
using Styrhous.Licensing.Api.Antiforgery;
using Styrhous.Licensing.Api.Authentication;
using Styrhous.Licensing.Api.Devices;
using Styrhous.Licensing.Api.Desktop;
using Styrhous.Licensing.Api.Entitlements;
using Styrhous.Licensing.Application.Billing;
using Styrhous.Licensing.Application.Devices;
using Styrhous.Licensing.Application.Entitlements;
using Styrhous.Licensing.Application.Messaging;
using Styrhous.Licensing.Application.Organizations;
using Styrhous.Licensing.Application.Signups;
using Styrhous.Licensing.Infrastructure.Billing;
using Styrhous.Licensing.Infrastructure.DataProtection;
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
        AddApplicationConfiguration(
            builder.Configuration,
            applicationConfiguration);
        builder.Logging.AddFilter("OpenIddict", LogLevel.Warning);
        builder.Logging.AddFilter(
            "Microsoft.AspNetCore.Hosting.Diagnostics",
            LogLevel.Warning);
        ConfigurePersistence(builder.Services, builder.Configuration);
        ConfigureApiServices(
            builder.Services,
            builder.Configuration);
        builder.Services.AddValidation();
        builder.Services.AddProblemDetails(options =>
        {
            options.CustomizeProblemDetails = context =>
            {
                if (context.ProblemDetails is HttpValidationProblemDetails validation)
                {
                    validation.Errors = validation.Errors.ToDictionary(
                        error => System.Text.Json.JsonNamingPolicy.CamelCase.ConvertName(error.Key),
                        error => error.Value);
                }
            };
        });
        builder.Services.AddHttpLogging(options =>
        {
            options.CombineLogs = true;
            options.LoggingFields =
                HttpLoggingFields.RequestMethod
                | HttpLoggingFields.RequestPath
                | HttpLoggingFields.ResponseStatusCode
                | HttpLoggingFields.Duration;
        });

        var application = builder.Build();
        UseCanonicalPublicOrigin(application);
        application.UseHttpLogging();
        application.Use(async (context, next) =>
        {
            if (context.Request.Path.StartsWithSegments("/api")
                || context.Request.Path.StartsWithSegments("/auth")
                || context.Request.Path.StartsWithSegments("/desktop/v1"))
            {
                context.Response.Headers.CacheControl = "no-store";
            }

            await next(context);
        });
        application.UseAuthentication();
        application.UseAuthorization();
        application.MapAccountAuthenticationEndpoints();
        application.MapAntiforgeryEndpoints();
        application.MapDeviceEndpoints();
        application.MapEntitlementEndpoints();
        application.MapGet("/health", () => TypedResults.Ok(new { status = "healthy" }));
        return application;
    }

    private static void UseCanonicalPublicOrigin(WebApplication application)
    {
        var origin = application.Services.GetRequiredService<Uri>();
        var host = origin.IsDefaultPort
            ? new HostString(origin.Host)
            : new HostString(origin.Host, origin.Port);
        application.Use((context, next) =>
        {
            context.Request.Scheme = origin.Scheme;
            context.Request.Host = host;
            return next(context);
        });
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

    private static void ConfigurePersistence(
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
            options.UseNpgsql(
                connectionString,
                npgsql => npgsql.EnableRetryOnFailure(
                    maxRetryCount: 2,
                    maxRetryDelay: TimeSpan.FromSeconds(2),
                    errorCodesToAdd: ["40001", "40P01"]));
            options.UseOpenIddict<Guid>();
            options.AddInterceptors(serviceProvider.GetServices<IInterceptor>());
        });
        services.AddSingleton(TimeProvider.System);
    }

    internal static void ConfigureStructuredLogging(ILoggingBuilder logging)
    {
        logging.ClearProviders();
        logging.AddJsonConsole();
    }

    private static void ConfigureApiServices(
        IServiceCollection services,
        ConfigurationManager configuration)
    {
        services.AddHostedService<BrowserAuthenticationStartupValidation>();
        services.AddHostedService<LicensingStartupValidation>();
        services.AddScoped<PostgresDeviceListingStore>();
        services.AddScoped<DeviceListingService>();
        services.AddScoped<PostgresDeviceRevocationStore>();
        services.AddScoped<DeviceRevocationService>();
        services.AddScoped<PostgresDeviceEntitlementCheckStore>();
        services.AddScoped<DeviceEntitlementCheckService>();
        services.AddScoped<PostgresUserSignupStore>();
        services.AddScoped<UserSignupService>();
        services.AddScoped<ExternalAccountService>();
        services.AddScoped<PostgresEntitlementStore>();
        services.AddScoped<EntitlementResolutionService>();
        ConfigureInvitationDeliveryProtection(services, configuration);
        services.AddSingleton(DesktopProtocolCertificateConfiguration.LoadIssuer(configuration));
        ConfigureApiSecurity(services, configuration);
    }

    private static void ConfigureInvitationDeliveryProtection(
        IServiceCollection services,
        IConfiguration configuration)
    {
        DataProtectionConfiguration.Configure(services, configuration);
    }

    private static void ConfigureApiSecurity(
        IServiceCollection services,
        IConfiguration configuration)
    {
        var authentication = services.AddAuthentication(options =>
        {
            options.DefaultAuthenticateScheme = AccountAuthentication.SessionScheme;
            options.DefaultChallengeScheme = AccountAuthentication.SessionScheme;
            options.DefaultForbidScheme = AccountAuthentication.SessionScheme;
            options.DefaultSignInScheme = AccountAuthentication.SessionScheme;
        });
        authentication.AddCookie(AccountAuthentication.SessionScheme, options =>
        {
            options.Cookie.Name = "__Host-styrhous-session";
            options.Cookie.HttpOnly = true;
            options.Cookie.SameSite = SameSiteMode.Lax;
            options.Cookie.SecurePolicy = CookieSecurePolicy.Always;
            options.Cookie.Path = "/";
            options.SlidingExpiration = false;
            options.ExpireTimeSpan = TimeSpan.FromDays(30);
            options.Events.OnRedirectToLogin = context =>
            {
                context.Response.StatusCode = StatusCodes.Status401Unauthorized;
                return Task.CompletedTask;
            };
            options.Events.OnRedirectToAccessDenied = context =>
            {
                context.Response.StatusCode = StatusCodes.Status403Forbidden;
                return Task.CompletedTask;
            };
        });
        authentication.AddCookie(AccountAuthentication.ExternalScheme, options =>
        {
            options.Cookie.Name = "__Host-styrhous-external";
            options.Cookie.HttpOnly = true;
            options.Cookie.SameSite = SameSiteMode.Lax;
            options.Cookie.SecurePolicy = CookieSecurePolicy.Always;
            options.Cookie.Path = "/";
            options.ExpireTimeSpan = TimeSpan.FromMinutes(10);
        });
        AccountAuthentication.ConfigureProviders(authentication, configuration);
        services.AddAuthorization();
        services.AddAntiforgery(options =>
        {
            options.HeaderName = AntiforgeryEndpoints.HeaderName;
            options.Cookie.Name = "__Host-styrhous-antiforgery";
            options.Cookie.HttpOnly = true;
            options.Cookie.SameSite = SameSiteMode.Strict;
            options.Cookie.SecurePolicy = CookieSecurePolicy.Always;
            options.Cookie.Path = "/";
        });
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
