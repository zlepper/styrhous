namespace Styrhous.Licensing.Application.Billing;



public enum BillingWebhookInboxDisposition
{
    PendingProcessing,
    Ignored,
}

public enum BillingWebhookInboxStoreStatus
{
    Received,
    Duplicate,
}

public sealed record BillingWebhookInboxStoreResult(
    BillingWebhookInboxStoreStatus Status,
    Guid InboxEventId);
