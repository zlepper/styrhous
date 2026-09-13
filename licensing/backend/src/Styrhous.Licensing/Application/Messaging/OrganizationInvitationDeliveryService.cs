using Styrhous.Licensing.Persistence;

namespace Styrhous.Licensing.Application.Messaging;

public sealed class OrganizationInvitationDeliveryService(
    PostgresOrganizationInvitationDeliveryStore deliveryStore,
    IOrganizationInvitationEmailSender emailSender,
    TimeProvider timeProvider)
{
    private static readonly TimeSpan ProcessingLeaseDuration = TimeSpan.FromMinutes(5);

    private static readonly TimeSpan LeaseReleaseTimeout = TimeSpan.FromSeconds(5);

    public async Task<OrganizationInvitationDeliveryProcessingResult> DeliverAsync(
        Guid outboxMessageId,
        CancellationToken cancellationToken = default)
    {

        var leaseId = Guid.CreateVersion7();
        var claimResult = await deliveryStore.TryAcquireAsync(
            outboxMessageId,
            leaseId,
            ProcessingLeaseDuration,
            cancellationToken);
        if (claimResult.Status != OrganizationInvitationDeliveryClaimStatus.Acquired)
        {
            return new OrganizationInvitationDeliveryProcessingResult(
                MapClaimStatus(claimResult.Status));
        }

        var claim = claimResult.Claim
            ?? throw new InvalidOperationException(
                "An acquired invitation delivery claim is required.");
        if (timeProvider.GetUtcNow() >= claim.Delivery.ExpiresAt)
        {
            await ReleaseAsync(claim);
            return new OrganizationInvitationDeliveryProcessingResult(
                OrganizationInvitationDeliveryProcessingStatus.Expired);
        }

        var releaseAttempted = false;
        try
        {
            await emailSender.SendAsync(
                claim.OutboxMessageId,
                claim.Delivery,
                cancellationToken);
            var deliveredAt = timeProvider.GetUtcNow();
            if (deliveredAt >= claim.Delivery.ExpiresAt)
            {
                releaseAttempted = true;
                await ReleaseAsync(claim);
                return new OrganizationInvitationDeliveryProcessingResult(
                    OrganizationInvitationDeliveryProcessingStatus.Expired);
            }

            if (!await deliveryStore.CompleteAsync(
                    claim.OutboxMessageId,
                    claim.LeaseId,
                    deliveredAt,
                    cancellationToken))
            {
                throw new InvalidOperationException(
                    "The invitation delivery processing lease was lost before completion.");
            }

            return new OrganizationInvitationDeliveryProcessingResult(
                OrganizationInvitationDeliveryProcessingStatus.Delivered);
        }
        catch (Exception deliveryException)
        {
            if (releaseAttempted)
            {
                throw;
            }

            try
            {
                await ReleaseAsync(claim);
            }
            catch (Exception releaseException)
            {
                throw new AggregateException(
                    "Invitation delivery and lease release both failed.",
                    deliveryException,
                    releaseException);
            }

            throw;
        }
    }

    private static OrganizationInvitationDeliveryProcessingStatus MapClaimStatus(
        OrganizationInvitationDeliveryClaimStatus status)
    {
        return status switch
        {
            OrganizationInvitationDeliveryClaimStatus.NotFound =>
                OrganizationInvitationDeliveryProcessingStatus.NotFound,
            OrganizationInvitationDeliveryClaimStatus.AlreadyDelivered =>
                OrganizationInvitationDeliveryProcessingStatus.AlreadyDelivered,
            OrganizationInvitationDeliveryClaimStatus.Discarded =>
                OrganizationInvitationDeliveryProcessingStatus.Discarded,
            OrganizationInvitationDeliveryClaimStatus.Undeliverable =>
                OrganizationInvitationDeliveryProcessingStatus.Undeliverable,
            OrganizationInvitationDeliveryClaimStatus.Expired =>
                OrganizationInvitationDeliveryProcessingStatus.Expired,
            OrganizationInvitationDeliveryClaimStatus.Busy =>
                OrganizationInvitationDeliveryProcessingStatus.Busy,
            _ => throw new ArgumentOutOfRangeException(nameof(status)),
        };
    }

    private async Task ReleaseAsync(OrganizationInvitationDeliveryClaim claim)
    {
        using var releaseTimeout = new CancellationTokenSource(LeaseReleaseTimeout);
        await deliveryStore.ReleaseAsync(
            claim.OutboxMessageId,
            claim.LeaseId,
            releaseTimeout.Token);
    }
}
