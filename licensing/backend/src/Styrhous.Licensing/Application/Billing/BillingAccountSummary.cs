using Styrhous.Licensing.Domain.Accounts;
using Styrhous.Licensing.Domain.Billing;
using Styrhous.Licensing.Domain.Organizations;
using Styrhous.Licensing.Application.Entitlements;

namespace Styrhous.Licensing.Application.Billing;

public sealed record BillingAccountSummary(
    Guid BillingAccountId,
    BillingAccountKind AccountKind,
    Guid? OrganizationId,
    string? OrganizationName,
    OrganizationRole? OrganizationRole,
    int AssignedSeatCount,
    SeatEntitlement Entitlement,
    BillingTrialSummary? Trial,
    BillingSubscriptionSummary? Subscription)
{
    public bool CanManageBilling =>
        AccountKind == BillingAccountKind.Personal
        || OrganizationRole == Domain.Organizations.OrganizationRole.Owner;
}

public sealed record BillingTrialSummary(
    Guid TrialId,
    DateTimeOffset StartedAt,
    DateTimeOffset EndsAt,
    DateTimeOffset? TerminatedAt,
    DateTimeOffset? TransferredAt,
    bool IsActive);

public sealed record BillingSubscriptionSummary(
    Guid SubscriptionId,
    CommercialSubscriptionStatus Status,
    int SeatQuantity,
    bool CancelAtPeriodEnd,
    DateTimeOffset CurrentPeriodStartedAt,
    DateTimeOffset CurrentPeriodEndsAt,
    DateTimeOffset ProjectedAt);
