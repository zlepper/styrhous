using Styrhous.Licensing.Domain.Billing;

namespace Styrhous.Licensing.Application.Billing;

public sealed record BillingCheckoutProviderRequest(
    Guid BillingOperationId,
    Guid BillingAccountId,
    BillingCadence Cadence,
    int SeatQuantity,
    DateTimeOffset ExpiresAt,
    string? ExternalCustomerId = null);

public sealed record BillingCheckoutProviderSession(
    string ExternalSessionId,
    Uri RedirectUri);

public enum BillingCheckoutProviderSessionStatus
{
    Open,
    Complete,
    Expired,
}

public sealed record BillingCheckoutProviderSessionState
{
    private BillingCheckoutProviderSessionState(
        BillingCheckoutProviderSessionStatus status,
        string? externalSubscriptionId)
    {
        Status = status;
        ExternalSubscriptionId = externalSubscriptionId;
    }

    public BillingCheckoutProviderSessionStatus Status { get; }

    public string? ExternalSubscriptionId { get; }

    public static BillingCheckoutProviderSessionState Open { get; } =
        new(BillingCheckoutProviderSessionStatus.Open, externalSubscriptionId: null);

    public static BillingCheckoutProviderSessionState Expired { get; } =
        new(BillingCheckoutProviderSessionStatus.Expired, externalSubscriptionId: null);

    public static BillingCheckoutProviderSessionState Complete(
        string externalSubscriptionId)
    {
        if (string.IsNullOrWhiteSpace(externalSubscriptionId)
            || externalSubscriptionId.Length > BillingOperation.MaximumExternalIdentifierLength)
        {
            throw new InvalidOperationException(
                "A completed Checkout session requires a valid subscription identifier.");
        }

        return new BillingCheckoutProviderSessionState(
            BillingCheckoutProviderSessionStatus.Complete,
            externalSubscriptionId);
    }
}

public interface IBillingCheckoutProvider
{
    Task<BillingCheckoutProviderSession> CreateSessionAsync(
        BillingCheckoutProviderRequest request,
        CancellationToken cancellationToken);

    Task<BillingCheckoutProviderSessionState> GetSessionStateAsync(
        string externalSessionId,
        CancellationToken cancellationToken);
}

public sealed class BillingCheckoutProviderUnavailableException : Exception
{
    public BillingCheckoutProviderUnavailableException(string message)
        : base(message)
    {
    }

    public BillingCheckoutProviderUnavailableException(string message, Exception innerException)
        : base(message, innerException)
    {
    }
}

public sealed class BillingCheckoutProviderSessionCreationRejectedException : Exception
{
    public BillingCheckoutProviderSessionCreationRejectedException(string message)
        : base(message)
    {
    }

    public BillingCheckoutProviderSessionCreationRejectedException(
        string message,
        Exception innerException)
        : base(message, innerException)
    {
    }
}
