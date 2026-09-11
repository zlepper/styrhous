using Styrhous.Licensing.Application.Billing;
using Styrhous.Licensing.Api.Entitlements;
using Styrhous.Licensing.Api.Organizations;
using Styrhous.Licensing.Domain.Accounts;
using Styrhous.Licensing.Domain.Billing;

namespace Styrhous.Licensing.Api.Billing;

public sealed record BillingAccountListResponse(
    string ReasonCode,
    IReadOnlyList<BillingAccountSummaryResponse> Accounts)
{
    internal static BillingAccountListResponse From(
        IReadOnlyList<BillingAccountSummary> accounts)
    {
        return new(
            BillingAccountReasonCodes.Listed,
            accounts.Select(BillingAccountSummaryResponse.From).ToArray());
    }
}

public sealed record BillingAccountSummaryResponse(
    Guid BillingAccountId,
    string AccountKind,
    Guid? OrganizationId,
    string? OrganizationName,
    string? OrganizationRole,
    bool CanManageBilling,
    int AssignedSeatCount,
    SeatEntitlementResponse Entitlement,
    BillingTrialSummaryResponse? Trial,
    BillingSubscriptionSummaryResponse? Subscription,
    bool CanStartCheckout)
{
    internal static BillingAccountSummaryResponse From(BillingAccountSummary account)
    {
        return new(
            account.BillingAccountId,
            AccountKindName(account.AccountKind),
            account.OrganizationId,
            account.OrganizationName,
            account.OrganizationRole is null
                ? null
                : OrganizationRoleNames.ToApiValue(account.OrganizationRole.Value),
            account.CanManageBilling,
            account.AssignedSeatCount,
            SeatEntitlementResponse.From(account.Entitlement),
            account.Trial is null ? null : BillingTrialSummaryResponse.From(account.Trial),
            account.Subscription is null
                ? null
                : BillingSubscriptionSummaryResponse.From(account.Subscription),
            account.CanManageBilling && BillingCheckoutEligibility.AllowsPurchase(account.Subscription?.Status));
    }

    private static string AccountKindName(BillingAccountKind kind)
    {
        return kind switch
        {
            BillingAccountKind.Personal => "personal",
            BillingAccountKind.Organization => "organization",
            _ => throw new ArgumentOutOfRangeException(
                nameof(kind),
                kind,
                "The billing account kind is not supported."),
        };
    }
}

public sealed record BillingTrialSummaryResponse(
    Guid TrialId,
    DateTimeOffset StartedAt,
    DateTimeOffset EndsAt,
    DateTimeOffset? TerminatedAt,
    DateTimeOffset? TransferredAt,
    bool IsActive)
{
    internal static BillingTrialSummaryResponse From(BillingTrialSummary trial)
    {
        return new(
            trial.TrialId,
            trial.StartedAt,
            trial.EndsAt,
            trial.TerminatedAt,
            trial.TransferredAt,
            trial.IsActive);
    }
}

public sealed record BillingSubscriptionSummaryResponse(
    Guid SubscriptionId,
    string Status,
    int SeatQuantity,
    bool CancelAtPeriodEnd,
    DateTimeOffset CurrentPeriodStartedAt,
    DateTimeOffset CurrentPeriodEndsAt,
    DateTimeOffset ProjectedAt)
{
    internal static BillingSubscriptionSummaryResponse From(
        BillingSubscriptionSummary subscription)
    {
        return new(
            subscription.SubscriptionId,
            StatusName(subscription.Status),
            subscription.SeatQuantity,
            subscription.CancelAtPeriodEnd,
            subscription.CurrentPeriodStartedAt,
            subscription.CurrentPeriodEndsAt,
            subscription.ProjectedAt);
    }

    private static string StatusName(CommercialSubscriptionStatus status)
    {
        return status switch
        {
            CommercialSubscriptionStatus.Active => "active",
            CommercialSubscriptionStatus.PastDue => "past_due",
            CommercialSubscriptionStatus.Unpaid => "unpaid",
            CommercialSubscriptionStatus.Paused => "paused",
            CommercialSubscriptionStatus.Incomplete => "incomplete",
            CommercialSubscriptionStatus.IncompleteExpired => "incomplete_expired",
            CommercialSubscriptionStatus.Trialing => "trialing",
            CommercialSubscriptionStatus.Canceled => "canceled",
            _ => throw new ArgumentOutOfRangeException(
                nameof(status),
                status,
                "The commercial subscription status is not supported."),
        };
    }
}
