using Microsoft.Extensions.Options;
using Stripe;
using Stripe.Checkout;
using Styrhous.Licensing.Application.Billing;
using Styrhous.Licensing.Domain.Billing;

namespace Styrhous.Licensing.Infrastructure.Billing;

public sealed class StripeBillingCheckoutProvider(
    IStripeClient stripeClient,
    IOptions<StripeBillingOptions> options)
    : IBillingCheckoutProvider
{
    private readonly SessionService _sessions = new(stripeClient);
    private readonly StripeBillingOptions _options = options.Value;

    public async Task<BillingCheckoutProviderSession> CreateSessionAsync(
        BillingCheckoutProviderRequest request,
        CancellationToken cancellationToken)
    {
        var (monthlyPriceId, annualPriceId) = _options.GetValidatedPriceIds();

        var priceId = request.Cadence switch
        {
            BillingCadence.Monthly => monthlyPriceId,
            BillingCadence.Annual => annualPriceId,
            _ => throw new ArgumentOutOfRangeException(nameof(request)),
        };
        var successUri = StripeBillingOptions.RequireAbsoluteHttpsUri(
            _options.CheckoutSuccessUrl,
            nameof(_options.CheckoutSuccessUrl));
        var cancelUri = StripeBillingOptions.RequireAbsoluteHttpsUri(
            _options.CheckoutCancelUrl,
            nameof(_options.CheckoutCancelUrl));
        var billingOperationId = request.BillingOperationId.ToString();
        var billingAccountId = request.BillingAccountId.ToString();
        var createOptions = new SessionCreateOptions
        {
            Mode = "subscription",
            Customer = request.ExternalCustomerId,
            SuccessUrl = successUri.ToString(),
            CancelUrl = cancelUri.ToString(),
            ExpiresAt = request.ExpiresAt.UtcDateTime,
            ClientReferenceId = billingOperationId,
            AutomaticTax = new SessionAutomaticTaxOptions { Enabled = true },
            Metadata = new Dictionary<string, string>
            {
                [StripeBillingMetadata.BillingOperationIdKey] = billingOperationId,
            },
            LineItems =
            [
                new SessionLineItemOptions
                {
                    Price = priceId,
                    Quantity = request.SeatQuantity,
                },
            ],
            SubscriptionData = new SessionSubscriptionDataOptions
            {
                Metadata = new Dictionary<string, string>
                {
                    [StripeBillingMetadata.BillingAccountIdKey] =
                        billingAccountId,
                    [StripeBillingMetadata.BillingOperationIdKey] = billingOperationId,
                },
            },
        };
        Session session;
        try
        {
            session = await _sessions.CreateAsync(
                createOptions,
                new RequestOptions
                {
                    IdempotencyKey = $"styrhous-checkout-{request.BillingOperationId:N}",
                },
                cancellationToken);
        }
        catch (TaskCanceledException exception) when (!cancellationToken.IsCancellationRequested)
        {
            throw ProviderUnavailable(exception);
        }
        catch (HttpRequestException exception)
        {
            throw ProviderUnavailable(exception);
        }
        catch (StripeException exception) when (
            string.Equals(
                exception.StripeError?.Param,
                "expires_at",
                StringComparison.Ordinal))
        {
            throw new BillingCheckoutProviderSessionCreationRejectedException(
                "Stripe rejected the Checkout session expiry.",
                exception);
        }
        catch (StripeException exception)
        {
            throw ProviderUnavailable(exception);
        }

        if (string.IsNullOrWhiteSpace(session.Id)
            || session.Id.Length > BillingOperation.MaximumExternalIdentifierLength)
        {
            throw new InvalidOperationException(
                "Stripe returned an invalid Checkout session identifier.");
        }

        if (!Uri.TryCreate(session.Url, UriKind.Absolute, out var redirectUri)
            || !string.Equals(
                redirectUri.Scheme,
                Uri.UriSchemeHttps,
                StringComparison.OrdinalIgnoreCase))
        {
            throw new InvalidOperationException(
                "Stripe returned an invalid Checkout redirect URI.");
        }

        return new BillingCheckoutProviderSession(session.Id, redirectUri);
    }

    public async Task<BillingCheckoutProviderSessionState> GetSessionStateAsync(
        string externalSessionId,
        CancellationToken cancellationToken)
    {
        if (string.IsNullOrWhiteSpace(externalSessionId)
            || externalSessionId.Length > BillingOperation.MaximumExternalIdentifierLength)
        {
            throw new ArgumentException(
                "A valid external Checkout session identifier is required.",
                nameof(externalSessionId));
        }

        Session session;
        try
        {
            session = await _sessions.GetAsync(
                externalSessionId,
                options: null,
                requestOptions: null,
                cancellationToken);
        }
        catch (TaskCanceledException exception) when (!cancellationToken.IsCancellationRequested)
        {
            throw ProviderUnavailable(exception);
        }
        catch (HttpRequestException exception)
        {
            throw ProviderUnavailable(exception);
        }
        catch (StripeException exception)
        {
            throw ProviderUnavailable(exception);
        }

        if (!string.Equals(session.Id, externalSessionId, StringComparison.Ordinal))
        {
            throw new InvalidOperationException(
                "Stripe returned a different Checkout session than the one requested.");
        }

        return session.Status switch
        {
            "open" => BillingCheckoutProviderSessionState.Open,
            "complete" => BillingCheckoutProviderSessionState.Complete(
                session.SubscriptionId
                    ?? throw new InvalidOperationException(
                        "A completed Stripe Checkout session has no subscription.")),
            "expired" => BillingCheckoutProviderSessionState.Expired,
            _ => throw new InvalidOperationException(
                "Stripe returned an unsupported Checkout session status."),
        };
    }

    private static BillingCheckoutProviderUnavailableException ProviderUnavailable(
        Exception exception)
    {
        return new("Stripe Checkout is temporarily unavailable.", exception);
    }
}
