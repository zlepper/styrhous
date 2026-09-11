namespace Styrhous.Licensing.Application.Billing;

public sealed record BillingCustomerPortalProviderSession(Uri RedirectUri);

public interface IBillingCustomerPortalProvider
{
    Task<BillingCustomerPortalProviderSession> CreateSessionAsync(
        string externalCustomerId,
        CancellationToken cancellationToken);
}

public sealed class BillingCustomerPortalProviderUnavailableException : Exception
{
    public BillingCustomerPortalProviderUnavailableException(string message)
        : base(message)
    {
    }

    public BillingCustomerPortalProviderUnavailableException(
        string message,
        Exception innerException)
        : base(message, innerException)
    {
    }
}
