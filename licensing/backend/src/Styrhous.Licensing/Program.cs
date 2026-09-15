using Microsoft.AspNetCore.Authentication;
using Microsoft.AspNetCore.Authentication.Cookies;
using Microsoft.AspNetCore.Authorization;
using Microsoft.AspNetCore.HttpLogging;
using Microsoft.EntityFrameworkCore;
using Microsoft.EntityFrameworkCore.Diagnostics;
using Microsoft.Extensions.Options;
using Npgsql;
using OpenIddict.EntityFrameworkCore;
using Stripe;
using Styrhous.Licensing.Api.Antiforgery;
using Styrhous.Licensing.Api.Authentication;
using Styrhous.Licensing.Api.Billing;
using Styrhous.Licensing.Api.Devices;
using Styrhous.Licensing.Api.Desktop;
using Styrhous.Licensing.Api.Entitlements;
using Styrhous.Licensing.Api.Organizations;
using Styrhous.Licensing.Application.Billing;
using Styrhous.Licensing.Application.Devices;
using Styrhous.Licensing.Application.Desktop;
using Styrhous.Licensing.Application.Entitlements;
using Styrhous.Licensing.Application.Messaging;
using Styrhous.Licensing.Application.Organizations;
using Styrhous.Licensing.Application.Signups;
using Styrhous.Licensing.Infrastructure.Billing;
using Styrhous.Licensing.Infrastructure.DataProtection;
using Styrhous.Licensing.Infrastructure.Messaging;
using Styrhous.Licensing.Infrastructure.Organizations;
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
        switch (command.Mode)
        {
            case LicensingRuntimeMode.Api:
                await BuildApplication([.. command.HostArguments]).RunAsync();
                return;
            case LicensingRuntimeMode.Migrate:
                await RunMigrationsAsync([.. command.HostArguments]);
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
        var builder = WebApplication.CreateBuilder(args);
        ConfigureStructuredLogging(builder.Logging);
        builder.Logging.AddFilter("OpenIddict", LogLevel.Warning);
        builder.Logging.AddFilter(
            "Microsoft.AspNetCore.Hosting.Diagnostics",
            LogLevel.Warning);
        var backgroundMessaging = BackgroundMessagingSettings.From(builder.Configuration);
        var connectionString = ConfigurePersistence(
            builder.Services,
            builder.Configuration,
            backgroundMessaging.QueueName);
        ConfigureApiServices(builder.Services, builder.Configuration);
        ConfigureBackgroundWorkServices(builder.Services, builder.Configuration);
        ConfigureInvitationEmail(builder.Services, builder.Configuration);
        ConfigureBackgroundMessaging(
            builder.Services,
            connectionString,
            backgroundMessaging);
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
            context.Response.OnStarting(() =>
            {
                context.Response.Headers["Content-Security-Policy"] = "frame-ancestors 'none'";
                context.Response.Headers["Referrer-Policy"] = "strict-origin-when-cross-origin";
                context.Response.Headers["X-Content-Type-Options"] = "nosniff";
                context.Response.Headers["X-Frame-Options"] = "DENY";
                return Task.CompletedTask;
            });
            if (context.Request.Path.StartsWithSegments("/api")
                || context.Request.Path.StartsWithSegments("/auth")
                || context.Request.Path.StartsWithSegments("/desktop/v1"))
            {
                context.Response.Headers.CacheControl = "no-store";
            }

            await next(context);
        });
        application.UseMiddleware<DesktopProtocolTransactionMiddleware>();
        application.UseAuthentication();
        application.UseAuthorization();
        application.UseDefaultFiles();
        application.UseStaticFiles();
        UsePortalFallback(application);
        application.MapAccountAuthenticationEndpoints();
        application.MapAntiforgeryEndpoints();
        application.MapBillingAccountEndpoints();
        application.MapBillingWebhookEndpoints();
        application.MapDeviceEndpoints();
        application.MapDesktopDeviceAuthorizationEndpoints();
        application.MapDesktopProtocolMetadataEndpoints();
        application.MapDesktopTokenEndpoints();
        application.MapDesktopEntitlementEndpoints();
        application.MapEntitlementEndpoints();
        application.MapOrganizationEndpoints();
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

    internal static async Task RunMigrationsAsync(string[] args)
    {
        using var host = BuildMigrationHost(args);
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
        var builder = Host.CreateApplicationBuilder(args);
        ConfigureStructuredLogging(builder.Logging);
        ConfigurePersistence(builder.Services, builder.Configuration);
        return builder.Build();
    }

    private static string ConfigurePersistence(
        IServiceCollection services,
        ConfigurationManager configuration,
        string? backgroundQueueName = null)
    {
        var connectionString = configuration.GetConnectionString(
            LicensingConnectionString);
        if (string.IsNullOrWhiteSpace(connectionString))
        {
            throw new InvalidOperationException(
                $"ConnectionStrings:{LicensingConnectionString} is required.");
        }

        connectionString = NormalizeNpgsqlConnectionString(connectionString);
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
        if (backgroundQueueName is not null)
        {
            services.AddSingleton(new PostgresBackgroundWorkOutbox(backgroundQueueName));
        }
        services.AddSingleton(TimeProvider.System);
        return connectionString;
    }

    internal static string NormalizeNpgsqlConnectionString(string connectionString)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(connectionString);
        var builder = new NpgsqlConnectionStringBuilder(connectionString)
        {
            MinPoolSize = 0,
            MaxPoolSize = 30,
            ConnectionIdleLifetime = 10,
            ConnectionPruningInterval = 10,
        };
        return builder.ConnectionString;
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
        services.AddScoped<PostgresBillingAccountListingStore>();
        services.AddScoped<BillingAccountListingService>();
        services.AddScoped<PostgresBillingCheckoutStore>();
        services.AddScoped<PostgresBillingSeatQuantityStore>();
        services.AddScoped<PostgresBillingCustomerPortalStore>();
        services.AddScoped<PostgresCommercialSubscriptionProjectionStore>();
        services.AddScoped<CommercialSubscriptionProjectionService>();
        services.AddScoped<PostgresBillingProviderReadRevisionSource>();
        services.AddScoped<BillingCheckoutCompletionReconciler>();
        services.AddScoped<BillingCheckoutService>();
        services.AddScoped<BillingSeatQuantityService>();
        services.AddScoped<BillingCustomerPortalService>();
        services.AddScoped<PostgresDeviceListingStore>();
        services.AddScoped<DeviceListingService>();
        services.AddScoped<PostgresDeviceRevocationStore>();
        services.AddScoped<DeviceRevocationService>();
        services.AddScoped<PostgresDeviceEntitlementCheckStore>();
        services.AddScoped<DeviceEntitlementCheckService>();
        services.AddScoped<PostgresDesktopDeviceAuthorizationStore>();
        services.AddScoped<DesktopDeviceAuthorizationService>();
        services.AddScoped<PostgresUserSignupStore>();
        services.AddScoped<UserSignupService>();
        services.AddScoped<ExternalAccountService>();
        services.AddScoped<PostgresEntitlementStore>();
        services.AddScoped<EntitlementResolutionService>();
        services.AddScoped<PostgresOrganizationStore>();
        services.AddScoped<OrganizationCreationService>();
        services.AddScoped<PostgresOrganizationListingStore>();
        services.AddScoped<OrganizationListingService>();
        services.AddScoped<PostgresOrganizationMemberListingStore>();
        services.AddScoped<OrganizationMemberListingService>();
        services.AddScoped<PostgresOrganizationMemberRemovalStore>();
        services.AddScoped<OrganizationMemberRemovalService>();
        services.AddScoped<PostgresOrganizationSeatAssignmentStore>();
        services.AddScoped<OrganizationSeatAssignmentService>();
        services.AddScoped<PostgresOrganizationRoleManagementStore>();
        services.AddScoped<OrganizationRoleManagementService>();
        services.AddScoped<PostgresOrganizationInvitationListingStore>();
        services.AddScoped<OrganizationInvitationListingService>();
        services.AddScoped<PostgresOrganizationInvitationCreationStore>();
        services.AddScoped<OrganizationInvitationCreationService>();
        services.AddScoped<PostgresOrganizationInvitationCancellationStore>();
        services.AddScoped<OrganizationInvitationCancellationService>();
        services.AddScoped<PostgresOrganizationInvitationResendStore>();
        services.AddScoped<OrganizationInvitationResendService>();
        services.AddScoped<PostgresOrganizationInvitationAcceptanceStore>();
        services.AddScoped<OrganizationInvitationAcceptanceService>();
        services.AddScoped<PostgresBillingWebhookInboxStore>();
        services.AddScoped<BillingWebhookIngestionService>();
        ConfigureStripeOptions(
            services,
            configuration,
            requireApiConfiguration: true);
        ConfigureStripeClient(services);
        services.AddSingleton<IBillingCheckoutProvider, StripeBillingCheckoutProvider>();
        services.AddSingleton<IBillingPriceProvider, StripeBillingPriceProvider>();
        services.AddSingleton<
            IBillingCustomerPortalProvider,
            StripeBillingCustomerPortalProvider>();
        services.AddScoped<StripeCommercialSubscriptionProvider>();
        services.AddScoped<ICommercialSubscriptionProvider>(serviceProvider =>
            serviceProvider.GetRequiredService<StripeCommercialSubscriptionProvider>());
        services.AddScoped<IBillingSeatQuantityProvider>(serviceProvider =>
            serviceProvider.GetRequiredService<StripeCommercialSubscriptionProvider>());
        services.AddSingleton<IBillingWebhookVerifier, StripeBillingWebhookVerifier>();
        services.AddScoped<DesktopProtocolTransaction>();
        services.AddSingleton<
            IAuthorizationMiddlewareResultHandler,
            DesktopAuthorizationResultHandler>();
        services.AddSingleton<CryptographicOrganizationInvitationSecretGenerator>();
        ConfigureInvitationDeliveryProtection(services, configuration);
        var desktopCertificates =
            DesktopProtocolCertificateConfiguration.LoadCertificateRing(configuration);
        var desktopIssuer =
            DesktopProtocolCertificateConfiguration.LoadIssuer(configuration);
        services.AddSingleton(desktopCertificates);
        services.AddSingleton(desktopIssuer);
        services.AddSingleton(serviceProvider => new DesktopLeaseSigner(
            serviceProvider.GetRequiredService<DesktopProtocolCertificateRing>(),
            serviceProvider.GetRequiredService<Uri>()));
        services.AddDesktopProtocol(
            desktopIssuer,
            desktopCertificates.Current,
            desktopCertificates.Previous);
        ConfigureApiSecurity(services, configuration);
    }

    private static void ConfigureBackgroundWorkServices(
        IServiceCollection services,
        ConfigurationManager configuration)
    {
        services.AddScoped<PostgresBillingWebhookProcessingStore>();
        services.AddScoped<BillingWebhookProcessingService>();
        services.AddScoped<PostgresOrganizationInvitationDeliveryStore>();
        services.AddScoped<OrganizationInvitationDeliveryService>();
        services.AddScoped<PostgresInfrastructureSmokeProbeStore>();
        services.AddScoped<InfrastructureSmokeProbeProcessor>();
        var smokeProbeDelaySeconds = configuration.GetValue<int?>(
            "InfrastructureSmoke:ProcessingDelaySeconds") ?? 120;
        if (smokeProbeDelaySeconds is < 0 or > 780)
        {
            throw new InvalidOperationException(
                "InfrastructureSmoke:ProcessingDelaySeconds must be between 0 and 780.");
        }
        services.AddSingleton(new InfrastructureSmokeProbeSettings(
            TimeSpan.FromSeconds(smokeProbeDelaySeconds)));
    }

    private static void ConfigureStripeOptions(
        IServiceCollection services,
        ConfigurationManager configuration,
        bool requireApiConfiguration)
    {
        var options = services.AddOptions<StripeBillingOptions>()
            .Bind(configuration.GetSection(StripeBillingOptions.SectionName))
            .Validate(
                options => !string.IsNullOrWhiteSpace(options.SecretKey),
                $"{StripeBillingOptions.SectionName}:SecretKey is required.")
            .Validate(
                options => StripeBillingOptions.IsCanonicalPriceId(options.MonthlyPriceId),
                $"{StripeBillingOptions.SectionName}:MonthlyPriceId is required and cannot "
                    + "contain surrounding whitespace.")
            .Validate(
                options => StripeBillingOptions.IsCanonicalPriceId(options.AnnualPriceId),
                $"{StripeBillingOptions.SectionName}:AnnualPriceId is required and cannot "
                    + "contain surrounding whitespace.")
            .Validate(
                options => !string.Equals(
                    options.MonthlyPriceId,
                    options.AnnualPriceId,
                    StringComparison.Ordinal),
                $"{StripeBillingOptions.SectionName}:MonthlyPriceId and "
                    + $"{StripeBillingOptions.SectionName}:AnnualPriceId must differ.");
        if (requireApiConfiguration)
        {
            options
                .Validate(
                    value => !string.IsNullOrWhiteSpace(value.WebhookSecret),
                    $"{StripeBillingOptions.SectionName}:WebhookSecret is required.")
                .Validate(
                    value => StripeBillingOptions.IsAbsoluteHttpsUri(
                        value.CheckoutSuccessUrl),
                    $"{StripeBillingOptions.SectionName}:CheckoutSuccessUrl must be an "
                        + "absolute HTTPS URI.")
                .Validate(
                    value => StripeBillingOptions.IsAbsoluteHttpsUri(
                        value.CheckoutCancelUrl),
                    $"{StripeBillingOptions.SectionName}:CheckoutCancelUrl must be an "
                        + "absolute HTTPS URI.")
                .Validate(
                    value => StripeBillingOptions.IsCanonicalExternalIdentifier(
                        value.CustomerPortalConfigurationId),
                    $"{StripeBillingOptions.SectionName}:CustomerPortalConfigurationId "
                        + "is required, bounded, and cannot contain surrounding whitespace.")
                .Validate(
                    value => StripeBillingOptions.IsAbsoluteHttpsUri(
                        value.CustomerPortalReturnUrl),
                    $"{StripeBillingOptions.SectionName}:CustomerPortalReturnUrl must be an "
                        + "absolute HTTPS URI.");
        }

        options.ValidateOnStart();
    }

    private static void ConfigureStripeClient(IServiceCollection services)
    {
        services.AddSingleton<IStripeClient>(serviceProvider =>
        {
            var options = serviceProvider.GetRequiredService<
                IOptions<StripeBillingOptions>>().Value;
            if (string.IsNullOrWhiteSpace(options.SecretKey))
            {
                throw new InvalidOperationException(
                    $"{StripeBillingOptions.SectionName}:SecretKey is required.");
            }

            return new StripeClient(options.SecretKey);
        });
    }

    private static void ConfigureBackgroundMessaging(
        IServiceCollection services,
        string connectionString,
        BackgroundMessagingSettings settings)
    {
        services.AddScoped<NativeOutboxUpgrade>();
        services.AddScoped<BackgroundWorkRecoveryService>();
        RebusBackgroundMessagingConfiguration.Add(
            services,
            connectionString,
            settings);
        services.AddHostedService<StartupBackgroundWorkRecoveryService>();
    }

    private static void ConfigureInvitationDeliveryProtection(
        IServiceCollection services,
        IConfiguration configuration)
    {
        DataProtectionConfiguration.Configure(services, configuration);
        services.AddSingleton<DataProtectionOrganizationInvitationDeliveryProtector>();
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

    private static void ConfigureInvitationEmail(
        IServiceCollection services,
        ConfigurationManager configuration)
    {
        var section = configuration.GetSection(InvitationEmailSettings.SectionName);
        if (!Uri.TryCreate(section["AcceptanceUrl"], UriKind.Absolute, out var acceptanceUrl))
        {
            throw new InvalidOperationException(
                $"{InvitationEmailSettings.SectionName}:AcceptanceUrl must be an absolute URI.");
        }

        var settings = new InvitationEmailSettings(
            section["FromAddress"]
                ?? throw new InvalidOperationException(
                    $"{InvitationEmailSettings.SectionName}:FromAddress is required."),
            acceptanceUrl);
        var smtp = new SmtpEmailSettings(
            section["Smtp:Host"]
                ?? throw new InvalidOperationException(
                    $"{InvitationEmailSettings.SectionName}:Smtp:Host is required."),
            section.GetValue<int?>("Smtp:Port")
                ?? throw new InvalidOperationException(
                    $"{InvitationEmailSettings.SectionName}:Smtp:Port is required."),
            section["Smtp:Username"]
                ?? throw new InvalidOperationException(
                    $"{InvitationEmailSettings.SectionName}:Smtp:Username is required."),
            section["Smtp:Password"]
                ?? throw new InvalidOperationException(
                    $"{InvitationEmailSettings.SectionName}:Smtp:Password is required."));

        services.AddSingleton(settings);
        services.AddSingleton(smtp);
        services.AddSingleton<IEmailSubmissionClient, SmtpEmailSubmissionClient>();
        services.AddSingleton<
            IOrganizationInvitationEmailSender,
            OrganizationInvitationEmailSender>();
    }

    private static void UsePortalFallback(WebApplication application)
    {
        application.Use(async (context, next) =>
        {
            await next(context);
            if (context.Response.HasStarted
                || context.Response.StatusCode != StatusCodes.Status404NotFound
                || IsBackendPath(context.Request.Path))
            {
                return;
            }

            var webRoot = application.Environment.WebRootPath
                ?? throw new InvalidOperationException("The portal web root is not configured.");
            context.Response.StatusCode = StatusCodes.Status200OK;
            await Results.File(
                Path.Combine(webRoot, "index.html"),
                contentType: "text/html; charset=utf-8").ExecuteAsync(context);
        });
    }

    internal static bool IsBackendPath(PathString path)
    {
        return path.StartsWithSegments("/api")
            || path.StartsWithSegments("/auth")
            || path.StartsWithSegments("/desktop")
            || path.StartsWithSegments("/health");
    }
}
