using Microsoft.Extensions.Options;
using Stripe;
using Styrhous.Licensing.Application.Billing;

namespace Styrhous.Licensing.Infrastructure.Billing;

public sealed class StripeBillingPriceProvider(
    IStripeClient stripeClient,
    IOptions<StripeBillingOptions> options) : IBillingPriceProvider
{
    private readonly PriceService _prices = new(stripeClient);
    private readonly StripeBillingOptions _options = options.Value;

    public async Task<IReadOnlyList<BillingPrice>> ListAsync(CancellationToken cancellationToken)
    {
        try
        {
            var (monthly, annual) = _options.GetValidatedPriceIds();
            return await Task.WhenAll(
                ReadAsync(monthly, "monthly", "month", cancellationToken),
                ReadAsync(annual, "annual", "year", cancellationToken));
        }
        catch (Exception exception) when (
            exception is StripeException or HttpRequestException or InvalidOperationException
            || (exception is OperationCanceledException && !cancellationToken.IsCancellationRequested))
        {
            throw new BillingPricesUnavailableException(exception);
        }
    }

    private async Task<BillingPrice> ReadAsync(
        string id,
        string cadence,
        string interval,
        CancellationToken cancellationToken)
    {
        var price = await _prices.GetAsync(id, cancellationToken: cancellationToken);
        if (!price.Active
            || price.BillingScheme != "per_unit"
            || price.TransformQuantity is not null
            || price.Recurring?.Interval != interval
            || price.Recurring.IntervalCount != 1
            || price.Recurring.UsageType != "licensed"
            || price.UnitAmountDecimal is not decimal amount
            || amount < 0
            || price.Currency is not { Length: 3 }
            || !price.Currency.All(char.IsAsciiLetter))
        {
            throw new InvalidOperationException("Configured price is not a fixed recurring seat price.");
        }

        var currency = price.Currency.ToUpperInvariant();
        // Stripe's API units differ from ISO currency digits for ISK and UGX.
        // https://docs.stripe.com/currencies#zero-decimal
        var divisor = currency is "BIF" or "CLP" or "DJF" or "GNF" or "JPY" or "KMF"
            or "KRW" or "MGA" or "PYG" or "RWF" or "VND" or "VUV" or "XAF" or "XOF" or "XPF"
            ? 1m : 100m;
        return new BillingPrice(cadence, amount / divisor, currency, price.TaxBehavior switch { "inclusive" => true, "exclusive" => false, _ => (bool?)null });
    }
}
