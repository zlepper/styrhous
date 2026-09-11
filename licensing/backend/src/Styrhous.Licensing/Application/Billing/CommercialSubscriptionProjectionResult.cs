namespace Styrhous.Licensing.Application.Billing;

public enum CommercialSubscriptionProjectionStatus
{
    Created,
    Updated,
    Ignored,
    CausalConflict,
    BillingAccountNotFound,
}

public sealed record CommercialSubscriptionProjectionResult(
    CommercialSubscriptionProjectionStatus Status,
    Guid? SubscriptionId);
