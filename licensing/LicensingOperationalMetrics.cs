namespace Styrhous.Licensing.Operations;

public static class LicensingOperationalMetrics
{
    public const int StripeWebhookFailureEventId = 1001;
    public const int OldestOutboxAgeEventId = 1002;
    public const int StripeWebhookRejectedEventId = 1003;
    public const string OldestOutboxAgeStateName = "OldestOutboxAgeSeconds";
}
