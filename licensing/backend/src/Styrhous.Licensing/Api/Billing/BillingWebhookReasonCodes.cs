namespace Styrhous.Licensing.Api.Billing;

public static class BillingWebhookReasonCodes
{
    public const string Received = "billing_webhook_received";

    public const string Duplicate = "billing_webhook_duplicate";

    public const string Ignored = "billing_webhook_ignored";

    public const string InvalidSignature = "billing_webhook_invalid_signature";

    public const string InvalidPayload = "billing_webhook_invalid_payload";

    public const string PayloadTooLarge = "billing_webhook_payload_too_large";
}
