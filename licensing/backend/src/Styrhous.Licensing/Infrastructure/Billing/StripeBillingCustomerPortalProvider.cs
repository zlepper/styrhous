using System.Net;
using Microsoft.Extensions.Options;
using Stripe;
using Stripe.BillingPortal;
using Styrhous.Licensing.Application.Billing;
using Styrhous.Licensing.Domain.Billing;

namespace Styrhous.Licensing.Infrastructure.Billing;

public sealed partial class StripeBillingCustomerPortalProvider(
    IStripeClient stripeClient,
    IOptions<StripeBillingOptions> options,
    ILogger<StripeBillingCustomerPortalProvider> logger)
    : IBillingCustomerPortalProvider
{
    private readonly ConfigurationService _configurations = new(stripeClient);
    private readonly SessionService _sessions = new(stripeClient);
    private readonly StripeBillingOptions _options = options.Value;

    public async Task<BillingCustomerPortalProviderSession> CreateSessionAsync(
        string externalCustomerId,
        CancellationToken cancellationToken)
    {
        var customerId = StripeBillingOptions.RequireCanonicalExternalIdentifier(
            externalCustomerId,
            nameof(externalCustomerId));
        var configurationId = StripeBillingOptions.RequireCanonicalExternalIdentifier(
            _options.CustomerPortalConfigurationId,
            nameof(_options.CustomerPortalConfigurationId));
        var returnUri = StripeBillingOptions.RequireAbsoluteHttpsUri(
            _options.CustomerPortalReturnUrl,
            nameof(_options.CustomerPortalReturnUrl));
        Session session;
        try
        {
            var configuration = await _configurations.GetAsync(
                configurationId,
                options: null,
                requestOptions: null,
                cancellationToken);
            EnsureRestrictedConfiguration(configuration, configurationId);
            session = await _sessions.CreateAsync(
                new SessionCreateOptions
                {
                    Configuration = configurationId,
                    Customer = customerId,
                    ReturnUrl = returnUri.ToString(),
                },
                requestOptions: null,
                cancellationToken);
        }
        catch (TaskCanceledException exception) when (!cancellationToken.IsCancellationRequested)
        {
            LogTransientProviderFailure(logger, "timeout");
            throw ProviderUnavailable(exception);
        }
        catch (HttpRequestException exception)
        {
            LogTransientProviderFailure(logger, "network");
            throw ProviderUnavailable(exception);
        }
        catch (StripeException exception) when (
            StripeBillingFailureClassifier.IsTransient(exception.HttpStatusCode))
        {
            LogTransientStripeFailure(
                logger,
                exception.HttpStatusCode,
                exception.StripeError?.Type,
                exception.StripeError?.Code,
                exception.StripeResponse?.RequestId);
            throw ProviderUnavailable(exception);
        }
        catch (StripeException exception)
        {
            LogPermanentStripeRejection(
                logger,
                exception.HttpStatusCode,
                exception.StripeError?.Type,
                exception.StripeError?.Code,
                exception.StripeResponse?.RequestId);
            throw new InvalidOperationException(
                "Stripe rejected the configured Customer Portal or customer projection.");
        }

        if (string.IsNullOrWhiteSpace(session.Id)
            || session.Id.Length > CommercialSubscription.MaximumExternalIdentifierLength)
        {
            throw new InvalidOperationException(
                "Stripe returned an invalid Customer Portal session identifier.");
        }

        if (!string.Equals(session.Customer, customerId, StringComparison.Ordinal)
            || !string.Equals(
                session.ConfigurationId,
                configurationId,
                StringComparison.Ordinal)
            || !string.Equals(
                session.ReturnUrl,
                returnUri.ToString(),
                StringComparison.Ordinal))
        {
            throw new InvalidOperationException(
                "Stripe returned a Customer Portal session for different configured inputs.");
        }

        if (!Uri.TryCreate(session.Url, UriKind.Absolute, out var redirectUri)
            || !string.Equals(
                redirectUri.Scheme,
                Uri.UriSchemeHttps,
                StringComparison.OrdinalIgnoreCase))
        {
            throw new InvalidOperationException(
                "Stripe returned an invalid Customer Portal redirect URI.");
        }

        return new BillingCustomerPortalProviderSession(redirectUri);
    }

    private static void EnsureRestrictedConfiguration(
        Configuration configuration,
        string expectedConfigurationId)
    {
        if (!string.Equals(
                configuration.Id,
                expectedConfigurationId,
                StringComparison.Ordinal)
            || !configuration.Active
            || configuration.Features?.SubscriptionUpdate is null
            || configuration.Features.SubscriptionUpdate.Enabled)
        {
            throw new InvalidOperationException(
                "The configured Stripe Customer Portal must be active and disable "
                    + "subscription updates.");
        }
    }

    [LoggerMessage(
        EventId = 2401,
        Level = LogLevel.Error,
        Message = "Stripe permanently rejected Customer Portal session creation with status "
            + "{StripeStatusCode}, error type {StripeErrorType}, error code {StripeErrorCode}, "
            + "and request {StripeRequestId}.")]
    private static partial void LogPermanentStripeRejection(
        ILogger logger,
        HttpStatusCode stripeStatusCode,
        string? stripeErrorType,
        string? stripeErrorCode,
        string? stripeRequestId);

    [LoggerMessage(
        EventId = 2402,
        Level = LogLevel.Warning,
        Message = "Stripe Customer Portal session creation had a transient {FailureKind} "
            + "failure.")]
    private static partial void LogTransientProviderFailure(
        ILogger logger,
        string failureKind);

    [LoggerMessage(
        EventId = 2403,
        Level = LogLevel.Warning,
        Message = "Stripe transiently rejected Customer Portal session creation with status "
            + "{StripeStatusCode}, error type {StripeErrorType}, error code {StripeErrorCode}, "
            + "and request {StripeRequestId}.")]
    private static partial void LogTransientStripeFailure(
        ILogger logger,
        HttpStatusCode stripeStatusCode,
        string? stripeErrorType,
        string? stripeErrorCode,
        string? stripeRequestId);

    private static BillingCustomerPortalProviderUnavailableException ProviderUnavailable(
        Exception exception)
    {
        return new("Stripe Customer Portal is temporarily unavailable.", exception);
    }
}
