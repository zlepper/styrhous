namespace Styrhous.Licensing.Application.Messaging;

public enum OrganizationInvitationDeliveryClaimStatus
{
    Acquired,
    NotFound,
    AlreadyDelivered,
    Discarded,
    Undeliverable,
    Expired,
    Busy,
}

public sealed record OrganizationInvitationDeliveryClaim(
    Guid OutboxMessageId,
    Guid LeaseId,
    OrganizationInvitationDelivery Delivery);

public sealed record OrganizationInvitationDeliveryClaimResult(
    OrganizationInvitationDeliveryClaimStatus Status,
    OrganizationInvitationDeliveryClaim? Claim);

public enum OrganizationInvitationDeliveryProcessingStatus
{
    Delivered,
    NotFound,
    AlreadyDelivered,
    Discarded,
    Undeliverable,
    Expired,
    Busy,
}

public sealed record OrganizationInvitationDeliveryProcessingResult(
    OrganizationInvitationDeliveryProcessingStatus Status);
