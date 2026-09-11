using System.Security.Claims;
using System.Collections.Concurrent;
using System.Text.Encodings.Web;
using Microsoft.AspNetCore.Authentication;
using Microsoft.AspNetCore.Hosting;
using Microsoft.AspNetCore.Mvc.Testing;
using Microsoft.AspNetCore.TestHost;
using Microsoft.EntityFrameworkCore.Diagnostics;
using Microsoft.Extensions.Configuration;
using Microsoft.Extensions.DependencyInjection;
using Microsoft.Extensions.DependencyInjection.Extensions;
using Microsoft.Extensions.Logging;
using Microsoft.Extensions.Options;
using OpenIddict.Abstractions;
using OpenIddict.Validation.AspNetCore;
using Styrhous.Licensing.Api.Desktop;
using Styrhous.Licensing.Api.Authentication;
using Styrhous.Licensing.Application.Billing;
using Styrhous.Licensing.Application.Messaging;
using Styrhous.Licensing.Domain.Signups;
using Styrhous.Licensing.Infrastructure.Billing;
using Styrhous.Licensing.Infrastructure.DataProtection;
using Styrhous.Licensing.Persistence;
using Styrhous.Licensing.Tests.Infrastructure;
using Styrhous.Licensing.Tests.Persistence;

namespace Styrhous.Licensing.Tests.Api;

internal sealed class LicensingWebApplicationFactory(
    PostgresTestDatabase database,
    DateTimeOffset observedAt,
    bool useTestAuthentication = true,
    RequestCompletionObserver? requestCompletionObserver = null,
    RabbitMqApiMessaging? rabbitMqMessaging = null,
    IBillingCheckoutProvider? billingCheckoutProvider = null,
    IBillingCustomerPortalProvider? billingCustomerPortalProvider = null,
    IBillingSeatQuantityProvider? billingSeatQuantityProvider = null,
    TestCertificateRing? certificateRing = null,
    TestCertificateRing? desktopCertificateRing = null,
    string desktopIssuer = "https://localhost/",
    TestLogCollector? logCollector = null,
    bool useTestExternalProviders = false,
    bool configureExternalProviders = false,
    bool useScopeLessDesktopAuthentication = false,
    params IInterceptor[] interceptors)
    : WebApplicationFactory<Program>
{
    public const string AuthenticationScheme = "TestAuthentication";
    public const string UserIdHeader = "X-Test-User-Id";
    public const string ExternalSubjectHeader = "X-Test-External-Subject";
    public const string ExternalClaimsModeHeader = "X-Test-External-Claims-Mode";
    public const string ExternalOperationHeader = "X-Test-External-Operation";
    public const string ExternalEmailHeader = "X-Test-External-Email";
    public const string StripeSecretKey = "sk_test_application_factory";
    public const string StripeWebhookSecret = "whsec_test_webhook_secret";
    private readonly TestCertificateRing _certificateRing = certificateRing
        ?? new TestCertificateRing(TestDataProtectionCertificate.EncodedCertificate, []);
    private readonly TestCertificateRing _desktopCertificateRing =
        desktopCertificateRing
        ?? new TestCertificateRing(
            TestDataProtectionCertificate.EncodedDesktopCertificate,
            []);

    public HttpClient CreateApiClient(Guid? userId = null)
    {
        var client = CreateClient(
            new WebApplicationFactoryClientOptions
            {
                BaseAddress = new Uri("https://localhost"),
                AllowAutoRedirect = false,
                HandleCookies = true,
            });
        if (userId is not null)
        {
            client.DefaultRequestHeaders.Add(UserIdHeader, userId.Value.ToString());
        }

        return client;
    }

    protected override void ConfigureWebHost(IWebHostBuilder builder)
    {
        var configurationValues = ConfigurationValues();
        builder.UseEnvironment("Testing");
        if (logCollector is not null)
        {
            builder.ConfigureLogging(logging => logging.AddProvider(logCollector));
        }

        foreach (var (key, value) in configurationValues)
        {
            builder.UseSetting(key, value!);
        }

        builder.ConfigureAppConfiguration(
            (_, configuration) =>
                configuration.AddInMemoryCollection(configurationValues));
        builder.ConfigureTestServices(services =>
        {
            services.RemoveAll<TimeProvider>();
            services.AddSingleton<TimeProvider>(new FixedTimeProvider(observedAt));
            foreach (var interceptor in interceptors)
            {
                services.AddSingleton(interceptor);
            }

            if (requestCompletionObserver is not null)
            {
                services.AddSingleton<IStartupFilter>(requestCompletionObserver);
            }

            if (billingCheckoutProvider is not null)
            {
                services.RemoveAll<IBillingCheckoutProvider>();
                services.AddSingleton(billingCheckoutProvider);
            }

            if (billingCustomerPortalProvider is not null)
            {
                services.RemoveAll<IBillingCustomerPortalProvider>();
                services.AddSingleton(billingCustomerPortalProvider);
            }

            if (billingSeatQuantityProvider is not null)
            {
                services.RemoveAll<IBillingSeatQuantityProvider>();
                services.AddSingleton(billingSeatQuantityProvider);
            }

            if (useTestAuthentication)
            {
                services.AddAuthentication(options =>
                    {
                        options.DefaultAuthenticateScheme = AuthenticationScheme;
                        options.DefaultChallengeScheme = AuthenticationScheme;
                        options.DefaultForbidScheme = AuthenticationScheme;
                    })
                    .AddScheme<AuthenticationSchemeOptions, TestAuthenticationHandler>(
                        AuthenticationScheme,
                        _ => { });
            }

            if (useTestExternalProviders)
            {
                var authentication = services.AddAuthentication();
                foreach (var provider in AccountAuthenticationProviders.All)
                {
                    authentication.AddScheme<
                        AuthenticationSchemeOptions,
                        TestExternalAuthenticationHandler>(provider.Scheme, _ => { });
                }
            }

            if (useScopeLessDesktopAuthentication)
            {
                services.AddAuthentication().AddScheme<
                    AuthenticationSchemeOptions,
                    ScopeLessDesktopAuthenticationHandler>(
                    ScopeLessDesktopAuthenticationHandler.Scheme,
                    _ => { });
                services.Configure<OpenIddictValidationAspNetCoreOptions>(
                    OpenIddictValidationAspNetCoreDefaults.AuthenticationScheme,
                    options => options.ForwardAuthenticate =
                        ScopeLessDesktopAuthenticationHandler.Scheme);
            }
        });
    }

    private Dictionary<string, string?> ConfigurationValues()
    {
        var values = new Dictionary<string, string?>
        {
            ["ConnectionStrings:Licensing"] = database.ConnectionString,
            [DataProtectionConfiguration.CertificateConfigurationKey] =
                _certificateRing.Current,
            [DataProtectionConfiguration.CertificatePasswordConfigurationKey] =
                TestDataProtectionCertificate.Password,
            [DesktopProtocolCertificateConfiguration.CertificateConfigurationKey] =
                _desktopCertificateRing.Current,
            [DesktopProtocolCertificateConfiguration.CertificatePasswordConfigurationKey] =
                TestDataProtectionCertificate.Password,
            [DesktopProtocolCertificateConfiguration.IssuerConfigurationKey] =
                desktopIssuer,
            [$"{StripeBillingOptions.SectionName}:WebhookSecret"] =
                StripeWebhookSecret,
            [$"{StripeBillingOptions.SectionName}:SecretKey"] = StripeSecretKey,
            [$"{StripeBillingOptions.SectionName}:MonthlyPriceId"] =
                "price_test_monthly",
            [$"{StripeBillingOptions.SectionName}:AnnualPriceId"] =
                "price_test_annual",
            [$"{StripeBillingOptions.SectionName}:CheckoutSuccessUrl"] =
                "https://localhost/billing?checkout=success",
            [$"{StripeBillingOptions.SectionName}:CheckoutCancelUrl"] =
                "https://localhost/billing?checkout=cancelled",
            [$"{StripeBillingOptions.SectionName}:CustomerPortalConfigurationId"] =
                "bpc_test_styrhous",
            [$"{StripeBillingOptions.SectionName}:CustomerPortalReturnUrl"] =
                "https://localhost/billing",
            ["Messaging:QueueName"] = rabbitMqMessaging?.QueueName ?? database.DatabaseName,
            ["Messaging:ApiOutboxForwardingEnabled"] =
                (rabbitMqMessaging is not null).ToString(),
        };
        for (var index = 0; index < _certificateRing.Previous.Count; index++)
        {
            values[$"{DataProtectionConfiguration.PreviousCertificatesConfigurationKey}:"
                + $"{index}:Certificate"] = _certificateRing.Previous[index];
            values[$"{DataProtectionConfiguration.PreviousCertificatesConfigurationKey}:"
                + $"{index}:CertificatePassword"] = TestDataProtectionCertificate.Password;
        }
        for (var index = 0; index < _desktopCertificateRing.Previous.Count; index++)
        {
            values[$"{DesktopProtocolCertificateConfiguration.PreviousCertificatesConfigurationKey}:"
                + $"{index}:Certificate"] = _desktopCertificateRing.Previous[index];
            values[$"{DesktopProtocolCertificateConfiguration.PreviousCertificatesConfigurationKey}:"
                + $"{index}:CertificatePassword"] =
                TestDataProtectionCertificate.Password;
        }
        if (rabbitMqMessaging is not null)
        {
            values["Messaging:Transport"] = "RabbitMq";
            values["Messaging:QueueName"] = rabbitMqMessaging.QueueName;
            values["Messaging:ErrorQueueName"] =
                $"{rabbitMqMessaging.QueueName}-error";
            values["Messaging:RabbitMq:ConnectionString"] =
                rabbitMqMessaging.ConnectionString;
        }
        if (configureExternalProviders)
        {
            values["Authentication:GitHub:ClientId"] = "github-client";
            values["Authentication:GitHub:ClientSecret"] = "github-secret";
            values["Authentication:Google:ClientId"] = "google-client";
            values["Authentication:Google:ClientSecret"] = "google-secret";
            values["Authentication:Microsoft:ClientId"] = "microsoft-client";
            values["Authentication:Microsoft:ClientSecret"] = "microsoft-secret";
        }

        return values;
    }

    private sealed class TestAuthenticationHandler(
        IOptionsMonitor<AuthenticationSchemeOptions> options,
        ILoggerFactory logger,
        UrlEncoder encoder)
        : AuthenticationHandler<AuthenticationSchemeOptions>(options, logger, encoder)
    {
        protected override Task<AuthenticateResult> HandleAuthenticateAsync()
        {
            if (!Request.Headers.TryGetValue(UserIdHeader, out var userIdValues))
            {
                return Task.FromResult(AuthenticateResult.NoResult());
            }

            if (!Guid.TryParse(userIdValues.SingleOrDefault(), out var userId)
                || userId == Guid.Empty)
            {
                return Task.FromResult(
                    AuthenticateResult.Fail("The test user identifier is invalid."));
            }

            var identity = new ClaimsIdentity(
                new[]
                {
                    new Claim(ClaimTypes.NameIdentifier, userId.ToString()),
                },
                Scheme.Name);
            return Task.FromResult(
                AuthenticateResult.Success(
                    new AuthenticationTicket(new ClaimsPrincipal(identity), Scheme.Name)));
        }
    }

    private sealed class TestExternalAuthenticationHandler(
        IOptionsMonitor<AuthenticationSchemeOptions> options,
        ILoggerFactory logger,
        UrlEncoder encoder)
        : AuthenticationHandler<AuthenticationSchemeOptions>(options, logger, encoder)
    {
        protected override Task<AuthenticateResult> HandleAuthenticateAsync()
        {
            return Task.FromResult(AuthenticateResult.NoResult());
        }

        protected override async Task HandleChallengeAsync(
            AuthenticationProperties properties)
        {
            if (Request.Headers[ExternalOperationHeader].SingleOrDefault() is { } operation)
            {
                properties.Items[AccountAuthentication.OperationProperty] = operation;
            }

            var claimsMode = Request.Headers[ExternalClaimsModeHeader].SingleOrDefault();
            var subject = string.Equals(
                claimsMode,
                "oversized-subject",
                StringComparison.Ordinal)
                    ? new string('s', VerifiedExternalIdentity.MaximumSubjectLength + 1)
                    : Request.Headers[ExternalSubjectHeader].SingleOrDefault()
                        ?? $"{Scheme.Name}-test-subject";
            var claims = new List<Claim>();
            if (!string.Equals(claimsMode, "missing-subject", StringComparison.Ordinal))
            {
                claims.Add(new Claim(ClaimTypes.NameIdentifier, subject));
            }
            if (!string.Equals(
                    claimsMode,
                    "missing-verified-email",
                    StringComparison.Ordinal))
            {
                var email = string.Equals(
                    claimsMode,
                    "malformed-verified-email",
                    StringComparison.Ordinal)
                        ? "not-an-email"
                        : Request.Headers[ExternalEmailHeader].SingleOrDefault()
                            ?? "external@example.com";
                claims.Add(new Claim(
                    AccountAuthentication.VerifiedEmailClaim,
                    email));
            }

            var identity = new ClaimsIdentity(claims, Scheme.Name);
            await Context.SignInAsync(
                AccountAuthentication.ExternalScheme,
                new ClaimsPrincipal(identity),
                properties);
            Response.Redirect(properties.RedirectUri ?? "/");
        }
    }

    private sealed class ScopeLessDesktopAuthenticationHandler(
        IOptionsMonitor<AuthenticationSchemeOptions> options,
        ILoggerFactory logger,
        UrlEncoder encoder)
        : AuthenticationHandler<AuthenticationSchemeOptions>(options, logger, encoder)
    {
        public new const string Scheme = "ScopeLessDesktopAuthentication";

        protected override Task<AuthenticateResult> HandleAuthenticateAsync()
        {
            var identity = new ClaimsIdentity(
                [
                    new Claim(
                        OpenIddictConstants.Claims.Subject,
                        Guid.CreateVersion7().ToString()),
                ],
                Scheme);
            return Task.FromResult(AuthenticateResult.Success(
                new AuthenticationTicket(new ClaimsPrincipal(identity), Scheme)));
        }
    }
}

internal sealed record RabbitMqApiMessaging(
    string QueueName,
    string ConnectionString);

internal sealed record TestCertificateRing(
    string Current,
    IReadOnlyList<string> Previous);

internal sealed class TestLogCollector : ILoggerProvider
{
    private readonly ConcurrentQueue<string> _messages = new();

    public IReadOnlyCollection<string> Messages => _messages.ToArray();

    public ILogger CreateLogger(string categoryName)
    {
        return new CollectorLogger(_messages);
    }

    public void Dispose()
    {
    }

    public void Clear()
    {
        while (_messages.TryDequeue(out _))
        {
        }
    }

    private sealed class CollectorLogger(ConcurrentQueue<string> messages) : ILogger
    {

        public IDisposable? BeginScope<TState>(TState state)
            where TState : notnull
        {
            return null;
        }

        public bool IsEnabled(LogLevel logLevel)
        {
            return true;
        }

        public void Log<TState>(
            LogLevel logLevel,
            EventId eventId,
            TState state,
            Exception? exception,
            Func<TState, Exception?, string> formatter)
        {
            messages.Enqueue(formatter(state, exception));
        }
    }
}
