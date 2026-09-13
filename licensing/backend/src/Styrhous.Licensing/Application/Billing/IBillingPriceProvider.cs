namespace Styrhous.Licensing.Application.Billing;

public sealed record BillingPrice(
    string Cadence,
    decimal UnitAmount,
    string Currency,
    bool? TaxIncluded);

public interface IBillingPriceProvider
{
    Task<IReadOnlyList<BillingPrice>> ListAsync(CancellationToken cancellationToken);
}

public sealed class BillingPricesUnavailableException(Exception innerException)
    : Exception("Subscription prices are unavailable.", innerException);
