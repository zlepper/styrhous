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
    private static readonly JsonSerializerOptions WebJson =
        new(JsonSerializerDefaults.Web);
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
            case LicensingRuntimeMode.Worker:
                await BuildWorker(
                    [.. command.HostArguments],
                    applicationConfiguration).RunAsync();
                return;
            case LicensingRuntimeMode.Maintenance:
                await RunMaintenanceAsync(
                    [.. command.HostArguments],
                    applicationConfiguration);
                return;
            case LicensingRuntimeMode.MaintenanceLambda:
                await BuildMaintenanceApplication(
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
        ConfigureApiServices(builder.Services, builder.Configuration);
        ConfigureApiBackgroundMessaging(builder.Services, builder.Configuration);
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
        application.UseMiddleware<DesktopProtocolTransactionMiddleware>();
        application.UseAuthentication();
        application.UseAuthorization();
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

    internal static IHost BuildWorker(string[] args)
    {
        return BuildWorker(args, ApplicationConfigurationLoader.ReadInline());
    }

    internal static IHost BuildWorker(
        string[] args,
        string? applicationConfiguration,
        Action<HostApplicationBuilder>? configureBuilder = null)
    {
        var builder = Host.CreateApplicationBuilder(args);
        ConfigureStructuredLogging(builder.Logging);
        AddApplicationConfiguration(
            builder.Configuration,
            applicationConfiguration);
        ConfigurePersistence(builder.Services, builder.Configuration);
        ConfigureWorkerServices(builder.Services, builder.Configuration);
        ConfigureInvitationEmail(builder.Services, builder.Configuration);
        RebusBackgroundMessagingConfiguration.Add(
            builder.Services,
            builder.Configuration,
            receiveMessages: true);
        configureBuilder?.Invoke(builder);
        return builder.Build();
    }

    internal static IHost BuildMaintenanceHost(string[] args)
    {
        return BuildMaintenanceHost(args, ApplicationConfigurationLoader.ReadInline());
    }

    internal static IHost BuildMaintenanceHost(
        string[] args,
        string? applicationConfiguration,
        Action<HostApplicationBuilder>? configureBuilder = null)
    {
        var builder = Host.CreateApplicationBuilder(args);
        ConfigureStructuredLogging(builder.Logging);
        AddApplicationConfiguration(
            builder.Configuration,
            applicationConfiguration);
        ConfigurePersistence(builder.Services, builder.Configuration);
        ConfigureMaintenanceServices(builder.Services);
        RebusBackgroundMessagingConfiguration.Add(
            builder.Services,
            builder.Configuration,
            receiveMessages: false);
        configureBuilder?.Invoke(builder);
        return builder.Build();
    }

    internal static WebApplication BuildMaintenanceApplication(
        string[] args,
        string? applicationConfiguration,
        Action<WebApplicationBuilder>? configureBuilder = null)
    {
        var builder = WebApplication.CreateBuilder(args);
        configureBuilder?.Invoke(builder);
        ConfigureStructuredLogging(builder.Logging);
        AddApplicationConfiguration(
            builder.Configuration,
            applicationConfiguration);
        ConfigurePersistence(builder.Services, builder.Configuration);
        ConfigureMaintenanceServices(builder.Services);
        RebusBackgroundMessagingConfiguration.Add(
            builder.Services,
            builder.Configuration,
            receiveMessages: false);
        var application = builder.Build();
        application.MapPost(
            "/internal/lambda/maintenance",
            async (
                HttpRequest request,
                BackgroundWorkRecoveryService service,
                PostgresInfrastructureSmokeProbeStore smokeProbeStore,
                CancellationToken cancellationToken) =>
            {
                var smokeRequest = await ReadMaintenanceRequestAsync(
                    request,
                    cancellationToken);
                InfrastructureSmokeProbeStatus? smokeProbe = null;
                if (smokeRequest?.SmokeTestId is { } probeId)
                {
                    smokeProbe = await smokeProbeStore.SeedAsync(
                        probeId,
                        cancellationToken);
                }

                var recovery = await service.RecoverAsync(cancellationToken);
                var statusProbeId = smokeRequest?.SmokeTestStatusId
                    ?? smokeRequest?.SmokeTestId;
                if (statusProbeId is { } statusId)
                {
                    smokeProbe = await smokeProbeStore.FindAsync(
                        statusId,
                        cancellationToken);
                }

                return new MaintenanceExecutionResult(
                    recovery.OutboxDrained,
                    smokeProbe);
            });
        application.MapGet(
            "/health",
            () => TypedResults.Ok(new { status = "healthy" }));
        return application;
    }

    internal static Task<BackgroundWorkRecoveryResult> RunMaintenanceAsync(
        string[] args)
    {
        return RunMaintenanceAsync(args, ApplicationConfigurationLoader.ReadInline());
    }

    private static async Task<BackgroundWorkRecoveryResult> RunMaintenanceAsync(
        string[] args,
        string? applicationConfiguration)
    {
        using var host = BuildMaintenanceHost(args, applicationConfiguration);
        await host.StartAsync();
        try
        {
            await using var scope = host.Services.CreateAsyncScope();
            var lifetime = host.Services.GetRequiredService<IHostApplicationLifetime>();
            return await scope.ServiceProvider
                .GetRequiredService<BackgroundWorkRecoveryService>()
                .RecoverAsync(lifetime.ApplicationStopping);
        }
        finally
        {
            await host.StopAsync(CancellationToken.None);
        }
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
        services.AddSingleton(serviceProvider => new PostgresBackgroundWorkOutbox(
            BackgroundMessagingSettings.QueueNameFrom(configuration)));
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

    private static void ConfigureWorkerServices(
        IServiceCollection services,
        ConfigurationManager configuration)
    {
        services.AddHostedService<LicensingStartupValidation>();
        services.AddScoped<PostgresCommercialSubscriptionProjectionStore>();
        services.AddScoped<CommercialSubscriptionProjectionService>();
        services.AddScoped<PostgresBillingProviderReadRevisionSource>();
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
        ConfigureStripeOptions(
            services,
            configuration,
            requireApiConfiguration: false);
        ConfigureStripeClient(services);
        services.AddScoped<StripeCommercialSubscriptionProvider>();
        services.AddScoped<ICommercialSubscriptionProvider>(serviceProvider =>
            serviceProvider.GetRequiredService<StripeCommercialSubscriptionProvider>());
        ConfigureInvitationDeliveryProtection(services, configuration);
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

    private static void ConfigureApiBackgroundMessaging(
        IServiceCollection services,
        IConfiguration configuration)
    {
        if (BackgroundMessagingSettings.ApiOutboxForwardingEnabled(configuration))
        {
            RebusBackgroundMessagingConfiguration.Add(services, configuration, receiveMessages: false);
        }
    }

    private static void ConfigureMaintenanceServices(IServiceCollection services)
    {
        services.AddScoped<NativeOutboxUpgrade>();
        services.AddScoped<BackgroundWorkRecoveryService>();
        services.AddScoped<PostgresInfrastructureSmokeProbeStore>();
    }

    private static async Task<MaintenanceRequest?> ReadMaintenanceRequestAsync(
        HttpRequest request,
        CancellationToken cancellationToken)
    {
        if (request.ContentLength == 0)
        {
            return null;
        }

        using var reader = new StreamReader(
            request.Body,
            Encoding.UTF8,
            detectEncodingFromByteOrderMarks: false,
            leaveOpen: true);
        var body = await reader.ReadToEndAsync(cancellationToken);
        return string.IsNullOrWhiteSpace(body)
            ? null
            : JsonSerializer.Deserialize<MaintenanceRequest>(body, WebJson);
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
        var amazonSesRegion = section["AmazonSes:Region"];
        if (string.IsNullOrWhiteSpace(amazonSesRegion))
        {
            throw new InvalidOperationException(
                $"{InvitationEmailSettings.SectionName}:AmazonSes:Region is required.");
        }

        services.AddSingleton(settings);
        services.AddSingleton<IAmazonSimpleEmailServiceV2>(
            _ => new AmazonSimpleEmailServiceV2Client(
                RegionEndpoint.GetBySystemName(amazonSesRegion.Trim())));
        services.AddSingleton<IEmailSubmissionClient, AmazonSesEmailSubmissionClient>();
        services.AddSingleton<
            IOrganizationInvitationEmailSender,
            SesOrganizationInvitationEmailSender>();
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

    private sealed record MaintenanceRequest(
        Guid? SmokeTestId,
        Guid? SmokeTestStatusId);

    private sealed record MaintenanceExecutionResult(
        bool OutboxDrained,
        InfrastructureSmokeProbeStatus? SmokeProbe);
}
