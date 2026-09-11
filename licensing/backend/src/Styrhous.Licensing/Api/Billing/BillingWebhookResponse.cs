namespace Styrhous.Licensing.Api.Billing;

public sealed record BillingWebhookResponse(string ReasonCode, Guid? InboxEventId);
