using System.Text.Json;
using Microsoft.Extensions.Options;
using Stripe;
using Styrhous.Licensing.Application.Billing;
using Styrhous.Licensing.Domain.Billing;

namespace Styrhous.Licensing.Infrastructure.Billing;

public sealed class StripeBillingWebhookVerifier(
    IOptions<StripeBillingOptions> options,
    TimeProvider timeProvider)
    : IBillingWebhookVerifier
{
    private const long SignatureToleranceSeconds = 300;

    private readonly StripeBillingOptions _options = options.Value;

    public BillingWebhookVerificationResult Verify(string payload, string signature)
    {
        if (string.IsNullOrWhiteSpace(_options.WebhookSecret))
        {
            throw new InvalidOperationException(
                $"{StripeBillingOptions.SectionName}:WebhookSecret is required.");
        }

        if (string.IsNullOrEmpty(payload) || string.IsNullOrWhiteSpace(signature))
        {
            return BillingWebhookVerificationResult.InvalidSignature();
        }

        try
        {
            EventUtility.ValidateSignature(
                payload,
                signature,
                _options.WebhookSecret,
                SignatureToleranceSeconds,
                timeProvider.GetUtcNow().ToUnixTimeSeconds());
        }
        catch (StripeException)
        {
            return BillingWebhookVerificationResult.InvalidSignature();
        }

        try
        {
            using var document = JsonDocument.Parse(payload);
            var root = document.RootElement;
            if (root.ValueKind != JsonValueKind.Object
                || !TryReadRequiredText(
                    root,
                    "id",
                    BillingWebhookEvent.MaximumExternalIdentifierLength,
                    out var externalEventId)
                || !TryReadRequiredText(
                    root,
                    "type",
                    BillingWebhookEvent.MaximumEventTypeLength,
                    out var eventType)
                || !root.TryGetProperty("created", out var created)
                || created.ValueKind != JsonValueKind.Number
                || !created.TryGetInt64(out var createdUnixSeconds))
            {
                return BillingWebhookVerificationResult.InvalidPayload();
            }

            DateTimeOffset occurredAt;
            try
            {
                occurredAt = DateTimeOffset.FromUnixTimeSeconds(createdUnixSeconds);
            }
            catch (ArgumentOutOfRangeException)
            {
                return BillingWebhookVerificationResult.InvalidPayload();
            }

            return BillingWebhookVerificationResult.Verified(
                new VerifiedBillingWebhookEvent(
                    externalEventId,
                    eventType,
                    StripeBillingWebhookEventTypes.ToKind(eventType),
                    occurredAt));
        }
        catch (JsonException)
        {
            return BillingWebhookVerificationResult.InvalidPayload();
        }
    }

    private static bool TryReadRequiredText(
        JsonElement root,
        string propertyName,
        int maximumLength,
        out string value)
    {
        value = string.Empty;
        if (!root.TryGetProperty(propertyName, out var property)
            || property.ValueKind != JsonValueKind.String)
        {
            return false;
        }

        var candidate = property.GetString();
        if (string.IsNullOrWhiteSpace(candidate)
            || candidate.Length > maximumLength
            || !string.Equals(candidate, candidate.Trim(), StringComparison.Ordinal))
        {
            return false;
        }

        value = candidate;
        return true;
    }
}
