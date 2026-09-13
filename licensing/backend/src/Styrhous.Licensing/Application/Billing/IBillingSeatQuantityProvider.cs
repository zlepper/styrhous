namespace Styrhous.Licensing.Application.Billing;

public sealed record BillingSeatQuantityProviderRequest(
    Guid BillingOperationId,
    Guid BillingAccountId,
    string ExternalSubscriptionId,
    int PreviousSeatQuantity,
    int SeatQuantity);

public enum BillingSeatQuantityProviderStatus
{
    MutationRequired,
    Applied,
    Superseded,
}

public sealed record BillingSeatQuantityProviderResult(
    BillingSeatQuantityProviderStatus Status,
    AuthoritativeCommercialSubscription Subscription);

public interface IBillingSeatQuantityProvider
{
    Task<BillingSeatQuantityProviderResult> ObserveAsync(
        BillingSeatQuantityProviderRequest request,
        CancellationToken cancellationToken);

    Task<BillingSeatQuantityProviderResult> ApplyAsync(
        BillingSeatQuantityProviderRequest request,
        DateTimeOffset automaticReplayEndsAt,
        CancellationToken cancellationToken);
}

public sealed class BillingSeatQuantityProviderUnavailableException : Exception
{
    public BillingSeatQuantityProviderUnavailableException(string message)
        : base(message)
    {
    }

    public BillingSeatQuantityProviderUnavailableException(
        string message,
        Exception innerException)
        : base(message, innerException)
    {
    }
}

public sealed class BillingSeatQuantityProviderRejectedException : Exception
{
    public BillingSeatQuantityProviderRejectedException(string message)
        : base(message)
    {
    }

    public BillingSeatQuantityProviderRejectedException(
        string message,
        Exception innerException)
        : base(message, innerException)
    {
    }
}

public sealed class BillingSeatQuantityProviderIndeterminateException : Exception
{
    public BillingSeatQuantityProviderIndeterminateException(
        string message,
        Exception innerException)
        : base(message, innerException)
    {
    }
}
