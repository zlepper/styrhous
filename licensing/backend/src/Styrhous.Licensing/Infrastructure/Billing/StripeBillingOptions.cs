namespace Styrhous.Licensing.Infrastructure.Billing;

using Styrhous.Licensing.Domain.Billing;

public sealed record StripeBillingOptions
{
    public const string SectionName = "Stripe";

    public string SecretKey { get; init; } = string.Empty;

    public string WebhookSecret { get; init; } = string.Empty;

    public string MonthlyPriceId { get; init; } = string.Empty;

    public string AnnualPriceId { get; init; } = string.Empty;

    public string CheckoutSuccessUrl { get; init; } = string.Empty;

    public string CheckoutCancelUrl { get; init; } = string.Empty;

    public string CustomerPortalConfigurationId { get; init; } = string.Empty;

    public string CustomerPortalReturnUrl { get; init; } = string.Empty;

    public static bool IsCanonicalPriceId(string value)
    {
        return !string.IsNullOrWhiteSpace(value)
        && string.Equals(value, value.Trim(), StringComparison.Ordinal);
    }

    public static string RequireCanonicalPriceId(
        string value,
        string configurationName)
    {
        if (!IsCanonicalPriceId(value))
        {
            throw new InvalidOperationException(
                $"{SectionName}:{configurationName} is required and cannot contain "
                    + "surrounding whitespace.");
        }

        return value;
    }

    public static bool IsCanonicalExternalIdentifier(string value)
    {
        return !string.IsNullOrWhiteSpace(value)
        && value.Length <= CommercialSubscription.MaximumExternalIdentifierLength
        && string.Equals(value, value.Trim(), StringComparison.Ordinal);
    }

    public static string RequireCanonicalExternalIdentifier(
        string value,
        string configurationName)
    {
        if (!IsCanonicalExternalIdentifier(value))
        {
            throw new InvalidOperationException(
                $"{SectionName}:{configurationName} is required, bounded, and cannot contain "
                    + "surrounding whitespace.");
        }

        return value;
    }

    public static bool IsAbsoluteHttpsUri(string value)
    {
        return Uri.TryCreate(value, UriKind.Absolute, out var uri)
        && string.Equals(
            uri.Scheme,
            Uri.UriSchemeHttps,
            StringComparison.OrdinalIgnoreCase);
    }

    public static Uri RequireAbsoluteHttpsUri(
        string value,
        string configurationName)
    {
        if (!IsAbsoluteHttpsUri(value))
        {
            throw new InvalidOperationException(
                $"{SectionName}:{configurationName} must be an absolute HTTPS URI.");
        }

        return new Uri(value, UriKind.Absolute);
    }

    public (string Monthly, string Annual) GetValidatedPriceIds()
    {
        var monthly = RequireCanonicalPriceId(MonthlyPriceId, nameof(MonthlyPriceId));
        var annual = RequireCanonicalPriceId(AnnualPriceId, nameof(AnnualPriceId));
        if (string.Equals(monthly, annual, StringComparison.Ordinal))
        {
            throw new InvalidOperationException(
                "Stripe monthly and annual Price identifiers must be distinct.");
        }

        return (monthly, annual);
    }
}
