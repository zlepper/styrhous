using Styrhous.Licensing.Domain.Billing;

namespace Styrhous.Licensing.Application.Entitlements;

public sealed record CommercialSeatEntitlementSource(
    CommercialSubscriptionStatus Status,
    DateTimeOffset PeriodStartedAt,
    DateTimeOffset PeriodEndsAt,
    bool CancelsAtPeriodEnd,
    bool IsSeatFunded);

public sealed record SeatEntitlementSource(
    Guid SeatId,
    Guid BillingAccountId,
    DateTimeOffset? TrialStartedAt,
    DateTimeOffset? TrialEndsAt,
    CommercialSeatEntitlementSource? Commercial = null,
    bool ProductAccessEnabled = true);
